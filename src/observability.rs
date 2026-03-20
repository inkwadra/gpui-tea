use crate::{CommandKind, Key};
use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::SystemTime,
};

static NEXT_PROGRAM_ID: AtomicU64 = AtomicU64::new(1);

type RuntimeObserverFn<Msg> = Arc<dyn for<'a> Fn(RuntimeEvent<'a, Msg>) + Send + Sync>;
type TelemetryObserverFn<Msg> = Arc<dyn for<'a> Fn(TelemetryEnvelope<'a, Msg>) + Send + Sync>;
type DescribeMessageFn<Msg> = Arc<dyn Fn(&Msg) -> Arc<str> + Send + Sync>;
type DescribeKeyFn = Arc<dyn Fn(&Key) -> Arc<str> + Send + Sync>;
type DescribeProgramFn = Arc<dyn Fn(ProgramId) -> Arc<str> + Send + Sync>;
type QueueDepthFn = Arc<dyn Fn() -> usize + Send + Sync>;

/// Identify a mounted runtime instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProgramId(u64);

impl ProgramId {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self(NEXT_PROGRAM_ID.fetch_add(1, Ordering::Relaxed))
    }

    /// Return the raw numeric identifier.
    #[must_use]
    pub fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ProgramId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// Control how many queued messages `gpui_tea` keeps before applying loss or
/// rejection behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueuePolicy {
    /// Preserve the current behavior and grow the queue without a fixed limit.
    Unbounded,
    /// Reject newly enqueued messages once `capacity` items are already queued.
    RejectNew {
        /// Maximum number of queued messages before rejecting new ones.
        capacity: usize,
    },
    /// Drop the newest attempted message once `capacity` items are already
    /// queued.
    DropNewest {
        /// Maximum number of queued messages before dropping the attempted
        /// enqueue.
        capacity: usize,
    },
    /// Drop the oldest queued message and accept the newest one once
    /// `capacity` items are already queued.
    DropOldest {
        /// Maximum number of queued messages before replacing the oldest one.
        capacity: usize,
    },
}

impl QueuePolicy {
    #[must_use]
    pub(crate) fn capacity(self) -> Option<usize> {
        match self {
            Self::Unbounded => None,
            Self::RejectNew { capacity }
            | Self::DropNewest { capacity }
            | Self::DropOldest { capacity } => Some(capacity),
        }
    }
}

/// Describe how queue policy handled an enqueue attempt at capacity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueueOverflowAction {
    /// Reject the incoming message and report a dispatch error to the caller.
    RejectedNew,
    /// Drop the incoming message without changing the rest of the queue.
    DroppedNewest,
    /// Drop the oldest queued message to make room for the incoming message.
    DroppedOldest,
}

/// Hold common telemetry metadata that applies to every runtime event.
#[derive(Clone, Debug)]
pub struct TelemetryMetadata {
    /// Identify the mounted runtime instance.
    pub program_id: ProgramId,
    /// Hold the program description produced by
    /// [`ProgramConfig::describe_program`], if configured.
    pub program_description: Option<Arc<str>>,
    /// Report a monotonically increasing event identifier scoped to the
    /// mounted program.
    pub event_id: u64,
    /// Record when the telemetry event was emitted.
    pub emitted_at: SystemTime,
    /// Report the logical queue depth after the runtime applied the related
    /// state transition.
    pub queue_depth: usize,
}

/// Hold a telemetry event together with shared runtime metadata.
#[derive(Debug)]
pub struct TelemetryEnvelope<'a, Msg> {
    /// Hold metadata shared by all runtime telemetry events.
    pub metadata: TelemetryMetadata,
    /// Hold the runtime event payload.
    pub event: TelemetryEvent<'a, Msg>,
}

/// Report runtime telemetry from a mounted [`crate::Program`].
#[derive(Debug)]
pub enum TelemetryEvent<'a, Msg> {
    /// Record that [`crate::Dispatcher::dispatch`] accepted a message.
    DispatchAccepted {
        /// Hold the message description produced by
        /// [`ProgramConfig::describe_message`], if configured.
        message_description: Option<Arc<str>>,
    },
    /// Record that [`crate::Dispatcher::dispatch`] rejected a message because
    /// the program was no longer alive.
    DispatchRejected {
        /// Hold the message description produced by
        /// [`ProgramConfig::describe_message`], if configured.
        message_description: Option<Arc<str>>,
    },
    /// Record that queue policy overflow handling rejected or dropped a
    /// message.
    QueueOverflow {
        /// Hold the configured queue policy.
        policy: QueuePolicy,
        /// Describe how the overflow was handled.
        action: QueueOverflowAction,
        /// Hold the attempted message description produced by
        /// [`ProgramConfig::describe_message`], if configured.
        message_description: Option<Arc<str>>,
    },
    /// Mark the start of a queue-drain pass.
    QueueDrainStarted {
        /// Report how many messages were queued when draining began.
        queued: usize,
    },
    /// Warn that the queue exceeded the configured threshold.
    QueueWarning {
        /// Report the queue depth at the warning point.
        queued: usize,
        /// Report the configured threshold.
        threshold: usize,
    },
    /// Record that [`crate::Model::update`] is about to process a message.
    MessageProcessed {
        /// Borrow the processed message.
        message: &'a Msg,
        /// Hold the configured message description, if any.
        message_description: Option<Arc<str>>,
    },
    /// Record that the runtime scheduled a command.
    CommandScheduled {
        /// Identify whether the command runs synchronously, on the foreground
        /// executor, or on the background executor.
        kind: CommandKind,
        /// Hold the optional command label.
        label: Option<&'a str>,
        /// Borrow the keyed identity when one exists.
        key: Option<&'a Key>,
        /// Hold the configured key description, if any.
        key_description: Option<Arc<str>>,
    },
    /// Record that a command effect started running.
    EffectStarted {
        /// Identify the executor category used for the effect.
        kind: CommandKind,
        /// Hold the optional command label.
        label: Option<&'a str>,
        /// Borrow the keyed identity when one exists.
        key: Option<&'a Key>,
        /// Hold the configured key description, if any.
        key_description: Option<Arc<str>>,
    },
    /// Record that a keyed command replaced previously tracked work.
    KeyedCommandReplaced {
        /// Borrow the replaced key.
        key: &'a Key,
        /// Hold the configured key description, if any.
        key_description: Option<Arc<str>>,
        /// Report the kind of the previously tracked command.
        previous_kind: CommandKind,
        /// Hold the previous optional label.
        previous_label: Option<&'a str>,
        /// Report the kind of the new command.
        next_kind: CommandKind,
        /// Hold the new optional label.
        next_label: Option<&'a str>,
    },
    /// Record that explicit keyed cancellation removed currently tracked work.
    KeyedCommandCanceled {
        /// Borrow the canceled key.
        key: &'a Key,
        /// Hold the configured key description, if any.
        key_description: Option<Arc<str>>,
        /// Report the canceled command kind.
        canceled_kind: CommandKind,
        /// Hold the canceled command label, if any.
        canceled_label: Option<&'a str>,
    },
    /// Record that a command effect finished running.
    EffectCompleted {
        /// Identify the executor category used for the effect.
        kind: CommandKind,
        /// Hold the optional command label.
        label: Option<&'a str>,
        /// Borrow the keyed identity when one exists.
        key: Option<&'a Key>,
        /// Hold the configured key description, if any.
        key_description: Option<Arc<str>>,
        /// Report whether the effect produced a follow-up message.
        emitted_message: bool,
        /// Borrow the produced message when one exists.
        message: Option<&'a Msg>,
        /// Hold the configured message description, if any.
        message_description: Option<Arc<str>>,
    },
    /// Record that a keyed effect completed after being replaced or canceled.
    StaleKeyedCompletionIgnored {
        /// Identify the executor category used for the effect.
        kind: CommandKind,
        /// Hold the optional command label.
        label: Option<&'a str>,
        /// Borrow the keyed identity.
        key: &'a Key,
        /// Hold the configured key description, if any.
        key_description: Option<Arc<str>>,
        /// Report whether the stale effect produced a message.
        emitted_message: bool,
        /// Borrow the stale message when one exists.
        message: Option<&'a Msg>,
        /// Hold the configured message description, if any.
        message_description: Option<Arc<str>>,
    },
    /// Record that reconciliation built a newly declared subscription.
    SubscriptionBuilt {
        /// Borrow the active subscription key.
        key: &'a Key,
        /// Hold the configured key description, if any.
        key_description: Option<Arc<str>>,
        /// Hold the optional subscription label.
        label: Option<&'a str>,
    },
    /// Record that reconciliation retained an existing subscription.
    SubscriptionRetained {
        /// Borrow the active subscription key.
        key: &'a Key,
        /// Hold the configured key description, if any.
        key_description: Option<Arc<str>>,
        /// Hold the optional subscription label.
        label: Option<&'a str>,
    },
    /// Record that reconciliation removed a subscription.
    SubscriptionRemoved {
        /// Borrow the removed subscription key.
        key: &'a Key,
        /// Hold the configured key description, if any.
        key_description: Option<Arc<str>>,
        /// Hold the optional subscription label.
        label: Option<&'a str>,
    },
    /// Record the subscription reconciliation summary.
    SubscriptionsReconciled {
        /// Report how many subscriptions remain active.
        active: usize,
        /// Report how many subscriptions were built.
        added: usize,
        /// Report how many subscriptions were removed.
        removed: usize,
        /// Report how many subscriptions were retained.
        retained: usize,
    },
    /// Mark the end of a queue-drain pass.
    QueueDrainFinished {
        /// Report how many messages were processed in the drain.
        processed: usize,
        /// Report how many messages remain queued.
        remaining: usize,
    },
}

/// Report runtime activity from a mounted [`crate::Program`].
///
/// These events describe enqueue attempts, queue draining, command scheduling,
/// keyed task replacement, effect completion, and subscription lifecycle
/// reconciliation. They are emitted only when a [`ProgramConfig`] installs an
/// observer callback.
#[derive(Debug)]
pub enum RuntimeEvent<'a, Msg> {
    /// Record that [`crate::Dispatcher::dispatch`] accepted a message.
    DispatchAccepted {
        /// Hold the message description produced by
        /// [`ProgramConfig::describe_message`], if configured.
        message_description: Option<Arc<str>>,
    },
    /// Record that [`crate::Dispatcher::dispatch`] rejected a message because
    /// the program was no longer alive.
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
    /// Record that [`crate::Model::update`] is about to process a message.
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
        /// Hold the label attached with [`crate::Command::label`], if any.
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
        /// Hold the label attached with [`crate::Command::label`], if any.
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
        /// Hold the label attached with [`crate::Command::label`], if any.
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

/// Configure runtime behavior for a mounted [`crate::Program`].
///
/// Use this builder to attach observability hooks or queue controls before
/// calling [`crate::Program::mount_with`] or
/// [`crate::ModelExt::into_program_with`].
#[must_use]
pub struct ProgramConfig<Msg> {
    pub(crate) queue_warning_threshold: Option<usize>,
    pub(crate) queue_policy: QueuePolicy,
    pub(crate) observer: Option<RuntimeObserverFn<Msg>>,
    pub(crate) telemetry_observer: Option<TelemetryObserverFn<Msg>>,
    pub(crate) describe_message: Option<DescribeMessageFn<Msg>>,
    pub(crate) describe_key: Option<DescribeKeyFn>,
    pub(crate) describe_program: Option<DescribeProgramFn>,
}

impl<Msg> Clone for ProgramConfig<Msg> {
    fn clone(&self) -> Self {
        Self {
            queue_warning_threshold: self.queue_warning_threshold,
            queue_policy: self.queue_policy,
            observer: self.observer.clone(),
            telemetry_observer: self.telemetry_observer.clone(),
            describe_message: self.describe_message.clone(),
            describe_key: self.describe_key.clone(),
            describe_program: self.describe_program.clone(),
        }
    }
}

impl<Msg> Default for ProgramConfig<Msg> {
    fn default() -> Self {
        Self {
            queue_warning_threshold: None,
            queue_policy: QueuePolicy::Unbounded,
            observer: None,
            telemetry_observer: None,
            describe_message: None,
            describe_key: None,
            describe_program: None,
        }
    }
}

impl<Msg> fmt::Debug for ProgramConfig<Msg> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProgramConfig")
            .field("queue_warning_threshold", &self.queue_warning_threshold)
            .field("queue_policy", &self.queue_policy)
            .field("has_observer", &self.observer.is_some())
            .field("has_telemetry_observer", &self.telemetry_observer.is_some())
            .field("has_message_describer", &self.describe_message.is_some())
            .field("has_key_describer", &self.describe_key.is_some())
            .field("has_program_describer", &self.describe_program.is_some())
            .finish_non_exhaustive()
    }
}

impl<Msg> ProgramConfig<Msg> {
    /// Create a default runtime configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Emit a [`RuntimeEvent::QueueWarning`] when the queue grows past
    /// `threshold`.
    pub fn queue_warning_threshold(mut self, threshold: usize) -> Self {
        self.queue_warning_threshold = Some(threshold);
        self
    }

    /// Configure how many queued messages the runtime keeps before applying an
    /// overflow policy.
    pub fn queue_policy(mut self, queue_policy: QueuePolicy) -> Self {
        self.queue_policy = queue_policy;
        self
    }

    /// Attach a legacy runtime observer.
    ///
    /// The observer runs synchronously on the runtime hot path. Keep it cheap
    /// and non-blocking so it does not reduce GPUI responsiveness.
    pub fn observer<F>(mut self, observer: F) -> Self
    where
        F: for<'a> Fn(RuntimeEvent<'a, Msg>) + Send + Sync + 'static,
    {
        self.observer = Some(Arc::new(observer));
        self
    }

    /// Attach a structured telemetry observer.
    ///
    /// The observer runs synchronously on the runtime hot path. Prefer a
    /// lightweight adapter that forwards work to a background-safe telemetry
    /// backend.
    pub fn telemetry_observer<F>(mut self, observer: F) -> Self
    where
        F: for<'a> Fn(TelemetryEnvelope<'a, Msg>) + Send + Sync + 'static,
    {
        self.telemetry_observer = Some(Arc::new(observer));
        self
    }

    /// Provide a message formatter for observability hooks.
    pub fn describe_message<F, S>(mut self, describe_message: F) -> Self
    where
        F: Fn(&Msg) -> S + Send + Sync + 'static,
        S: Into<Arc<str>>,
    {
        self.describe_message = Some(Arc::new(move |msg| describe_message(msg).into()));
        self
    }

    /// Provide a key formatter for observability hooks.
    pub fn describe_key<F, S>(mut self, describe_key: F) -> Self
    where
        F: Fn(&Key) -> S + Send + Sync + 'static,
        S: Into<Arc<str>>,
    {
        self.describe_key = Some(Arc::new(move |key| describe_key(key).into()));
        self
    }

    /// Provide a program formatter for telemetry metadata.
    pub fn describe_program<F, S>(mut self, describe_program: F) -> Self
    where
        F: Fn(ProgramId) -> S + Send + Sync + 'static,
        S: Into<Arc<str>>,
    {
        self.describe_program = Some(Arc::new(move |program| describe_program(program).into()));
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

    fn describe_program_value(&self, program_id: ProgramId) -> Option<Arc<str>> {
        self.describe_program
            .as_ref()
            .map(|describe| describe(program_id))
    }
}

pub(crate) struct Observability<Msg> {
    config: ProgramConfig<Msg>,
    program_id: ProgramId,
    program_description: Option<Arc<str>>,
    next_event_id: Arc<AtomicU64>,
    queue_depth: QueueDepthFn,
}

impl<Msg> Clone for Observability<Msg> {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            program_id: self.program_id,
            program_description: self.program_description.clone(),
            next_event_id: self.next_event_id.clone(),
            queue_depth: self.queue_depth.clone(),
        }
    }
}

impl<Msg> Observability<Msg> {
    pub(crate) fn new(config: ProgramConfig<Msg>, queue_depth: QueueDepthFn) -> Self {
        let program_id = ProgramId::new();
        let program_description = config.describe_program_value(program_id);

        Self {
            config,
            program_id,
            program_description,
            next_event_id: Arc::new(AtomicU64::new(1)),
            queue_depth,
        }
    }

    pub(crate) fn queue_depth(&self) -> usize {
        (self.queue_depth)()
    }

    pub(crate) fn describe_message_value(&self, message: &Msg) -> Option<Arc<str>> {
        self.config.describe_message_value(message)
    }

    pub(crate) fn describe_key_value(&self, key: &Key) -> Option<Arc<str>> {
        self.config.describe_key_value(key)
    }

    pub(crate) fn queue_warning_threshold(&self) -> Option<usize> {
        self.config.queue_warning_threshold
    }

    pub(crate) fn queue_policy(&self) -> QueuePolicy {
        self.config.queue_policy
    }

    pub(crate) fn observe_runtime(&self, event: RuntimeEvent<'_, Msg>) {
        if let Some(observer) = &self.config.observer {
            observer(event);
        }
    }

    pub(crate) fn observe_telemetry(&self, event: TelemetryEvent<'_, Msg>) {
        if let Some(observer) = &self.config.telemetry_observer {
            observer(TelemetryEnvelope {
                metadata: TelemetryMetadata {
                    program_id: self.program_id,
                    program_description: self.program_description.clone(),
                    event_id: self.next_event_id.fetch_add(1, Ordering::Relaxed),
                    emitted_at: SystemTime::now(),
                    queue_depth: self.queue_depth(),
                },
                event,
            });
        }
    }
}

#[allow(clippy::too_many_lines)]
#[cfg(feature = "tracing")]
/// Publish telemetry envelopes as structured `tracing` events.
pub fn observe_tracing_telemetry<Msg>(envelope: TelemetryEnvelope<'_, Msg>) {
    use tracing::{Level, event};

    let program_id = envelope.metadata.program_id.get();
    let event_id = envelope.metadata.event_id;
    let queue_depth = envelope.metadata.queue_depth as u64;
    let program_description = option_arc_str(envelope.metadata.program_description.as_ref());

    match envelope.event {
        TelemetryEvent::DispatchAccepted {
            message_description,
        } => {
            event!(
                Level::DEBUG,
                event_name = "dispatch.accepted",
                program_id,
                event_id,
                queue_depth,
                program_description,
                message_description = option_arc_str(message_description.as_ref()),
            );
        }
        TelemetryEvent::DispatchRejected {
            message_description,
        } => {
            event!(
                Level::WARN,
                event_name = "dispatch.rejected",
                program_id,
                event_id,
                queue_depth,
                program_description,
                message_description = option_arc_str(message_description.as_ref()),
            );
        }
        TelemetryEvent::QueueOverflow {
            policy,
            action,
            message_description,
        } => {
            event!(
                Level::WARN,
                event_name = "queue.overflow",
                program_id,
                event_id,
                queue_depth,
                program_description,
                policy = ?policy,
                action = ?action,
                message_description = option_arc_str(message_description.as_ref()),
            );
        }
        TelemetryEvent::QueueDrainStarted { queued } => {
            event!(
                Level::DEBUG,
                event_name = "queue.drain.started",
                program_id,
                event_id,
                queue_depth,
                program_description,
                queued = queued as u64,
            );
        }
        TelemetryEvent::QueueWarning { queued, threshold } => {
            event!(
                Level::WARN,
                event_name = "queue.warning",
                program_id,
                event_id,
                queue_depth,
                program_description,
                queued = queued as u64,
                threshold = threshold as u64,
            );
        }
        TelemetryEvent::MessageProcessed {
            message_description,
            ..
        } => {
            event!(
                Level::DEBUG,
                event_name = "message.processed",
                program_id,
                event_id,
                queue_depth,
                program_description,
                message_description = option_arc_str(message_description.as_ref()),
            );
        }
        TelemetryEvent::CommandScheduled {
            kind,
            label,
            key_description,
            ..
        } => {
            event!(
                Level::DEBUG,
                event_name = "command.scheduled",
                program_id,
                event_id,
                queue_depth,
                program_description,
                kind = ?kind,
                label = label.unwrap_or("<none>"),
                key_description = option_arc_str(key_description.as_ref()),
            );
        }
        TelemetryEvent::EffectStarted {
            kind,
            label,
            key_description,
            ..
        } => {
            event!(
                Level::DEBUG,
                event_name = "effect.started",
                program_id,
                event_id,
                queue_depth,
                program_description,
                kind = ?kind,
                label = label.unwrap_or("<none>"),
                key_description = option_arc_str(key_description.as_ref()),
            );
        }
        TelemetryEvent::KeyedCommandReplaced {
            key_description,
            previous_kind,
            previous_label,
            next_kind,
            next_label,
            ..
        } => {
            event!(
                Level::DEBUG,
                event_name = "keyed.replaced",
                program_id,
                event_id,
                queue_depth,
                program_description,
                key_description = option_arc_str(key_description.as_ref()),
                previous_kind = ?previous_kind,
                previous_label = previous_label.unwrap_or("<none>"),
                next_kind = ?next_kind,
                next_label = next_label.unwrap_or("<none>"),
            );
        }
        TelemetryEvent::KeyedCommandCanceled {
            key_description,
            canceled_kind,
            canceled_label,
            ..
        } => {
            event!(
                Level::DEBUG,
                event_name = "keyed.canceled",
                program_id,
                event_id,
                queue_depth,
                program_description,
                key_description = option_arc_str(key_description.as_ref()),
                canceled_kind = ?canceled_kind,
                canceled_label = canceled_label.unwrap_or("<none>"),
            );
        }
        TelemetryEvent::EffectCompleted {
            kind,
            label,
            key_description,
            emitted_message,
            message_description,
            ..
        } => {
            event!(
                Level::DEBUG,
                event_name = "effect.completed",
                program_id,
                event_id,
                queue_depth,
                program_description,
                kind = ?kind,
                label = label.unwrap_or("<none>"),
                key_description = option_arc_str(key_description.as_ref()),
                emitted_message,
                message_description = option_arc_str(message_description.as_ref()),
            );
        }
        TelemetryEvent::StaleKeyedCompletionIgnored {
            kind,
            label,
            key_description,
            emitted_message,
            message_description,
            ..
        } => {
            event!(
                Level::DEBUG,
                event_name = "effect.stale_ignored",
                program_id,
                event_id,
                queue_depth,
                program_description,
                kind = ?kind,
                label = label.unwrap_or("<none>"),
                key_description = option_arc_str(key_description.as_ref()),
                emitted_message,
                message_description = option_arc_str(message_description.as_ref()),
            );
        }
        TelemetryEvent::SubscriptionBuilt {
            key_description,
            label,
            ..
        } => {
            event!(
                Level::DEBUG,
                event_name = "subscription.built",
                program_id,
                event_id,
                queue_depth,
                program_description,
                key_description = option_arc_str(key_description.as_ref()),
                label = label.unwrap_or("<none>"),
            );
        }
        TelemetryEvent::SubscriptionRetained {
            key_description,
            label,
            ..
        } => {
            event!(
                Level::DEBUG,
                event_name = "subscription.retained",
                program_id,
                event_id,
                queue_depth,
                program_description,
                key_description = option_arc_str(key_description.as_ref()),
                label = label.unwrap_or("<none>"),
            );
        }
        TelemetryEvent::SubscriptionRemoved {
            key_description,
            label,
            ..
        } => {
            event!(
                Level::DEBUG,
                event_name = "subscription.removed",
                program_id,
                event_id,
                queue_depth,
                program_description,
                key_description = option_arc_str(key_description.as_ref()),
                label = label.unwrap_or("<none>"),
            );
        }
        TelemetryEvent::SubscriptionsReconciled {
            active,
            added,
            removed,
            retained,
        } => {
            event!(
                Level::DEBUG,
                event_name = "subscriptions.reconciled",
                program_id,
                event_id,
                queue_depth,
                program_description,
                active = active as u64,
                added = added as u64,
                removed = removed as u64,
                retained = retained as u64,
            );
        }
        TelemetryEvent::QueueDrainFinished {
            processed,
            remaining,
        } => {
            event!(
                Level::DEBUG,
                event_name = "queue.drain.finished",
                program_id,
                event_id,
                queue_depth,
                program_description,
                processed = processed as u64,
                remaining = remaining as u64,
            );
        }
    }
}

#[allow(clippy::cast_precision_loss, clippy::needless_pass_by_value)]
#[cfg(feature = "metrics")]
/// Publish queue, effect, and subscription runtime activity through the
/// `metrics` facade.
pub fn observe_metrics_telemetry<Msg>(envelope: TelemetryEnvelope<'_, Msg>) {
    use metrics::{counter, gauge};

    let TelemetryEnvelope { metadata, event } = envelope;
    gauge!("gpui_tea.queue.depth").set(metadata.queue_depth as f64);

    match event {
        TelemetryEvent::DispatchAccepted { .. } => {
            counter!("gpui_tea.dispatch.accepted").increment(1);
        }
        TelemetryEvent::DispatchRejected { .. } => {
            counter!("gpui_tea.dispatch.rejected").increment(1);
        }
        TelemetryEvent::QueueOverflow { action, .. } => {
            counter!("gpui_tea.queue.overflow", "action" => format!("{action:?}")).increment(1);
        }
        TelemetryEvent::CommandScheduled { kind, .. } => {
            counter!("gpui_tea.command.scheduled", "kind" => format!("{kind:?}")).increment(1);
        }
        TelemetryEvent::EffectStarted { kind, .. } => {
            counter!("gpui_tea.effect.started", "kind" => format!("{kind:?}")).increment(1);
        }
        TelemetryEvent::EffectCompleted { kind, .. } => {
            counter!("gpui_tea.effect.completed", "kind" => format!("{kind:?}")).increment(1);
        }
        TelemetryEvent::KeyedCommandReplaced { .. } => {
            counter!("gpui_tea.keyed.replaced").increment(1);
        }
        TelemetryEvent::KeyedCommandCanceled { .. } => {
            counter!("gpui_tea.keyed.canceled").increment(1);
        }
        TelemetryEvent::StaleKeyedCompletionIgnored { .. } => {
            counter!("gpui_tea.keyed.stale_ignored").increment(1);
        }
        TelemetryEvent::SubscriptionBuilt { .. } => {
            counter!("gpui_tea.subscription.built").increment(1);
        }
        TelemetryEvent::SubscriptionRemoved { .. } => {
            counter!("gpui_tea.subscription.removed").increment(1);
        }
        TelemetryEvent::SubscriptionsReconciled { active, .. } => {
            gauge!("gpui_tea.subscription.active").set(active as f64);
        }
        TelemetryEvent::QueueDrainStarted { .. }
        | TelemetryEvent::QueueWarning { .. }
        | TelemetryEvent::MessageProcessed { .. }
        | TelemetryEvent::SubscriptionRetained { .. }
        | TelemetryEvent::QueueDrainFinished { .. } => {}
    }
}

#[cfg(feature = "tracing")]
fn option_arc_str(value: Option<&Arc<str>>) -> &str {
    value.map_or("<none>", Arc::as_ref)
}

#[cfg(all(test, feature = "tracing"))]
mod tracing_tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tracing::{Metadata, Subscriber};
    use tracing_subscriber::{
        layer::{Context, Layer},
        prelude::*,
        registry::LookupSpan,
    };

    struct CountLayer {
        count: Arc<AtomicUsize>,
    }

    impl<S> Layer<S> for CountLayer
    where
        S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    {
        fn on_event(&self, _event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
            self.count.fetch_add(1, Ordering::SeqCst);
        }

        fn register_callsite(
            &self,
            _metadata: &'static Metadata<'static>,
        ) -> tracing::subscriber::Interest {
            tracing::subscriber::Interest::always()
        }
    }

    #[allow(clippy::missing_panics_doc)]
    #[test]
    fn tracing_adapter_emits_event() {
        let count = Arc::new(AtomicUsize::new(0));
        let subscriber = tracing_subscriber::registry().with(CountLayer {
            count: count.clone(),
        });

        tracing::subscriber::with_default(subscriber, || {
            observe_tracing_telemetry::<()>(TelemetryEnvelope {
                metadata: TelemetryMetadata {
                    program_id: ProgramId(1),
                    program_description: Some(Arc::from("program")),
                    event_id: 1,
                    emitted_at: SystemTime::now(),
                    queue_depth: 0,
                },
                event: TelemetryEvent::DispatchAccepted {
                    message_description: Some(Arc::from("run")),
                },
            });
        });

        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
}

#[cfg(all(test, feature = "metrics"))]
mod metrics_tests {
    use super::*;

    #[test]
    fn metrics_adapter_accepts_envelope() {
        observe_metrics_telemetry::<()>(TelemetryEnvelope {
            metadata: TelemetryMetadata {
                program_id: ProgramId(1),
                program_description: Some(Arc::from("program")),
                event_id: 1,
                emitted_at: SystemTime::now(),
                queue_depth: 3,
            },
            event: TelemetryEvent::EffectCompleted {
                kind: CommandKind::Background,
                label: Some("load"),
                key: None,
                key_description: None,
                emitted_message: true,
                message: None,
                message_description: Some(Arc::from("loaded")),
            },
        });
    }
}
