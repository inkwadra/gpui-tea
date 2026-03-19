use crate::command::{CommandInner, Effect};
use crate::observability::{ProgramConfig, RuntimeEvent};
use crate::runtime::{CommandMeta, KeyedEffectContext, Runtime};
use crate::{Command, CommandKind, Dispatcher, Key, Subscriptions, View};
use futures::StreamExt;
use gpui::{App, AppContext, AsyncApp, Context, Entity, IntoElement, Render, Task, Window};

/// Define the application state and transition contract for a [`Program`].
///
/// A model owns the current state, updates that state in response to messages,
/// renders GPUI elements, and declares any long-lived subscriptions needed for
/// the current state.
pub trait Model: Sized + 'static {
    /// Name the message type consumed by [`Model::update`] and produced by
    /// commands and subscriptions.
    type Msg: Send + 'static;

    /// Initialize the model and return an initial command.
    ///
    /// [`Program::mount`] and [`Program::mount_with`] call this exactly once
    /// after creating the runtime and before the initial subscription
    /// reconciliation.
    fn init(&mut self, _cx: &mut App) -> Command<Self::Msg> {
        Command::none()
    }

    /// Update the model in response to a single message.
    ///
    /// The runtime processes messages in FIFO order. If an update emits or
    /// dispatches more messages, they are appended to the queue and processed
    /// later instead of re-entering `update` recursively.
    fn update(&mut self, msg: Self::Msg, cx: &mut App) -> Command<Self::Msg>;

    /// Render the current model state as GPUI elements.
    ///
    /// Use `dispatcher` inside event handlers to enqueue follow-up messages back
    /// into the same program instance.
    fn view(&self, window: &mut Window, cx: &mut App, dispatcher: &Dispatcher<Self::Msg>) -> View;

    /// Return the declarative subscriptions required by the current model state.
    ///
    /// The runtime reconciles this set after `init` and after each queue-drain
    /// pass that processed at least one message.
    fn subscriptions(&self, _cx: &mut App) -> Subscriptions<Self::Msg> {
        Subscriptions::none()
    }
}

/// Host a [`Model`] inside a GPUI entity and drive the TEA runtime.
///
/// A program owns the model, message queue, keyed task tracking, subscription
/// registry, and runtime configuration for a single mounted instance.
pub struct Program<M: Model> {
    model: M,
    runtime: Runtime<M::Msg>,
}

impl<M: Model> Program<M> {
    fn build_runtime(config: ProgramConfig<M::Msg>, cx: &mut Context<'_, Self>) -> Runtime<M::Msg> {
        let (sender, mut receiver) = futures::channel::mpsc::unbounded();
        let dispatcher = Dispatcher::new(sender, config.clone());
        let program = cx.weak_entity();
        let receive_task = cx.spawn(move |_this, async_cx: &mut AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                while let Some(message) = receiver.next().await {
                    program
                        .update(&mut async_cx, |program, cx| {
                            program.dispatch(message, cx);
                        })
                        .ok();
                }
            }
        });

        Runtime::new(dispatcher, receive_task, config)
    }

    /// Create, initialize, and mount a program entity with the default
    /// configuration.
    ///
    /// The runtime is created first, then [`Model::init`] runs, the returned
    /// command executes, and finally the initial subscription set is
    /// reconciled.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use gpui::{App, Window, div};
    /// use gpui_tea::{Command, Dispatcher, IntoView, Model, Program, View};
    ///
    /// #[derive(Clone, Copy)]
    /// enum Msg {
    ///     Increment,
    ///     Decrement,
    ///     Reset,
    /// }
    ///
    /// struct Counter {
    ///     value: i32,
    /// }
    ///
    /// impl Model for Counter {
    ///     type Msg = Msg;
    ///
    ///     fn init(&mut self, _cx: &mut App) -> Command<Self::Msg> {
    ///         Command::none()
    ///     }
    ///
    ///     fn update(&mut self, msg: Self::Msg, _cx: &mut App) -> Command<Self::Msg> {
    ///         match msg {
    ///             Msg::Increment => {
    ///                 self.value += 1;
    ///                 Command::none()
    ///             }
    ///             Msg::Decrement => {
    ///                 self.value -= 1;
    ///                 Command::none()
    ///             }
    ///             Msg::Reset => {
    ///                 self.value = 0;
    ///                 Command::none()
    ///             }
    ///         }
    ///     }
    ///
    ///     fn view(
    ///         &self,
    ///         _window: &mut Window,
    ///         _cx: &mut App,
    ///         _dispatcher: &Dispatcher<Self::Msg>,
    ///     ) -> View {
    ///         div().into_view()
    ///     }
    /// }
    ///
    /// # fn mount(cx: &mut App) {
    /// let _program = Program::mount(Counter { value: 0 }, cx);
    /// # }
    /// ```
    pub fn mount(model: M, cx: &mut App) -> Entity<Self> {
        Self::mount_with(model, ProgramConfig::default(), cx)
    }

    /// Create, initialize, and mount a program entity with an explicit
    /// configuration.
    ///
    /// This follows the same lifecycle as [`Program::mount`] while also
    /// applying the supplied [`ProgramConfig`].
    pub fn mount_with(model: M, config: ProgramConfig<M::Msg>, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let runtime = Self::build_runtime(config, cx);
            let mut program = Self { model, runtime };
            let init_command = program.model.init(cx);
            program.execute_command(init_command, cx);
            program.reconcile_subscriptions(cx);
            program
        })
    }

    /// Borrow the underlying model.
    pub fn model(&self) -> &M {
        &self.model
    }

    /// Get an enqueue-only handle for sending messages to this program.
    ///
    /// Cloning the returned dispatcher is cheap because it shares the same
    /// underlying queue handle.
    pub fn dispatcher(&self) -> Dispatcher<M::Msg> {
        self.runtime.dispatcher.clone()
    }

    pub(crate) fn dispatch(&mut self, message: M::Msg, cx: &mut Context<'_, Self>) {
        self.runtime.queue.pending.push_back(message);
        self.observe_queue_warning_if_needed();
        self.drain_queue(cx);
    }

    fn observe_queue_warning_if_needed(&self) {
        if let Some(threshold) = self.runtime.config.queue_warning_threshold {
            let queued = self.runtime.queue.pending.len();
            if queued > threshold {
                self.runtime
                    .config
                    .observe(RuntimeEvent::QueueWarning { queued, threshold });
            }
        }
    }

    fn drain_queue(&mut self, cx: &mut Context<'_, Self>) {
        // Re-entrant dispatch appends to `pending`; only the outermost caller
        // performs the drain.
        if self.runtime.queue.is_draining {
            return;
        }

        self.runtime.queue.is_draining = true;
        let queued = self.runtime.queue.pending.len();
        self.runtime
            .config
            .observe(RuntimeEvent::QueueDrainStarted { queued });

        let mut processed = 0;

        while let Some(message) = self.runtime.queue.pending.pop_front() {
            let message_description = self.runtime.config.describe_message_value(&message);
            self.runtime.config.observe(RuntimeEvent::MessageProcessed {
                message: &message,
                message_description,
            });

            let command = self.model.update(message, cx);
            processed += 1;
            self.execute_command(command, cx);
        }

        self.runtime.queue.is_draining = false;

        if processed > 0 {
            self.reconcile_subscriptions(cx);
            cx.notify();
        }

        self.runtime
            .config
            .observe(RuntimeEvent::QueueDrainFinished {
                processed,
                remaining: self.runtime.queue.pending.len(),
            });
    }

    fn execute_command(&mut self, command: Command<M::Msg>, cx: &mut Context<'_, Self>) {
        let (label, inner) = command.into_parts();

        match inner {
            CommandInner::None => {}
            CommandInner::Emit(message) => {
                let meta = CommandMeta::new(CommandKind::Emit, label);
                self.observe_command_scheduled(&meta, None);
                self.observe_effect_completed(&meta, None, Some(&message));
                self.dispatch(message, cx);
            }
            CommandInner::Batch(commands) => {
                for command in commands {
                    self.execute_command(command, cx);
                }
            }
            CommandInner::Effect(effect) => {
                let meta = CommandMeta::new(effect.kind(), label);
                self.observe_command_scheduled(&meta, None);
                Self::spawn_effect(effect, meta, None, cx).detach();
            }
            CommandInner::Keyed { key, effect } => {
                let meta = CommandMeta::new(effect.kind(), label);
                self.observe_command_scheduled(&meta, Some(&key));
                let generation = self.runtime.tasks.next_generation();
                let keyed = KeyedEffectContext {
                    key: key.clone(),
                    generation,
                };
                let task = Self::spawn_effect(effect, meta.clone(), Some(keyed), cx);
                // Replacing the active entry makes the new generation current
                // and lets any older completion be recognized as stale.
                let previous =
                    self.runtime
                        .tasks
                        .insert(key.clone(), generation, meta.clone(), task);

                if let Some(previous) = previous {
                    self.runtime
                        .config
                        .observe(RuntimeEvent::KeyedCommandReplaced {
                            key: &key,
                            key_description: self.runtime.config.describe_key_value(&key),
                            previous_kind: previous.kind,
                            previous_label: previous.label(),
                            next_kind: meta.kind,
                            next_label: meta.label(),
                        });
                }
            }
        }
    }

    fn observe_command_scheduled(&self, meta: &CommandMeta, key: Option<&Key>) {
        self.runtime.config.observe(RuntimeEvent::CommandScheduled {
            kind: meta.kind,
            label: meta.label(),
            key,
            key_description: key.and_then(|key| self.runtime.config.describe_key_value(key)),
        });
    }

    fn observe_effect_completed(
        &self,
        meta: &CommandMeta,
        key: Option<&Key>,
        message: Option<&M::Msg>,
    ) {
        self.runtime.config.observe(RuntimeEvent::EffectCompleted {
            kind: meta.kind,
            label: meta.label(),
            key,
            key_description: key.and_then(|key| self.runtime.config.describe_key_value(key)),
            emitted_message: message.is_some(),
            message,
            message_description: message
                .and_then(|message| self.runtime.config.describe_message_value(message)),
        });
    }

    fn apply_completion(
        &mut self,
        message: Option<M::Msg>,
        meta: &CommandMeta,
        keyed: Option<KeyedEffectContext>,
        cx: &mut Context<'_, Self>,
    ) {
        let key = keyed.as_ref().map(|keyed| &keyed.key);
        self.observe_effect_completed(meta, key, message.as_ref());

        if let Some(keyed) = keyed {
            if self.runtime.tasks.is_current(&keyed.key, keyed.generation) {
                self.runtime
                    .tasks
                    .clear_current(&keyed.key, keyed.generation);
                if let Some(message) = message {
                    self.dispatch(message, cx);
                }
            } else {
                self.runtime
                    .config
                    .observe(RuntimeEvent::StaleKeyedCompletionIgnored {
                        kind: meta.kind,
                        label: meta.label(),
                        key: &keyed.key,
                        key_description: self.runtime.config.describe_key_value(&keyed.key),
                        emitted_message: message.is_some(),
                        message: message.as_ref(),
                        message_description: message.as_ref().and_then(|message| {
                            self.runtime.config.describe_message_value(message)
                        }),
                    });
            }
        } else if let Some(message) = message {
            self.dispatch(message, cx);
        }
    }

    fn spawn_effect(
        effect: Effect<M::Msg>,
        meta: CommandMeta,
        keyed: Option<KeyedEffectContext>,
        cx: &mut Context<'_, Self>,
    ) -> Task<()> {
        let program = cx.weak_entity();

        match effect {
            Effect::Foreground(effect) => cx.spawn(move |_this, async_cx: &mut AsyncApp| {
                let mut async_cx = async_cx.clone();
                async move {
                    let message = effect(&mut async_cx).await;
                    program
                        .update(&mut async_cx, |program, cx| {
                            program.apply_completion(message, &meta, keyed, cx);
                        })
                        .ok();
                }
            }),
            Effect::Background(effect) => {
                let executor = cx.background_executor().clone();
                cx.spawn(move |_this, async_cx: &mut AsyncApp| {
                    let mut async_cx = async_cx.clone();
                    async move {
                        let spawned_executor = executor.clone();
                        let message = executor.spawn(effect(spawned_executor)).await;
                        program
                            .update(&mut async_cx, |program, cx| {
                                program.apply_completion(message, &meta, keyed, cx);
                            })
                            .ok();
                    }
                })
            }
        }
    }

    fn reconcile_subscriptions(&mut self, cx: &mut Context<'_, Self>) {
        let subscriptions = self.model.subscriptions(cx);
        let dispatcher = self.dispatcher();
        let stats = self.runtime.subscriptions.reconcile(
            subscriptions,
            &dispatcher,
            &self.runtime.config,
            cx,
        );

        self.runtime
            .config
            .observe(RuntimeEvent::SubscriptionsReconciled {
                active: stats.active,
                added: stats.added,
                removed: stats.removed,
                retained: stats.retained,
            });
    }
}

impl<M: Model> Render for Program<M> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let dispatcher = self.dispatcher();
        self.model.view(window, cx, &dispatcher)
    }
}

/// Mount a [`Model`] as a fully initialized [`Program`].
pub trait ModelExt: Model {
    /// Mount this model as a fully initialized program entity.
    ///
    /// This is a convenience wrapper around [`Program::mount`].
    fn into_program(self, cx: &mut App) -> Entity<Program<Self>> {
        Program::mount(self, cx)
    }

    /// Mount this model with an explicit runtime configuration.
    ///
    /// This is a convenience wrapper around [`Program::mount_with`].
    fn into_program_with(
        self,
        config: ProgramConfig<Self::Msg>,
        cx: &mut App,
    ) -> Entity<Program<Self>> {
        Program::mount_with(self, config, cx)
    }
}

impl<M: Model> ModelExt for M {}

#[cfg(test)]
mod runtime_tests {
    use super::*;
    use crate::IntoView;
    use gpui::{Entity, Task, TestAppContext, Window, div};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Debug, PartialEq)]
    enum Msg {
        Set(i32),
    }

    struct TestModel {
        state: i32,
    }

    impl Model for TestModel {
        type Msg = Msg;

        fn update(&mut self, msg: Self::Msg, _cx: &mut App) -> Command<Self::Msg> {
            match msg {
                Msg::Set(value) => {
                    self.state = value;
                    Command::none()
                }
            }
        }

        fn view(
            &self,
            _window: &mut Window,
            _cx: &mut App,
            _dispatcher: &Dispatcher<Self::Msg>,
        ) -> View {
            div().into_view()
        }
    }

    #[allow(clippy::missing_panics_doc)]
    #[gpui::test]
    fn stale_keyed_completion_is_ignored(cx: &mut TestAppContext) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let config = ProgramConfig::default()
            .describe_message(|msg: &Msg| match msg {
                Msg::Set(value) => format!("set:{value}"),
            })
            .describe_key(|key| format!("{key:?}"))
            .observer({
                let events = events.clone();
                move |event: RuntimeEvent<'_, Msg>| {
                    if let RuntimeEvent::StaleKeyedCompletionIgnored {
                        message_description,
                        ..
                    } = event
                    {
                        events
                            .lock()
                            .unwrap()
                            .push(message_description.map(|value| value.to_string()));
                    }
                }
            });

        let program: Entity<Program<TestModel>> =
            cx.update(|cx| TestModel { state: 0 }.into_program_with(config, cx));
        let key = Key::new("stale");

        program.update(cx, |program: &mut Program<TestModel>, cx| {
            let new_generation = program.runtime.tasks.next_generation();
            program.runtime.tasks.insert(
                key.clone(),
                new_generation,
                CommandMeta::new(CommandKind::Foreground, Some(Arc::from("current"))),
                Task::ready(()),
            );

            let stale_meta = CommandMeta::new(CommandKind::Foreground, Some(Arc::from("stale")));
            program.apply_completion(
                Some(Msg::Set(7)),
                &stale_meta,
                Some(KeyedEffectContext {
                    key: key.clone(),
                    generation: new_generation.previous(),
                }),
                cx,
            );
        });

        program.read_with(cx, |program: &Program<TestModel>, _cx| {
            assert_eq!(program.model().state, 0);
        });
        assert_eq!(
            events.lock().unwrap().as_slice(),
            &[Some(String::from("set:7"))]
        );
    }
}
