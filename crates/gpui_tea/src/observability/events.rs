use crate::{CommandKind, Key};
use std::{sync::Arc, time::SystemTime};

use super::{ProgramId, QueueOverflowAction, QueuePolicy};

#[derive(Clone, Debug)]
/// Carry metadata emitted with each [`TelemetryEvent`].
pub struct TelemetryMetadata {
    /// Identify the mounted program that emitted the event.
    pub program_id: ProgramId,
    /// Hold the optional human-readable description configured for the program.
    pub program_description: Option<Arc<str>>,
    /// Carry a monotonically increasing event identifier within the emitting process.
    pub event_id: u64,
    /// Record when the event was emitted.
    pub emitted_at: SystemTime,
    /// Snapshot the tracked queue depth at emission time.
    pub queue_depth: usize,
}

#[derive(Debug)]
/// Wrap a telemetry event together with its metadata.
pub struct TelemetryEnvelope<'a, Msg> {
    /// Attach metadata describing the emitting program and queue state.
    pub metadata: TelemetryMetadata,
    /// Carry the detailed telemetry event payload.
    pub event: TelemetryEvent<'a, Msg>,
}

#[derive(Debug)]
/// Describe detailed runtime telemetry intended for machine consumption.
pub enum TelemetryEvent<'a, Msg> {
    /// A dispatch call was accepted by the runtime.
    DispatchAccepted {
        /// Hold the optional human-readable description for the dispatched message.
        message_description: Option<Arc<str>>,
    },
    /// A dispatch call could not be delivered to the runtime.
    DispatchRejected {
        /// Hold the optional human-readable description for the dispatched message.
        message_description: Option<Arc<str>>,
    },
    /// Queue capacity handling was triggered while dispatching or enqueuing a message.
    QueueOverflow {
        /// Identify the active queue policy.
        policy: QueuePolicy,
        /// Describe the action taken by the policy.
        action: QueueOverflowAction,
        /// Hold the optional human-readable description for the affected message.
        message_description: Option<Arc<str>>,
    },
    /// Queue draining started for the current batch of pending messages.
    QueueDrainStarted {
        /// Report the number of messages pending when draining began.
        queued: usize,
    },
    /// Queue depth is currently above the configured warning threshold.
    QueueWarning {
        /// Report the current tracked queue depth.
        queued: usize,
        /// Report the configured warning threshold.
        threshold: usize,
    },
    /// A message was applied to the model.
    MessageProcessed {
        /// Borrow the processed message value.
        message: &'a Msg,
        /// Hold the optional human-readable description for the processed message.
        message_description: Option<Arc<str>>,
    },
    /// A command was scheduled for execution.
    CommandScheduled {
        /// Classify the command execution kind.
        kind: CommandKind,
        /// Hold the optional command label.
        label: Option<&'a str>,
        /// Borrow the command key when the command is keyed.
        key: Option<&'a Key>,
        /// Hold the optional human-readable description for the key.
        key_description: Option<Arc<str>>,
    },
    /// An asynchronous effect started running.
    EffectStarted {
        /// Classify the effect execution kind.
        kind: CommandKind,
        /// Hold the optional command label.
        label: Option<&'a str>,
        /// Borrow the effect key when the effect is keyed.
        key: Option<&'a Key>,
        /// Hold the optional human-readable description for the key.
        key_description: Option<Arc<str>>,
    },
    /// A newly scheduled keyed command replaced an earlier tracked keyed command.
    KeyedCommandReplaced {
        /// Borrow the key shared by the old and new commands.
        key: &'a Key,
        /// Hold the optional human-readable description for the key.
        key_description: Option<Arc<str>>,
        /// Report the execution kind of the replaced command.
        previous_kind: CommandKind,
        /// Hold the optional label of the replaced command.
        previous_label: Option<&'a str>,
        /// Report the execution kind of the replacement command.
        next_kind: CommandKind,
        /// Hold the optional label of the replacement command.
        next_label: Option<&'a str>,
    },
    /// A keyed command was canceled explicitly.
    ///
    /// This event is emitted only in telemetry, not in [`RuntimeEvent`].
    KeyedCommandCanceled {
        /// Borrow the canceled key.
        key: &'a Key,
        /// Hold the optional human-readable description for the key.
        key_description: Option<Arc<str>>,
        /// Report the execution kind of the canceled command.
        canceled_kind: CommandKind,
        /// Hold the optional label of the canceled command.
        canceled_label: Option<&'a str>,
    },
    /// A command or effect completed.
    ///
    /// This is also emitted for synchronous [`crate::Command::emit`] commands.
    EffectCompleted {
        /// Classify the command execution kind.
        kind: CommandKind,
        /// Hold the optional command label.
        label: Option<&'a str>,
        /// Borrow the key when the command is keyed.
        key: Option<&'a Key>,
        /// Hold the optional human-readable description for the key.
        key_description: Option<Arc<str>>,
        /// Report whether the completion produced a follow-up message.
        emitted_message: bool,
        /// Borrow the emitted message when one was produced.
        message: Option<&'a Msg>,
        /// Hold the optional human-readable description for the emitted message.
        message_description: Option<Arc<str>>,
    },
    /// A completion from an outdated keyed command was ignored.
    StaleKeyedCompletionIgnored {
        /// Classify the command execution kind.
        kind: CommandKind,
        /// Hold the optional command label.
        label: Option<&'a str>,
        /// Borrow the key whose stale completion was ignored.
        key: &'a Key,
        /// Hold the optional human-readable description for the key.
        key_description: Option<Arc<str>>,
        /// Report whether the stale completion tried to emit a message.
        emitted_message: bool,
        /// Borrow the stale emitted message when one was produced.
        message: Option<&'a Msg>,
        /// Hold the optional human-readable description for the stale emitted message.
        message_description: Option<Arc<str>>,
    },
    /// A new subscription resource was built during reconciliation.
    SubscriptionBuilt {
        /// Borrow the subscription key.
        key: &'a Key,
        /// Hold the optional human-readable description for the key.
        key_description: Option<Arc<str>>,
        /// Hold the optional subscription label.
        label: Option<&'a str>,
    },
    /// An existing subscription resource was retained during reconciliation.
    SubscriptionRetained {
        /// Borrow the subscription key.
        key: &'a Key,
        /// Hold the optional human-readable description for the key.
        key_description: Option<Arc<str>>,
        /// Hold the optional subscription label.
        label: Option<&'a str>,
    },
    /// A previously active subscription resource was removed during reconciliation.
    SubscriptionRemoved {
        /// Borrow the subscription key.
        key: &'a Key,
        /// Hold the optional human-readable description for the key.
        key_description: Option<Arc<str>>,
        /// Hold the optional subscription label.
        label: Option<&'a str>,
    },
    /// Subscription reconciliation finished for the current model state.
    SubscriptionsReconciled {
        /// Report the number of subscriptions active after reconciliation.
        active: usize,
        /// Report how many subscriptions were built.
        added: usize,
        /// Report how many subscriptions were removed.
        removed: usize,
        /// Report how many subscriptions were retained.
        retained: usize,
    },
    /// Queue draining finished for the current batch.
    QueueDrainFinished {
        /// Report how many messages were processed.
        processed: usize,
        /// Report how many messages remain queued.
        remaining: usize,
    },
}

#[derive(Debug)]
/// Describe high-level runtime events intended for application observers.
pub enum RuntimeEvent<'a, Msg> {
    /// A dispatch call was accepted by the runtime.
    DispatchAccepted {
        /// Hold the optional human-readable description for the dispatched message.
        message_description: Option<Arc<str>>,
    },
    /// A dispatch call could not be delivered to the runtime.
    DispatchRejected {
        /// Hold the optional human-readable description for the dispatched message.
        message_description: Option<Arc<str>>,
    },
    /// Queue draining started for the current batch of pending messages.
    QueueDrainStarted {
        /// Report the number of messages pending when draining began.
        queued: usize,
    },
    /// Queue depth is currently above the configured warning threshold.
    QueueWarning {
        /// Report the current tracked queue depth.
        queued: usize,
        /// Report the configured warning threshold.
        threshold: usize,
    },
    /// A message was applied to the model.
    MessageProcessed {
        /// Borrow the processed message value.
        message: &'a Msg,
        /// Hold the optional human-readable description for the processed message.
        message_description: Option<Arc<str>>,
    },
    /// A command was scheduled for execution.
    CommandScheduled {
        /// Classify the command execution kind.
        kind: CommandKind,
        /// Hold the optional command label.
        label: Option<&'a str>,
        /// Borrow the command key when the command is keyed.
        key: Option<&'a Key>,
        /// Hold the optional human-readable description for the key.
        key_description: Option<Arc<str>>,
    },
    /// A newly scheduled keyed command replaced an earlier tracked keyed command.
    KeyedCommandReplaced {
        /// Borrow the key shared by the old and new commands.
        key: &'a Key,
        /// Hold the optional human-readable description for the key.
        key_description: Option<Arc<str>>,
        /// Report the execution kind of the replaced command.
        previous_kind: CommandKind,
        /// Hold the optional label of the replaced command.
        previous_label: Option<&'a str>,
        /// Report the execution kind of the replacement command.
        next_kind: CommandKind,
        /// Hold the optional label of the replacement command.
        next_label: Option<&'a str>,
    },
    /// A command or effect completed.
    ///
    /// This is also emitted for synchronous [`crate::Command::emit`] commands.
    EffectCompleted {
        /// Classify the command execution kind.
        kind: CommandKind,
        /// Hold the optional command label.
        label: Option<&'a str>,
        /// Borrow the key when the command is keyed.
        key: Option<&'a Key>,
        /// Hold the optional human-readable description for the key.
        key_description: Option<Arc<str>>,
        /// Report whether the completion produced a follow-up message.
        emitted_message: bool,
        /// Borrow the emitted message when one was produced.
        message: Option<&'a Msg>,
        /// Hold the optional human-readable description for the emitted message.
        message_description: Option<Arc<str>>,
    },
    /// A completion from an outdated keyed command was ignored.
    StaleKeyedCompletionIgnored {
        /// Classify the command execution kind.
        kind: CommandKind,
        /// Hold the optional command label.
        label: Option<&'a str>,
        /// Borrow the key whose stale completion was ignored.
        key: &'a Key,
        /// Hold the optional human-readable description for the key.
        key_description: Option<Arc<str>>,
        /// Report whether the stale completion tried to emit a message.
        emitted_message: bool,
        /// Borrow the stale emitted message when one was produced.
        message: Option<&'a Msg>,
        /// Hold the optional human-readable description for the stale emitted message.
        message_description: Option<Arc<str>>,
    },
    /// A new subscription resource was built during reconciliation.
    SubscriptionBuilt {
        /// Borrow the subscription key.
        key: &'a Key,
        /// Hold the optional human-readable description for the key.
        key_description: Option<Arc<str>>,
        /// Hold the optional subscription label.
        label: Option<&'a str>,
    },
    /// An existing subscription resource was retained during reconciliation.
    SubscriptionRetained {
        /// Borrow the subscription key.
        key: &'a Key,
        /// Hold the optional human-readable description for the key.
        key_description: Option<Arc<str>>,
        /// Hold the optional subscription label.
        label: Option<&'a str>,
    },
    /// A previously active subscription resource was removed during reconciliation.
    SubscriptionRemoved {
        /// Borrow the subscription key.
        key: &'a Key,
        /// Hold the optional human-readable description for the key.
        key_description: Option<Arc<str>>,
        /// Hold the optional subscription label.
        label: Option<&'a str>,
    },
    /// Subscription reconciliation finished for the current model state.
    SubscriptionsReconciled {
        /// Report the number of subscriptions active after reconciliation.
        active: usize,
        /// Report how many subscriptions were built.
        added: usize,
        /// Report how many subscriptions were removed.
        removed: usize,
        /// Report how many subscriptions were retained.
        retained: usize,
    },
    /// Queue draining finished for the current batch.
    QueueDrainFinished {
        /// Report how many messages were processed.
        processed: usize,
        /// Report how many messages remain queued.
        remaining: usize,
    },
}
