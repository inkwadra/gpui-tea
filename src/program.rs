use crate::command::{CommandInner, CommandKind, Effect};
use crate::{Command, Dispatcher, Key, SubHandle, SubscriptionContext, Subscriptions};
use futures::StreamExt;
use gpui::{App, AppContext, AsyncApp, Context, Entity, IntoElement, Render, Task, Window};
use std::{collections::HashMap, sync::Arc};

/// Observe [`RuntimeEvent`] values emitted by a mounted [`Program`].
///
/// The runtime invokes observers synchronously at the event site. Keep the
/// callback lightweight and non-blocking so dispatch, queue draining, and
/// effect completion stay responsive.
pub type RuntimeObserver<Msg> = Arc<dyn for<'a> Fn(RuntimeEvent<'a, Msg>) + Send + Sync>;
type DescribeMessageFn<Msg> = Arc<dyn Fn(&Msg) -> Arc<str> + Send + Sync>;
type DescribeKeyFn = Arc<dyn Fn(&Key) -> Arc<str> + Send + Sync>;

/// Report runtime activity from a mounted [`Program`].
///
/// These events describe enqueue attempts, queue draining, command scheduling,
/// keyed task replacement, effect completion, and subscription lifecycle
/// reconciliation. They are emitted only when a [`ProgramConfig`] installs an
/// [`RuntimeObserver`].
pub enum RuntimeEvent<'a, Msg> {
    /// Record that [`Dispatcher::dispatch`] accepted a message.
    DispatchAccepted {
        /// Hold the message description produced by
        /// [`ProgramConfig::describe_message`], if configured.
        message_description: Option<Arc<str>>,
    },
    /// Record that [`Dispatcher::dispatch`] rejected a message because the
    /// program was no longer alive.
    DispatchRejected {
        /// Hold the message description produced by
        /// [`ProgramConfig::describe_message`], if configured.
        message_description: Option<Arc<str>>,
    },
    /// Mark the start of a queue-drain pass.
    QueueDrainStarted {
        /// Report how many messages were pending when draining began.
        queued: usize,
    },
    /// Warn that the in-memory queue exceeded the configured threshold.
    QueueWarning {
        /// Report the number of pending messages at the warning point.
        queued: usize,
        /// Report the threshold configured through
        /// [`ProgramConfig::queue_warning_threshold`].
        threshold: usize,
    },
    /// Record that [`Model::update`] is about to process a message.
    MessageProcessed {
        /// Borrow the message being processed.
        message: &'a Msg,
        /// Hold the message description produced by
        /// [`ProgramConfig::describe_message`], if configured.
        message_description: Option<Arc<str>>,
    },
    /// Record that the runtime scheduled a command for execution.
    CommandScheduled {
        /// Identify whether the scheduled command is synchronous, foreground, or
        /// background work.
        kind: CommandKind,
        /// Hold the label attached with [`Command::label`], if any.
        label: Option<&'a str>,
        /// Borrow the keyed identity when the command is keyed.
        key: Option<&'a Key>,
        /// Hold the key description produced by [`ProgramConfig::describe_key`],
        /// if configured.
        key_description: Option<Arc<str>>,
    },
    /// Record that a keyed command replaced previously tracked work.
    KeyedCommandReplaced {
        /// Borrow the key whose tracked task changed.
        key: &'a Key,
        /// Hold the key description produced by [`ProgramConfig::describe_key`],
        /// if configured.
        key_description: Option<Arc<str>>,
        /// Report the kind of the previously tracked command.
        previous_kind: CommandKind,
        /// Hold the label from the previously tracked command, if any.
        previous_label: Option<&'a str>,
        /// Report the kind of the newly tracked command.
        next_kind: CommandKind,
        /// Hold the label from the newly tracked command, if any.
        next_label: Option<&'a str>,
    },
    /// Record that a command effect finished running.
    EffectCompleted {
        /// Identify which executor category the effect used.
        kind: CommandKind,
        /// Hold the label attached with [`Command::label`], if any.
        label: Option<&'a str>,
        /// Borrow the keyed identity when the effect is keyed.
        key: Option<&'a Key>,
        /// Hold the key description produced by [`ProgramConfig::describe_key`],
        /// if configured.
        key_description: Option<Arc<str>>,
        /// Report whether the effect produced a follow-up message.
        emitted_message: bool,
        /// Borrow the produced message when one exists.
        message: Option<&'a Msg>,
        /// Hold the message description produced by
        /// [`ProgramConfig::describe_message`], if configured.
        message_description: Option<Arc<str>>,
    },
    /// Record that a keyed effect completed after a newer generation replaced
    /// it, so its output was ignored.
    StaleKeyedCompletionIgnored {
        /// Identify which executor category the stale effect used.
        kind: CommandKind,
        /// Hold the label attached with [`Command::label`], if any.
        label: Option<&'a str>,
        /// Borrow the key whose stale completion was ignored.
        key: &'a Key,
        /// Hold the key description produced by [`ProgramConfig::describe_key`],
        /// if configured.
        key_description: Option<Arc<str>>,
        /// Report whether the stale effect produced a message.
        emitted_message: bool,
        /// Borrow the stale message when one exists.
        message: Option<&'a Msg>,
        /// Hold the message description produced by
        /// [`ProgramConfig::describe_message`], if configured.
        message_description: Option<Arc<str>>,
    },
    /// Record that reconciliation built a newly declared subscription.
    SubscriptionBuilt {
        /// Borrow the subscription key that became active.
        key: &'a Key,
        /// Hold the key description produced by [`ProgramConfig::describe_key`],
        /// if configured.
        key_description: Option<Arc<str>>,
        /// Hold the label attached with [`crate::Subscription::label`], if any.
        label: Option<&'a str>,
    },
    /// Record that reconciliation kept an existing subscription handle alive
    /// because the same key was declared again.
    SubscriptionRetained {
        /// Borrow the subscription key that remained active.
        key: &'a Key,
        /// Hold the key description produced by [`ProgramConfig::describe_key`],
        /// if configured.
        key_description: Option<Arc<str>>,
        /// Hold the latest declarative label for the retained subscription, if
        /// any.
        label: Option<&'a str>,
    },
    /// Record that reconciliation removed a previously active subscription
    /// because its key was no longer declared.
    SubscriptionRemoved {
        /// Borrow the subscription key that was removed.
        key: &'a Key,
        /// Hold the key description produced by [`ProgramConfig::describe_key`],
        /// if configured.
        key_description: Option<Arc<str>>,
        /// Hold the last label associated with the removed subscription, if any.
        label: Option<&'a str>,
    },
    /// Record the outcome of reconciling declarative subscriptions after a
    /// queue-drain pass.
    SubscriptionsReconciled {
        /// Report the number of active subscriptions after reconciliation.
        active: usize,
        /// Report how many subscriptions were newly built.
        added: usize,
        /// Report how many subscriptions were dropped because their keys were
        /// no longer present.
        removed: usize,
        /// Report how many subscriptions kept their existing handles because
        /// their keys were reused.
        retained: usize,
    },
    /// Mark the end of a queue-drain pass.
    QueueDrainFinished {
        /// Report how many messages were processed in this drain.
        processed: usize,
        /// Report how many messages remain queued after draining completed.
        remaining: usize,
    },
}

/// Configure runtime behavior for a mounted [`Program`].
///
/// Use this builder to attach observability hooks or queue-warning thresholds
/// before calling [`Program::mount_with`] or [`ModelExt::into_program_with`].
pub struct ProgramConfig<Msg> {
    pub(crate) queue_warning_threshold: Option<usize>,
    pub(crate) observer: Option<RuntimeObserver<Msg>>,
    pub(crate) describe_message: Option<DescribeMessageFn<Msg>>,
    pub(crate) describe_key: Option<DescribeKeyFn>,
}

impl<Msg> Clone for ProgramConfig<Msg> {
    fn clone(&self) -> Self {
        Self {
            queue_warning_threshold: self.queue_warning_threshold,
            observer: self.observer.clone(),
            describe_message: self.describe_message.clone(),
            describe_key: self.describe_key.clone(),
        }
    }
}

impl<Msg> Default for ProgramConfig<Msg> {
    fn default() -> Self {
        Self {
            queue_warning_threshold: None,
            observer: None,
            describe_message: None,
            describe_key: None,
        }
    }
}

impl<Msg> ProgramConfig<Msg> {
    /// Create a default runtime configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Emit a [`RuntimeEvent::QueueWarning`] when the in-memory queue grows past
    /// `threshold`.
    ///
    /// The runtime compares the queue length after enqueueing a message and
    /// before draining resumes.
    #[must_use]
    pub fn queue_warning_threshold(mut self, threshold: usize) -> Self {
        self.queue_warning_threshold = Some(threshold);
        self
    }

    /// Attach a runtime observer.
    ///
    /// The observer sees events synchronously as the runtime processes them.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use gpui::{App, IntoElement, Window, div};
    /// use gpui_tea::{Command, Dispatcher, Model, ModelExt, ProgramConfig, RuntimeEvent};
    ///
    /// enum Msg {
    ///     Ping,
    /// }
    ///
    /// struct Demo;
    ///
    /// impl Model for Demo {
    ///     type Msg = Msg;
    ///
    ///     fn update(&mut self, _msg: Self::Msg, _cx: &mut App) -> Command<Self::Msg> {
    ///         Command::none()
    ///     }
    ///
    ///     fn view(
    ///         &self,
    ///         _window: &mut Window,
    ///         _cx: &mut App,
    ///         _dispatcher: &Dispatcher<Self::Msg>,
    ///     ) -> impl IntoElement + use<> {
    ///         div()
    ///     }
    /// }
    ///
    /// # fn mount(cx: &mut App) {
    /// let config = ProgramConfig::new()
    ///     .describe_message(|_msg: &Msg| "ping")
    ///     .observer(|event| {
    ///         if let RuntimeEvent::DispatchAccepted { message_description } = event {
    ///             let _ = message_description;
    ///         }
    ///     });
    ///
    /// let _program = Demo.into_program_with(config, cx);
    /// # }
    /// ```
    #[must_use]
    pub fn observer<F>(mut self, observer: F) -> Self
    where
        F: for<'a> Fn(RuntimeEvent<'a, Msg>) + Send + Sync + 'static,
    {
        self.observer = Some(Arc::new(observer));
        self
    }

    /// Provide a message formatter for observability hooks.
    ///
    /// The runtime calls this closure lazily when an event includes message
    /// context, such as [`RuntimeEvent::DispatchAccepted`] or
    /// [`RuntimeEvent::EffectCompleted`].
    #[must_use]
    pub fn describe_message<F, S>(mut self, describe_message: F) -> Self
    where
        F: Fn(&Msg) -> S + Send + Sync + 'static,
        S: Into<Arc<str>>,
    {
        self.describe_message = Some(Arc::new(move |msg| describe_message(msg).into()));
        self
    }

    /// Provide a key formatter for observability hooks.
    ///
    /// The runtime calls this closure lazily when an event includes keyed
    /// command context or subscription lifecycle context.
    #[must_use]
    pub fn describe_key<F, S>(mut self, describe_key: F) -> Self
    where
        F: Fn(&Key) -> S + Send + Sync + 'static,
        S: Into<Arc<str>>,
    {
        self.describe_key = Some(Arc::new(move |key| describe_key(key).into()));
        self
    }

    pub(crate) fn describe_message_value(&self, message: &Msg) -> Option<Arc<str>> {
        self.describe_message
            .as_ref()
            .map(|describe| describe(message))
    }

    pub(crate) fn describe_key_value(&self, key: &Key) -> Option<Arc<str>> {
        self.describe_key.as_ref().map(|describe| describe(key))
    }

    pub(crate) fn observe(&self, event: RuntimeEvent<'_, Msg>) {
        if let Some(observer) = &self.observer {
            observer(event);
        }
    }
}

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
    fn view(
        &self,
        window: &mut Window,
        cx: &mut App,
        dispatcher: &Dispatcher<Self::Msg>,
    ) -> impl IntoElement + use<Self>;

    /// Return the declarative subscriptions required by the current model state.
    ///
    /// The runtime reconciles this set after `init` and after each queue-drain
    /// pass that processed at least one message.
    fn subscriptions(&self, _cx: &mut App) -> Subscriptions<Self::Msg> {
        Subscriptions::none()
    }
}

struct MessageQueue<Msg> {
    pending: std::collections::VecDeque<Msg>,
    is_draining: bool,
}

impl<Msg> Default for MessageQueue<Msg> {
    fn default() -> Self {
        Self {
            pending: std::collections::VecDeque::new(),
            is_draining: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TaskGeneration(u64);

impl TaskGeneration {
    fn next(counter: &mut u64) -> Self {
        // Tag each keyed task with a wrapping generation so stale completions
        // can be recognized without comparing task handles.
        *counter = counter.wrapping_add(1);
        Self(*counter)
    }
}

#[derive(Clone)]
struct CommandMeta {
    kind: CommandKind,
    label: Option<Arc<str>>,
}

impl CommandMeta {
    fn new(kind: CommandKind, label: Option<Arc<str>>) -> Self {
        Self { kind, label }
    }

    fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }
}

#[derive(Clone)]
struct KeyedEffectContext {
    key: Key,
    generation: TaskGeneration,
}

struct RunningTask {
    generation: TaskGeneration,
    meta: CommandMeta,
    // Retain the GPUI task handle so replacing the entry releases the previous
    // keyed task.
    _task: Task<()>,
}

#[derive(Default)]
struct KeyedTasks {
    next_generation: u64,
    active: HashMap<Key, RunningTask>,
}

impl KeyedTasks {
    fn next_generation(&mut self) -> TaskGeneration {
        TaskGeneration::next(&mut self.next_generation)
    }

    fn insert(
        &mut self,
        key: Key,
        generation: TaskGeneration,
        meta: CommandMeta,
        task: Task<()>,
    ) -> Option<CommandMeta> {
        self.active
            .insert(
                key,
                RunningTask {
                    generation,
                    meta,
                    _task: task,
                },
            )
            .map(|running| running.meta)
    }

    fn is_current(&self, key: &Key, generation: TaskGeneration) -> bool {
        self.active
            .get(key)
            .is_some_and(|active| active.generation == generation)
    }

    fn clear_current(&mut self, key: &Key, generation: TaskGeneration) {
        if self.is_current(key, generation) {
            self.active.remove(key);
        }
    }
}

struct ActiveSubscription {
    handle: SubHandle,
    label: Option<Arc<str>>,
}

#[derive(Default)]
struct SubscriptionRegistry {
    active: HashMap<Key, ActiveSubscription>,
}

struct SubscriptionReconcileStats {
    active: usize,
    added: usize,
    removed: usize,
    retained: usize,
}

impl SubscriptionRegistry {
    fn reconcile<Msg: Send + 'static, T: 'static>(
        &mut self,
        subscriptions: Subscriptions<Msg>,
        dispatcher: &Dispatcher<Msg>,
        config: &ProgramConfig<Msg>,
        cx: &mut Context<'_, T>,
    ) -> SubscriptionReconcileStats {
        let mut next_active = HashMap::with_capacity(subscriptions.len());
        let mut added = 0;
        let mut retained = 0;

        for subscription in subscriptions {
            let key = subscription.key;
            let label = subscription.label;

            // Reusing the same key keeps the existing handle alive and skips the
            // builder. A new key builds a fresh subscription.
            if let Some(active) = self.active.remove(&key) {
                retained += 1;
                config.observe(RuntimeEvent::SubscriptionRetained {
                    key: &key,
                    key_description: config.describe_key_value(&key),
                    label: label.as_deref(),
                });
                next_active.insert(
                    key,
                    ActiveSubscription {
                        handle: active.handle,
                        label,
                    },
                );
            } else {
                added += 1;
                let mut subscription_context = SubscriptionContext::new(cx, dispatcher.clone());
                let handle = (subscription.builder)(&mut subscription_context);
                config.observe(RuntimeEvent::SubscriptionBuilt {
                    key: &key,
                    key_description: config.describe_key_value(&key),
                    label: label.as_deref(),
                });
                next_active.insert(key, ActiveSubscription { handle, label });
            }
        }

        let removed = self.active.len();
        for (key, active) in self.active.drain() {
            config.observe(RuntimeEvent::SubscriptionRemoved {
                key: &key,
                key_description: config.describe_key_value(&key),
                label: active.label.as_deref(),
            });
        }
        self.active = next_active;

        SubscriptionReconcileStats {
            active: self.active.len(),
            added,
            removed,
            retained,
        }
    }
}

struct Runtime<Msg: Send + 'static> {
    queue: MessageQueue<Msg>,
    tasks: KeyedTasks,
    subscriptions: SubscriptionRegistry,
    dispatcher: Dispatcher<Msg>,
    config: ProgramConfig<Msg>,
    _receive_task: Task<()>,
}

impl<Msg: Send + 'static> Runtime<Msg> {
    fn new(
        dispatcher: Dispatcher<Msg>,
        receive_task: Task<()>,
        config: ProgramConfig<Msg>,
    ) -> Self {
        Self {
            queue: MessageQueue::default(),
            tasks: KeyedTasks::default(),
            subscriptions: SubscriptionRegistry::default(),
            dispatcher,
            config,
            _receive_task: receive_task,
        }
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
    /// use gpui::{App, IntoElement, Window, div};
    /// use gpui_tea::{Command, Dispatcher, Model, Program};
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
    ///     ) -> impl IntoElement + use<> {
    ///         div()
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
    use gpui::{Entity, IntoElement, Task, TestAppContext, Window, div};
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
        ) -> impl IntoElement + use<> {
            div()
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
                    generation: TaskGeneration(new_generation.0.wrapping_sub(1)),
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
