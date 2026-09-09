#[cfg(any(feature = "tracing", feature = "metrics"))]
use super::{TelemetryEnvelope, TelemetryEvent};

#[cfg(any(feature = "tracing", feature = "metrics"))]
use std::sync::Arc;

#[cfg(feature = "tracing")]
/// Record a [`TelemetryEnvelope`] as `tracing` events.
///
/// Events share the `gpui_tea::observability::adapters` target; filter by the `event_name` field
/// (for example `dispatch.accepted`).
#[allow(clippy::too_many_lines)]
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

#[cfg(feature = "metrics")]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
/// Configuration for the [`MetricsAdapter`].
pub struct MetricsAdapterConfig {
    pub(crate) include_program_id: bool,
    pub(crate) program_name: Option<Arc<str>>,
}

#[cfg(feature = "metrics")]
impl MetricsAdapterConfig {
    /// Create a new metrics adapter configuration with default settings.
    ///
    /// By default, `include_program_id` is `false` to avoid high cardinality,
    /// and `program_name` is `None`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable or disable attaching `"program_id"` labels to emitted metrics.
    ///
    /// Defaults to `false`. When enabled, metrics include `"program_id" => id`.
    /// Note that each mounted program receives a new monotonic identifier,
    /// which can cause high label cardinality in metric collectors.
    #[must_use]
    pub fn include_program_id(mut self, include: bool) -> Self {
        self.include_program_id = include;
        self
    }

    /// Set an explicit program name to use for the `"program"` label.
    ///
    /// If not configured, the adapter falls back to `program_description` from
    /// telemetry metadata. If neither is present, the `"program"` label is omitted.
    #[must_use]
    pub fn program_name(mut self, name: impl Into<Arc<str>>) -> Self {
        self.program_name = Some(name.into());
        self
    }

    /// Return whether `"program_id"` labels are enabled.
    #[must_use]
    pub fn is_program_id_included(&self) -> bool {
        self.include_program_id
    }

    /// Return the configured program name, if any.
    #[must_use]
    pub fn program_name_label(&self) -> Option<&str> {
        self.program_name
            .as_deref()
            .filter(|s| !s.trim().is_empty())
    }

    /// Build a [`MetricsAdapter`] with this configuration.
    #[must_use]
    pub fn build(self) -> MetricsAdapter {
        MetricsAdapter::with_config(self)
    }
}

#[cfg(feature = "metrics")]
impl From<MetricsAdapterConfig> for MetricsAdapter {
    fn from(config: MetricsAdapterConfig) -> Self {
        Self::with_config(config)
    }
}

#[cfg(feature = "metrics")]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
/// An adapter that records [`TelemetryEnvelope`] events as `metrics` counters and gauges.
///
/// # Label Cardinality
///
/// By default, this adapter uses a stable `"program"` label derived from the configured
/// `program_name` or the program's `program_description`. If neither is provided, the
/// `"program"` label is omitted entirely.
///
/// Per-instance `"program_id"` labels are disabled by default to prevent unbounded
/// cardinality in systems where programs are created and destroyed frequently. If per-instance
/// identification is desired, enable it explicitly via [`MetricsAdapter::include_program_id`].
pub struct MetricsAdapter {
    config: MetricsAdapterConfig,
}

#[cfg(feature = "metrics")]
impl MetricsAdapter {
    /// Create a new metrics adapter with default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new metrics adapter with the specified configuration.
    #[must_use]
    pub fn with_config(config: MetricsAdapterConfig) -> Self {
        Self { config }
    }

    /// Set whether to include `"program_id"` labels on emitted metrics.
    #[must_use]
    pub fn include_program_id(mut self, include: bool) -> Self {
        self.config = self.config.include_program_id(include);
        self
    }

    /// Set an explicit program name used as the `"program"` label.
    #[must_use]
    pub fn program_name(mut self, name: impl Into<Arc<str>>) -> Self {
        self.config = self.config.program_name(name);
        self
    }

    /// Return a reference to the adapter configuration.
    #[must_use]
    pub fn config(&self) -> &MetricsAdapterConfig {
        &self.config
    }

    /// Return a telemetry observer closure suitable for [`crate::ProgramConfig::telemetry_observer`].
    pub fn observer<Msg>(
        &self,
    ) -> impl Fn(TelemetryEnvelope<'_, Msg>) + Clone + Send + Sync + 'static {
        let adapter = self.clone();
        move |envelope| adapter.observe(envelope)
    }

    /// Record a [`TelemetryEnvelope`] as `metrics` counters and gauges.
    #[allow(clippy::cast_precision_loss)]
    pub fn observe<Msg>(&self, envelope: TelemetryEnvelope<'_, Msg>) {
        use metrics::{Label, counter, gauge};

        let TelemetryEnvelope { metadata, event } = envelope;

        let mut base_labels = Vec::with_capacity(2);
        let program = self
            .config
            .program_name
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                metadata
                    .program_description
                    .as_deref()
                    .filter(|s| !s.trim().is_empty())
            });
        if let Some(program) = program {
            base_labels.push(Label::new("program", program.to_string()));
        }
        if self.config.include_program_id {
            base_labels.push(Label::new(
                "program_id",
                metadata.program_id.get().to_string(),
            ));
        }

        gauge!("gpui_tea.queue.depth", base_labels.clone()).set(metadata.queue_depth as f64);

        match event {
            TelemetryEvent::DispatchAccepted { .. } => {
                counter!("gpui_tea.dispatch.accepted", base_labels).increment(1);
            }
            TelemetryEvent::DispatchRejected { .. } => {
                counter!("gpui_tea.dispatch.rejected", base_labels).increment(1);
            }
            TelemetryEvent::QueueOverflow { action, .. } => {
                let mut labels = base_labels;
                labels.push(Label::new("action", action.as_str()));
                counter!("gpui_tea.queue.overflow", labels).increment(1);
            }
            TelemetryEvent::CommandScheduled { kind, .. } => {
                let mut labels = base_labels;
                labels.push(Label::new("kind", kind.as_str()));
                counter!("gpui_tea.command.scheduled", labels).increment(1);
            }
            TelemetryEvent::EffectStarted { kind, .. } => {
                let mut labels = base_labels;
                labels.push(Label::new("kind", kind.as_str()));
                counter!("gpui_tea.effect.started", labels).increment(1);
            }
            TelemetryEvent::EffectCompleted { kind, .. } => {
                let mut labels = base_labels;
                labels.push(Label::new("kind", kind.as_str()));
                counter!("gpui_tea.effect.completed", labels).increment(1);
            }
            TelemetryEvent::KeyedCommandReplaced { .. } => {
                counter!("gpui_tea.keyed.replaced", base_labels).increment(1);
            }
            TelemetryEvent::KeyedCommandCanceled { .. } => {
                counter!("gpui_tea.keyed.canceled", base_labels).increment(1);
            }
            TelemetryEvent::StaleKeyedCompletionIgnored { .. } => {
                counter!("gpui_tea.keyed.stale_ignored", base_labels).increment(1);
            }
            TelemetryEvent::SubscriptionBuilt { .. } => {
                counter!("gpui_tea.subscription.built", base_labels).increment(1);
            }
            TelemetryEvent::SubscriptionRemoved { .. } => {
                counter!("gpui_tea.subscription.removed", base_labels).increment(1);
            }
            TelemetryEvent::SubscriptionsReconciled { active, .. } => {
                gauge!("gpui_tea.subscription.active", base_labels).set(active as f64);
            }
            TelemetryEvent::QueueWarning { .. } => {
                counter!("gpui_tea.queue.warning", base_labels).increment(1);
            }
            TelemetryEvent::QueueDrainStarted { .. }
            | TelemetryEvent::MessageProcessed { .. }
            | TelemetryEvent::SubscriptionRetained { .. }
            | TelemetryEvent::QueueDrainFinished { .. } => {}
        }
    }
}

#[cfg(feature = "metrics")]
/// Record a [`TelemetryEnvelope`] as `metrics` counters and gauges using the default [`MetricsAdapter`].
///
/// # Cardinality Note
///
/// By default, metrics emitted by this function derive a stable `"program"` label from the program's
/// `program_description`. If no description is provided, the `"program"` label is omitted entirely.
/// Monotonic `program_id` labels are not included by default to prevent high cardinality.
///
/// For custom configuration (such as enabling `"program_id"` or specifying a static program name),
/// use [`MetricsAdapter`].
pub fn observe_metrics_telemetry<Msg>(envelope: TelemetryEnvelope<'_, Msg>) {
    MetricsAdapter::default().observe(envelope);
}

#[cfg(feature = "tracing")]
fn option_arc_str(value: Option<&Arc<str>>) -> &str {
    value.map_or("<none>", Arc::as_ref)
}

#[cfg(all(test, feature = "tracing"))]
mod tracing_tests {
    use super::*;
    use crate::observability::{ProgramId, TelemetryEnvelope, TelemetryEvent, TelemetryMetadata};
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

    #[test]
    fn tracing_adapter_emits_event() {
        let count = Arc::new(AtomicUsize::new(0));
        let subscriber = tracing_subscriber::registry().with(CountLayer {
            count: count.clone(),
        });

        tracing::subscriber::with_default(subscriber, || {
            observe_tracing_telemetry::<()>(TelemetryEnvelope {
                metadata: TelemetryMetadata {
                    program_id: ProgramId::from_raw(1),
                    program_description: Some(Arc::from("program")),
                    event_id: 1,
                    emitted_at: std::time::SystemTime::now(),
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
    use crate::{
        CommandKind,
        observability::{
            ProgramId, QueueOverflowAction, QueuePolicy, TelemetryEnvelope, TelemetryEvent,
            TelemetryMetadata,
        },
    };
    use metrics::{
        Counter, CounterFn, Gauge, GaugeFn, Histogram, HistogramFn, Key, KeyName, Metadata,
        Recorder, SharedString, Unit,
    };
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Debug)]
    struct RecordingHandle {
        key: Key,
        seen: Arc<Mutex<Vec<Key>>>,
    }

    impl RecordingHandle {
        fn record(&self) {
            self.seen.lock().unwrap().push(self.key.clone());
        }
    }

    impl CounterFn for RecordingHandle {
        fn increment(&self, _value: u64) {
            self.record();
        }

        fn absolute(&self, _value: u64) {
            self.record();
        }
    }

    impl GaugeFn for RecordingHandle {
        fn increment(&self, _value: f64) {
            self.record();
        }

        fn decrement(&self, _value: f64) {
            self.record();
        }

        fn set(&self, _value: f64) {
            self.record();
        }
    }

    impl HistogramFn for RecordingHandle {
        fn record(&self, _value: f64) {
            self.record();
        }
    }

    #[derive(Debug)]
    struct RecordingRecorder {
        seen: Arc<Mutex<Vec<Key>>>,
    }

    impl Recorder for RecordingRecorder {
        fn describe_counter(
            &self,
            _key_name: KeyName,
            _unit: Option<Unit>,
            _description: SharedString,
        ) {
        }

        fn describe_gauge(
            &self,
            _key_name: KeyName,
            _unit: Option<Unit>,
            _description: SharedString,
        ) {
        }

        fn describe_histogram(
            &self,
            _key_name: KeyName,
            _unit: Option<Unit>,
            _description: SharedString,
        ) {
        }

        fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
            Counter::from_arc(Arc::new(RecordingHandle {
                key: key.clone(),
                seen: self.seen.clone(),
            }))
        }

        fn register_gauge(&self, key: &Key, _metadata: &Metadata<'_>) -> Gauge {
            Gauge::from_arc(Arc::new(RecordingHandle {
                key: key.clone(),
                seen: self.seen.clone(),
            }))
        }

        fn register_histogram(&self, key: &Key, _metadata: &Metadata<'_>) -> Histogram {
            Histogram::from_arc(Arc::new(RecordingHandle {
                key: key.clone(),
                seen: self.seen.clone(),
            }))
        }
    }

    fn has_label(key: &Key, label_key: &str, label_value: &str) -> bool {
        key.labels()
            .any(|label| label.key() == label_key && label.value() == label_value)
    }

    fn has_any_label(key: &Key, label_key: &str) -> bool {
        key.labels().any(|label| label.key() == label_key)
    }

    #[test]
    fn metrics_adapter_accepts_envelope() {
        observe_metrics_telemetry::<()>(TelemetryEnvelope {
            metadata: TelemetryMetadata {
                program_id: ProgramId::from_raw(1),
                program_description: Some(Arc::from("program")),
                event_id: 1,
                emitted_at: std::time::SystemTime::now(),
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

    #[test]
    fn metrics_adapter_default_uses_program_description_and_omits_program_id() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorder = RecordingRecorder { seen: seen.clone() };

        metrics::with_local_recorder(&recorder, || {
            observe_metrics_telemetry::<()>(TelemetryEnvelope {
                metadata: TelemetryMetadata {
                    program_id: ProgramId::from_raw(7),
                    program_description: Some(Arc::from("counter_app")),
                    event_id: 1,
                    emitted_at: std::time::SystemTime::now(),
                    queue_depth: 3,
                },
                event: TelemetryEvent::SubscriptionsReconciled {
                    active: 2,
                    added: 1,
                    removed: 0,
                    retained: 1,
                },
            });

            observe_metrics_telemetry::<()>(TelemetryEnvelope {
                metadata: TelemetryMetadata {
                    program_id: ProgramId::from_raw(7),
                    program_description: Some(Arc::from("counter_app")),
                    event_id: 2,
                    emitted_at: std::time::SystemTime::now(),
                    queue_depth: 1,
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
        });

        let seen = seen.lock().unwrap();
        // Uses "program" label with value "counter_app"
        assert!(seen.iter().any(|key| {
            key.name() == "gpui_tea.queue.depth" && has_label(key, "program", "counter_app")
        }));
        assert!(seen.iter().any(|key| {
            key.name() == "gpui_tea.subscription.active" && has_label(key, "program", "counter_app")
        }));
        assert!(seen.iter().any(|key| {
            key.name() == "gpui_tea.effect.completed"
                && has_label(key, "program", "counter_app")
                && has_label(key, "kind", "background")
        }));
        // program_id is NOT included by default (cardinality safe)
        assert!(seen.iter().all(|key| !has_any_label(key, "program_id")));
        assert!(
            seen.iter()
                .all(|key| !has_any_label(key, "program_description"))
        );
    }

    #[test]
    fn metrics_adapter_omits_program_label_when_no_description_or_name() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorder = RecordingRecorder { seen: seen.clone() };

        metrics::with_local_recorder(&recorder, || {
            observe_metrics_telemetry::<()>(TelemetryEnvelope {
                metadata: TelemetryMetadata {
                    program_id: ProgramId::from_raw(99),
                    program_description: None,
                    event_id: 1,
                    emitted_at: std::time::SystemTime::now(),
                    queue_depth: 0,
                },
                event: TelemetryEvent::DispatchAccepted {
                    message_description: None,
                },
            });
        });

        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2); // queue.depth and dispatch.accepted
        assert!(seen.iter().all(|key| !has_any_label(key, "program")));
        assert!(seen.iter().all(|key| !has_any_label(key, "program_id")));
    }

    #[test]
    fn metrics_adapter_with_configured_program_name() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorder = RecordingRecorder { seen: seen.clone() };
        let adapter = MetricsAdapter::new().program_name("configured_name");

        metrics::with_local_recorder(&recorder, || {
            adapter.observe::<()>(TelemetryEnvelope {
                metadata: TelemetryMetadata {
                    program_id: ProgramId::from_raw(12),
                    program_description: None,
                    event_id: 1,
                    emitted_at: std::time::SystemTime::now(),
                    queue_depth: 2,
                },
                event: TelemetryEvent::DispatchAccepted {
                    message_description: None,
                },
            });
        });

        let seen = seen.lock().unwrap();
        assert!(seen.iter().any(|key| {
            key.name() == "gpui_tea.dispatch.accepted"
                && has_label(key, "program", "configured_name")
        }));
        assert!(seen.iter().all(|key| !has_any_label(key, "program_id")));
    }

    #[test]
    fn metrics_adapter_attaches_program_id_when_enabled() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorder = RecordingRecorder { seen: seen.clone() };
        let adapter = MetricsAdapter::new()
            .include_program_id(true)
            .program_name("my_app");

        metrics::with_local_recorder(&recorder, || {
            adapter.observe::<()>(TelemetryEnvelope {
                metadata: TelemetryMetadata {
                    program_id: ProgramId::from_raw(42),
                    program_description: None,
                    event_id: 1,
                    emitted_at: std::time::SystemTime::now(),
                    queue_depth: 0,
                },
                event: TelemetryEvent::QueueOverflow {
                    policy: QueuePolicy::Unbounded,
                    action: QueueOverflowAction::RejectedNew,
                    message_description: None,
                },
            });
        });

        let seen = seen.lock().unwrap();
        assert!(seen.iter().any(|key| {
            key.name() == "gpui_tea.queue.overflow"
                && has_label(key, "program", "my_app")
                && has_label(key, "program_id", "42")
                && has_label(key, "action", "rejected_new")
        }));
    }

    #[test]
    fn metrics_adapter_config_builder_api() {
        let config = MetricsAdapterConfig::new()
            .include_program_id(true)
            .program_name("test_program");

        assert!(config.is_program_id_included());
        assert_eq!(config.program_name_label(), Some("test_program"));

        let adapter = config.build();
        assert!(adapter.config().is_program_id_included());
        assert_eq!(adapter.config().program_name_label(), Some("test_program"));

        let adapter2 = MetricsAdapter::default().include_program_id(false);
        assert!(!adapter2.config().is_program_id_included());
    }

    #[test]
    fn metrics_adapter_from_config() {
        let config = MetricsAdapterConfig::new()
            .include_program_id(true)
            .program_name("from_config");
        let adapter = MetricsAdapter::from(config);
        assert!(adapter.config().is_program_id_included());
        assert_eq!(adapter.config().program_name_label(), Some("from_config"));
    }

    #[test]
    fn metrics_adapter_empty_program_name_handling() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorder = RecordingRecorder { seen: seen.clone() };

        // Case 1: program_name is empty string, program_description is provided -> falls back to description
        let adapter = MetricsAdapter::new().program_name("");
        metrics::with_local_recorder(&recorder, || {
            adapter.observe::<()>(TelemetryEnvelope {
                metadata: TelemetryMetadata {
                    program_id: ProgramId::from_raw(1),
                    program_description: Some(Arc::from("fallback_desc")),
                    event_id: 1,
                    emitted_at: std::time::SystemTime::now(),
                    queue_depth: 0,
                },
                event: TelemetryEvent::DispatchAccepted {
                    message_description: None,
                },
            });
        });

        {
            let seen = seen.lock().unwrap();
            assert!(seen.iter().any(|key| {
                key.name() == "gpui_tea.dispatch.accepted"
                    && has_label(key, "program", "fallback_desc")
            }));
        }

        // Case 2: program_name is whitespace, program_description is None -> omits "program" label
        let seen2 = Arc::new(Mutex::new(Vec::new()));
        let recorder2 = RecordingRecorder {
            seen: seen2.clone(),
        };
        let adapter2 = MetricsAdapter::new().program_name("   ");
        metrics::with_local_recorder(&recorder2, || {
            adapter2.observe::<()>(TelemetryEnvelope {
                metadata: TelemetryMetadata {
                    program_id: ProgramId::from_raw(2),
                    program_description: None,
                    event_id: 2,
                    emitted_at: std::time::SystemTime::now(),
                    queue_depth: 0,
                },
                event: TelemetryEvent::DispatchAccepted {
                    message_description: None,
                },
            });
        });

        {
            let seen2 = seen2.lock().unwrap();
            assert!(seen2.iter().all(|key| !has_any_label(key, "program")));
        }
    }
}
