use crate::command::{CommandInner, Effect};
use crate::keyed_tasks::{CommandMeta, KeyedEffectContext};
use crate::model::Model;
use crate::observability::{
    Observability, ProgramConfig, QueueOverflowAction, RuntimeEvent, TelemetryEvent,
};
use crate::queue::{QueueReservation, QueueTracker, QueuedMessage};
use crate::runtime::{EffectId, Runtime};
use crate::{Command, CommandKind, Dispatcher, Key, ModelContext};
use futures::StreamExt;
use gpui::{App, AppContext, AsyncApp, Context, Entity, IntoElement, Render, Task, Window};
use std::{fmt, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Snapshot runtime bookkeeping for a mounted [`Program`].
pub struct RuntimeSnapshot {
    /// Report the current queue depth.
    pub queue_depth: usize,
    /// Report whether the runtime is actively draining queued messages.
    pub is_draining: bool,
    /// Report the number of tracked keyed effects.
    pub active_keyed_tasks: usize,
    /// Report the number of active subscriptions.
    pub active_subscriptions: usize,
}

/// Host a mounted [`Model`] inside the GPUI runtime.
pub struct Program<M: Model> {
    model: M,
    runtime: Runtime<M::Msg>,
}

#[derive(Clone)]
enum CompletionContext {
    Keyed(KeyedEffectContext),
    NonKeyed(EffectId),
}

impl<M> fmt::Debug for Program<M>
where
    M: Model + fmt::Debug,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Program")
            .field("model", &self.model)
            .finish_non_exhaustive()
    }
}

impl<M: Model> Program<M> {
    fn build_runtime(config: ProgramConfig<M::Msg>, cx: &mut Context<'_, Self>) -> Runtime<M::Msg> {
        let queue_policy = config.queue_policy;
        let queue_tracker = Arc::new(QueueTracker::new(queue_policy));
        let observability = Observability::new(
            config,
            Arc::new({
                let queue_tracker = queue_tracker.clone();
                move || queue_tracker.depth()
            }),
        );
        let (sender, mut receiver) = futures::channel::mpsc::unbounded();
        let dispatcher = Dispatcher::new(sender, queue_tracker.clone(), observability.clone());
        let program = cx.weak_entity();
        let receive_task = cx.spawn(move |_this, async_cx: &mut AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                while let Some(message) = receiver.next().await {
                    program
                        .update(&mut async_cx, |program, cx| {
                            program.receive_enqueued(message, cx);
                        })
                        .ok();
                }
            }
        });

        Runtime::new(dispatcher, queue_tracker, receive_task, observability)
    }

    /// Mount `model` with the default [`ProgramConfig`].
    ///
    /// # Arguments
    ///
    /// * `model` - The initial model state.
    /// * `cx` - The GPUI application context.
    ///
    /// Mounting eagerly calls [`Model::init()`], executes the returned [`Command`] through the
    /// standard queue-drain model, and reconciles the initial subscriptions before returning.
    ///
    /// # Returns
    ///
    /// An [`Entity`] holding the mounted program.
    pub fn mount(model: M, cx: &mut App) -> Entity<Self> {
        Self::mount_with(model, ProgramConfig::default(), cx)
    }

    /// Mount `model` with an explicit [`ProgramConfig`].
    ///
    /// # Arguments
    ///
    /// * `model` - The initial model state.
    /// * `config` - The configuration to use for the program runtime.
    /// * `cx` - The GPUI application context.
    ///
    /// Mounting eagerly calls [`Model::init()`], executes the returned [`Command`] through the
    /// standard queue-drain model, and reconciles the initial subscriptions before returning.
    ///
    /// # Returns
    ///
    /// An [`Entity`] holding the mounted program.
    pub fn mount_with(model: M, config: ProgramConfig<M::Msg>, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let runtime = Self::build_runtime(config, cx);
            let mut program = Self { model, runtime };
            let init_command = program.model.init(cx, &ModelContext::root());
            program.execute_command_transaction(init_command, cx);
            if program.runtime.queue.pending.is_empty() {
                program.reconcile_subscriptions(cx);
            } else {
                program.drain_queue(cx);
            }
            program
        })
    }

    /// Borrow the mounted model state.
    ///
    /// # Returns
    ///
    /// A reference to the underlying model.
    pub fn model(&self) -> &M {
        &self.model
    }

    /// Capture the current runtime bookkeeping state.
    ///
    /// # Returns
    ///
    /// A snapshot of the program's runtime.
    pub fn runtime_snapshot(&self) -> RuntimeSnapshot {
        RuntimeSnapshot {
            queue_depth: self.runtime.queue_tracker.depth(),
            is_draining: self.runtime.queue.is_draining,
            active_keyed_tasks: self.runtime.tasks.len(),
            active_subscriptions: self.runtime.subscriptions.len(),
        }
    }

    /// Clone the dispatcher associated with this program.
    ///
    /// # Returns
    ///
    /// A cloned [`Dispatcher`] that sends messages to this program.
    pub fn dispatcher(&self) -> Dispatcher<M::Msg> {
        self.runtime.dispatcher.clone()
    }

    fn receive_enqueued(&mut self, queued: QueuedMessage<M::Msg>, cx: &mut Context<'_, Self>) {
        if self.runtime.queue_tracker.take_if_dropped(queued.id) {
            return;
        }

        self.runtime.queue.pending.push_back(queued);
        self.observe_queue_warning_if_needed();
        if self.runtime.queue.should_auto_drain() {
            self.drain_queue(cx);
        }
    }

    fn enqueue_message(&mut self, message: M::Msg, cx: &mut Context<'_, Self>) {
        let message_description = self.runtime.observability.describe_message_value(&message);
        let reservation = self.runtime.queue_tracker.reserve_enqueue();

        match reservation {
            QueueReservation::Rejected => {
                self.runtime
                    .observability
                    .observe_telemetry(TelemetryEvent::QueueOverflow {
                        policy: self.runtime.observability.queue_policy(),
                        action: QueueOverflowAction::RejectedNew,
                        message_description,
                    });
            }
            QueueReservation::DroppedNewest => {
                self.runtime
                    .observability
                    .observe_telemetry(TelemetryEvent::QueueOverflow {
                        policy: self.runtime.observability.queue_policy(),
                        action: QueueOverflowAction::DroppedNewest,
                        message_description,
                    });
            }
            QueueReservation::Accepted {
                id,
                overflow_action,
                ..
            } => {
                if let Some(action) = overflow_action {
                    self.runtime
                        .observability
                        .observe_telemetry(TelemetryEvent::QueueOverflow {
                            policy: self.runtime.observability.queue_policy(),
                            action,
                            message_description: message_description.clone(),
                        });
                }

                self.receive_enqueued(QueuedMessage { id, message }, cx);
            }
        }
    }

    fn observe_queue_warning_if_needed(&self) {
        if let Some(threshold) = self.runtime.observability.queue_warning_threshold() {
            let queued = self.runtime.queue_tracker.depth();
            if queued > threshold {
                self.runtime
                    .observability
                    .observe_runtime(RuntimeEvent::QueueWarning { queued, threshold });
                self.runtime
                    .observability
                    .observe_telemetry(TelemetryEvent::QueueWarning { queued, threshold });
            }
        }
    }

    fn drain_queue(&mut self, cx: &mut Context<'_, Self>) {
        if self.runtime.queue.is_draining {
            return;
        }

        self.runtime.queue.is_draining = true;
        let queued = self.runtime.queue.pending.len();
        self.runtime
            .observability
            .observe_runtime(RuntimeEvent::QueueDrainStarted { queued });
        self.runtime
            .observability
            .observe_telemetry(TelemetryEvent::QueueDrainStarted { queued });

        let mut processed = 0;

        while let Some(queued) = self.runtime.queue.pending.pop_front() {
            if self.runtime.queue_tracker.take_if_dropped(queued.id) {
                continue;
            }

            self.runtime.queue_tracker.complete_processed(queued.id);
            let message_description = self
                .runtime
                .observability
                .describe_message_value(&queued.message);
            self.runtime
                .observability
                .observe_runtime(RuntimeEvent::MessageProcessed {
                    message: &queued.message,
                    message_description: message_description.clone(),
                });
            self.runtime
                .observability
                .observe_telemetry(TelemetryEvent::MessageProcessed {
                    message: &queued.message,
                    message_description,
                });

            let command = self.model.update(queued.message, cx, &ModelContext::root());
            processed += 1;
            self.execute_command(command, cx);
        }

        self.runtime.queue.is_draining = false;

        if processed > 0 {
            self.reconcile_subscriptions(cx);
            cx.notify();
        }

        let remaining = self.runtime.queue.pending.len();
        self.runtime
            .observability
            .observe_runtime(RuntimeEvent::QueueDrainFinished {
                processed,
                remaining,
            });
        self.runtime
            .observability
            .observe_telemetry(TelemetryEvent::QueueDrainFinished {
                processed,
                remaining,
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
                self.enqueue_message(message, cx);
            }
            CommandInner::Batch(commands) => {
                for command in commands {
                    self.execute_command(command, cx);
                }
            }
            CommandInner::Effect(effect) => {
                let meta = CommandMeta::new(effect.kind(), label);
                self.observe_command_scheduled(&meta, None);
                self.observe_effect_started(&meta, None);
                let effect_id = self.runtime.effects.next_id();
                let task =
                    Self::spawn_effect(effect, meta, CompletionContext::NonKeyed(effect_id), cx);
                self.runtime.effects.insert(effect_id, task);
            }
            CommandInner::Keyed { key, effect } => {
                let meta = CommandMeta::new(effect.kind(), label);
                self.observe_command_scheduled(&meta, Some(&key));
                self.observe_effect_started(&meta, Some(&key));
                let generation = self.runtime.tasks.next_generation();
                let keyed = KeyedEffectContext {
                    key: key.clone(),
                    generation,
                };
                let task =
                    Self::spawn_effect(effect, meta.clone(), CompletionContext::Keyed(keyed), cx);
                let previous =
                    self.runtime
                        .tasks
                        .insert(key.clone(), generation, meta.clone(), task);

                if let Some(previous) = previous {
                    let previous_kind = previous.meta.kind;
                    let previous_label = previous.meta.label().map(ToOwned::to_owned);
                    self.runtime.observability.observe_runtime(
                        RuntimeEvent::KeyedCommandReplaced {
                            key: &key,
                            key_description: self.runtime.observability.describe_key_value(&key),
                            previous_kind,
                            previous_label: previous_label.as_deref(),
                            next_kind: meta.kind,
                            next_label: meta.label(),
                        },
                    );
                    self.runtime.observability.observe_telemetry(
                        TelemetryEvent::KeyedCommandReplaced {
                            key: &key,
                            key_description: self.runtime.observability.describe_key_value(&key),
                            previous_kind,
                            previous_label: previous_label.as_deref(),
                            next_kind: meta.kind,
                            next_label: meta.label(),
                        },
                    );
                }
            }
            CommandInner::Cancel(key) => {
                self.cancel_keyed_command(&key);
            }
        }
    }

    fn execute_command_transaction(
        &mut self,
        command: Command<M::Msg>,
        cx: &mut Context<'_, Self>,
    ) {
        self.runtime.queue.suspend_auto_drain();
        self.execute_command(command, cx);
        self.runtime.queue.resume_auto_drain();
    }

    fn cancel_keyed_command(&mut self, key: &Key) {
        if let Some(running) = self.runtime.tasks.cancel(key) {
            let canceled_kind = running.meta.kind;
            let canceled_label = running.meta.label().map(ToOwned::to_owned);
            self.runtime
                .observability
                .observe_telemetry(TelemetryEvent::KeyedCommandCanceled {
                    key,
                    key_description: self.runtime.observability.describe_key_value(key),
                    canceled_kind,
                    canceled_label: canceled_label.as_deref(),
                });
        }
    }

    fn observe_command_scheduled(&self, meta: &CommandMeta, key: Option<&Key>) {
        self.runtime
            .observability
            .observe_runtime(RuntimeEvent::CommandScheduled {
                kind: meta.kind,
                label: meta.label(),
                key,
                key_description: key
                    .and_then(|key| self.runtime.observability.describe_key_value(key)),
            });
        self.runtime
            .observability
            .observe_telemetry(TelemetryEvent::CommandScheduled {
                kind: meta.kind,
                label: meta.label(),
                key,
                key_description: key
                    .and_then(|key| self.runtime.observability.describe_key_value(key)),
            });
    }

    fn observe_effect_started(&self, meta: &CommandMeta, key: Option<&Key>) {
        self.runtime
            .observability
            .observe_telemetry(TelemetryEvent::EffectStarted {
                kind: meta.kind,
                label: meta.label(),
                key,
                key_description: key
                    .and_then(|key| self.runtime.observability.describe_key_value(key)),
            });
    }

    fn observe_effect_completed(
        &self,
        meta: &CommandMeta,
        key: Option<&Key>,
        message: Option<&M::Msg>,
    ) {
        let key_description =
            key.and_then(|key| self.runtime.observability.describe_key_value(key));
        let message_description =
            message.and_then(|message| self.runtime.observability.describe_message_value(message));

        self.runtime
            .observability
            .observe_runtime(RuntimeEvent::EffectCompleted {
                kind: meta.kind,
                label: meta.label(),
                key,
                key_description: key_description.clone(),
                emitted_message: message.is_some(),
                message,
                message_description: message_description.clone(),
            });
        self.runtime
            .observability
            .observe_telemetry(TelemetryEvent::EffectCompleted {
                kind: meta.kind,
                label: meta.label(),
                key,
                key_description,
                emitted_message: message.is_some(),
                message,
                message_description,
            });
    }

    fn apply_completion(
        &mut self,
        message: Option<M::Msg>,
        meta: &CommandMeta,
        context: CompletionContext,
        cx: &mut Context<'_, Self>,
    ) {
        match context {
            CompletionContext::Keyed(keyed) => {
                let is_current = self.runtime.tasks.is_current(&keyed.key, keyed.generation);
                if is_current {
                    self.runtime
                        .tasks
                        .clear_current(&keyed.key, keyed.generation);
                }

                self.observe_effect_completed(meta, Some(&keyed.key), message.as_ref());

                if is_current {
                    if let Some(message) = message {
                        self.enqueue_message(message, cx);
                    }
                } else {
                    let key_description = self.runtime.observability.describe_key_value(&keyed.key);
                    let message_description = message.as_ref().and_then(|message| {
                        self.runtime.observability.describe_message_value(message)
                    });

                    self.runtime.observability.observe_runtime(
                        RuntimeEvent::StaleKeyedCompletionIgnored {
                            kind: meta.kind,
                            label: meta.label(),
                            key: &keyed.key,
                            key_description: key_description.clone(),
                            emitted_message: message.is_some(),
                            message: message.as_ref(),
                            message_description: message_description.clone(),
                        },
                    );
                    self.runtime.observability.observe_telemetry(
                        TelemetryEvent::StaleKeyedCompletionIgnored {
                            kind: meta.kind,
                            label: meta.label(),
                            key: &keyed.key,
                            key_description,
                            emitted_message: message.is_some(),
                            message: message.as_ref(),
                            message_description,
                        },
                    );
                }
            }
            CompletionContext::NonKeyed(effect_id) => {
                let finished = self.runtime.effects.finish(effect_id);
                debug_assert!(finished, "running non-keyed effect must still be tracked");

                self.observe_effect_completed(meta, None, message.as_ref());

                if let Some(message) = message {
                    self.enqueue_message(message, cx);
                }
            }
        }
    }

    fn spawn_effect(
        effect: Effect<M::Msg>,
        meta: CommandMeta,
        context: CompletionContext,
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
                            program.apply_completion(message, &meta, context, cx);
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
                                program.apply_completion(message, &meta, context, cx);
                            })
                            .ok();
                    }
                })
            }
        }
    }

    fn reconcile_subscriptions(&mut self, cx: &mut Context<'_, Self>) {
        let subscriptions = self.model.subscriptions(cx, &ModelContext::root());
        let dispatcher = self.dispatcher();
        let stats = self.runtime.subscriptions.reconcile(
            subscriptions,
            &dispatcher,
            &self.runtime.observability,
            cx,
        );

        self.runtime
            .observability
            .observe_runtime(RuntimeEvent::SubscriptionsReconciled {
                active: stats.active,
                added: stats.added,
                removed: stats.removed,
                retained: stats.retained,
            });
        self.runtime
            .observability
            .observe_telemetry(TelemetryEvent::SubscriptionsReconciled {
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
        self.model
            .view(window, cx, &ModelContext::root(), &dispatcher)
    }
}

#[cfg(test)]
mod runtime_tests {
    use super::*;
    use crate::{IntoView, ModelExt, QueuePolicy, View};
    use futures::channel::oneshot;
    use gpui::{Entity, Task, TestAppContext, Window, div};
    use std::collections::VecDeque;
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

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
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
            _scope: &ModelContext<Self::Msg>,
            _dispatcher: &Dispatcher<Self::Msg>,
        ) -> View {
            div().into_view()
        }
    }

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
                CompletionContext::Keyed(KeyedEffectContext {
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

    #[gpui::test]
    fn runtime_snapshot_reports_current_counts_without_side_effects(cx: &mut TestAppContext) {
        let program: Entity<Program<TestModel>> =
            cx.update(|cx| TestModel { state: 0 }.into_program(cx));

        let before = program.read_with(cx, |program, _cx| program.runtime_snapshot());
        let after = program.read_with(cx, |program, _cx| program.runtime_snapshot());

        assert_eq!(before, after);
        assert_eq!(before.queue_depth, 0);
        assert!(!before.is_draining);
        assert_eq!(before.active_keyed_tasks, 0);
        assert_eq!(before.active_subscriptions, 0);
    }

    #[gpui::test]
    fn internal_enqueue_respects_reject_new_policy_without_extra_notify(cx: &mut TestAppContext) {
        struct RejectModel {
            renders: Arc<Mutex<usize>>,
        }

        impl Model for RejectModel {
            type Msg = Msg;

            fn update(
                &mut self,
                msg: Self::Msg,
                _cx: &mut App,
                _scope: &ModelContext<Self::Msg>,
            ) -> Command<Self::Msg> {
                match msg {
                    Msg::Set(0) => {
                        Command::batch([Command::emit(Msg::Set(1)), Command::emit(Msg::Set(2))])
                    }
                    Msg::Set(_) => Command::none(),
                }
            }

            fn view(
                &self,
                _window: &mut Window,
                _cx: &mut App,
                _scope: &ModelContext<Self::Msg>,
                _dispatcher: &Dispatcher<Self::Msg>,
            ) -> View {
                *self.renders.lock().unwrap() += 1;
                div().into_view()
            }
        }

        let renders = Arc::new(Mutex::new(0));
        let program: Entity<Program<RejectModel>> = cx.update(|cx| {
            RejectModel {
                renders: renders.clone(),
            }
            .into_program_with(
                ProgramConfig::default().queue_policy(QueuePolicy::RejectNew { capacity: 1 }),
                cx,
            )
        });
        let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

        dispatcher.dispatch(Msg::Set(0)).unwrap();
        cx.run_until_parked();

        let snapshot = program.read_with(cx, |program, _cx| program.runtime_snapshot());
        assert_eq!(snapshot.queue_depth, 0);
    }

    #[gpui::test]
    fn cancel_key_clears_tracked_task_and_emits_telemetry(cx: &mut TestAppContext) {
        #[derive(Clone, Debug, PartialEq, Eq)]
        enum CancelMsg {
            RunNext,
            Loaded(i32),
        }

        #[derive(Clone, Debug, PartialEq, Eq)]
        enum CancelEvent {
            Canceled(Option<String>),
        }

        struct CancelModel {
            commands: VecDeque<Command<CancelMsg>>,
            values: Vec<i32>,
        }

        impl Model for CancelModel {
            type Msg = CancelMsg;

            fn update(
                &mut self,
                msg: Self::Msg,
                _cx: &mut App,
                _scope: &ModelContext<Self::Msg>,
            ) -> Command<Self::Msg> {
                match msg {
                    CancelMsg::RunNext => self.commands.pop_front().unwrap_or_else(Command::none),
                    CancelMsg::Loaded(value) => {
                        self.values.push(value);
                        Command::none()
                    }
                }
            }

            fn view(
                &self,
                _window: &mut Window,
                _cx: &mut App,
                _scope: &ModelContext<Self::Msg>,
                _dispatcher: &Dispatcher<Self::Msg>,
            ) -> View {
                div().into_view()
            }
        }

        let events = Arc::new(Mutex::new(Vec::new()));
        let (sender, receiver) = oneshot::channel();
        let config = ProgramConfig::default()
            .describe_message(|msg: &CancelMsg| match msg {
                CancelMsg::RunNext => String::from("run-next"),
                CancelMsg::Loaded(value) => format!("loaded:{value}"),
            })
            .describe_key(|key| format!("{key:?}"))
            .telemetry_observer({
                let events = events.clone();
                move |envelope| {
                    if let TelemetryEvent::KeyedCommandCanceled {
                        key_description, ..
                    } = envelope.event
                    {
                        events.lock().unwrap().push(CancelEvent::Canceled(
                            key_description.map(|value| value.to_string()),
                        ));
                    }
                }
            });

        let program: Entity<Program<CancelModel>> = cx.update(|cx| {
            CancelModel {
                commands: VecDeque::from([
                    Command::background_keyed("load", move |_| async move {
                        receiver.await.ok().map(CancelMsg::Loaded)
                    })
                    .label("load"),
                    Command::cancel_key("load"),
                ]),
                values: Vec::new(),
            }
            .into_program_with(config, cx)
        });
        let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

        dispatcher.dispatch(CancelMsg::RunNext).unwrap();
        cx.run_until_parked();
        dispatcher.dispatch(CancelMsg::RunNext).unwrap();
        cx.run_until_parked();
        assert_eq!(sender.send(8), Err(8));

        program.read_with(cx, |program, _cx| {
            assert!(program.model().values.is_empty());
            assert_eq!(program.runtime_snapshot().active_keyed_tasks, 0);
        });
        assert_eq!(
            events.lock().unwrap().as_slice(),
            &[CancelEvent::Canceled(Some(String::from("Key(\"load\")")))]
        );
    }
}
