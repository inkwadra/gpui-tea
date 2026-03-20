use crate::{
    Key,
    observability::events::{RuntimeEvent, TelemetryEnvelope},
};
use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

static NEXT_PROGRAM_ID: AtomicU64 = AtomicU64::new(1);

type RuntimeObserverFn<Msg> = Arc<dyn for<'a> Fn(RuntimeEvent<'a, Msg>) + Send + Sync>;
type TelemetryObserverFn<Msg> = Arc<dyn for<'a> Fn(TelemetryEnvelope<'a, Msg>) + Send + Sync>;
type DescribeMessageFn<Msg> = Arc<dyn Fn(&Msg) -> Arc<str> + Send + Sync>;
type DescribeKeyFn = Arc<dyn Fn(&Key) -> Arc<str> + Send + Sync>;
type DescribeProgramFn = Arc<dyn Fn(ProgramId) -> Arc<str> + Send + Sync>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// Identify a mounted program instance in observability output.
pub struct ProgramId(u64);

impl ProgramId {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self(NEXT_PROGRAM_ID.fetch_add(1, Ordering::Relaxed))
    }

    #[cfg(all(test, any(feature = "tracing", feature = "metrics")))]
    #[must_use]
    pub(crate) fn from_raw(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    /// Return the numeric identifier emitted in runtime and telemetry events.
    pub fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ProgramId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

#[must_use]
/// Configure runtime observability hooks and queue behavior.
pub struct ProgramConfig<Msg> {
    pub(crate) queue_warning_threshold: Option<usize>,
    pub(crate) queue_policy: super::QueuePolicy,
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
            queue_policy: super::QueuePolicy::Unbounded,
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
    /// Build a configuration with default queue and observer settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Emit queue warning events whenever the current queue depth is greater than `threshold`.
    pub fn queue_warning_threshold(mut self, threshold: usize) -> Self {
        self.queue_warning_threshold = Some(threshold);
        self
    }

    /// Set the queue backpressure policy used by the runtime.
    pub fn queue_policy(mut self, queue_policy: super::QueuePolicy) -> Self {
        self.queue_policy = queue_policy;
        self
    }

    /// Register an observer for high-level runtime events.
    pub fn observer<F>(mut self, observer: F) -> Self
    where
        F: for<'a> Fn(RuntimeEvent<'a, Msg>) + Send + Sync + 'static,
    {
        self.observer = Some(Arc::new(observer));
        self
    }

    /// Register an observer for detailed telemetry envelopes.
    pub fn telemetry_observer<F>(mut self, observer: F) -> Self
    where
        F: for<'a> Fn(TelemetryEnvelope<'a, Msg>) + Send + Sync + 'static,
    {
        self.telemetry_observer = Some(Arc::new(observer));
        self
    }

    /// Provide a human-readable description for messages.
    pub fn describe_message<F, S>(mut self, describe_message: F) -> Self
    where
        F: Fn(&Msg) -> S + Send + Sync + 'static,
        S: Into<Arc<str>>,
    {
        self.describe_message = Some(Arc::new(move |msg| describe_message(msg).into()));
        self
    }

    /// Provide a human-readable description for keyed effects and subscriptions.
    pub fn describe_key<F, S>(mut self, describe_key: F) -> Self
    where
        F: Fn(&Key) -> S + Send + Sync + 'static,
        S: Into<Arc<str>>,
    {
        self.describe_key = Some(Arc::new(move |key| describe_key(key).into()));
        self
    }

    /// Provide a human-readable description for each mounted [`ProgramId`].
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

    pub(crate) fn describe_program_value(&self, program_id: ProgramId) -> Option<Arc<str>> {
        self.describe_program
            .as_ref()
            .map(|describe| describe(program_id))
    }
}
