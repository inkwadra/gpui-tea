use crate::{CommandKind, Key};
use std::sync::Arc;

/// Observe [`RuntimeEvent`] values emitted by a mounted [`crate::Program`].
///
/// The runtime invokes observers synchronously at the event site. Keep the
/// callback lightweight and non-blocking so dispatch, queue draining, and
/// effect completion stay responsive.
pub type RuntimeObserver<Msg> = Arc<dyn for<'a> Fn(RuntimeEvent<'a, Msg>) + Send + Sync>;
type DescribeMessageFn<Msg> = Arc<dyn Fn(&Msg) -> Arc<str> + Send + Sync>;
type DescribeKeyFn = Arc<dyn Fn(&Key) -> Arc<str> + Send + Sync>;

/// Report runtime activity from a mounted [`crate::Program`].
///
/// These events describe enqueue attempts, queue draining, command scheduling,
/// keyed task replacement, effect completion, and subscription lifecycle
/// reconciliation. They are emitted only when a [`ProgramConfig`] installs an
/// [`RuntimeObserver`].
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
/// Use this builder to attach observability hooks or queue-warning thresholds
/// before calling [`crate::Program::mount_with`] or
/// [`crate::ModelExt::into_program_with`].
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
    #[must_use]
    pub fn observer<F>(mut self, observer: F) -> Self
    where
        F: for<'a> Fn(RuntimeEvent<'a, Msg>) + Send + Sync + 'static,
    {
        self.observer = Some(Arc::new(observer));
        self
    }

    /// Provide a message formatter for observability hooks.
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
