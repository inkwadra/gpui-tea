mod adapters;
mod config;
mod events;
mod policy;

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::SystemTime,
};

#[cfg(feature = "metrics")]
pub use adapters::observe_metrics_telemetry;
#[cfg(feature = "tracing")]
pub use adapters::observe_tracing_telemetry;
pub use config::{ProgramConfig, ProgramId};
pub use events::{RuntimeEvent, TelemetryEnvelope, TelemetryEvent, TelemetryMetadata};
pub use policy::{QueueOverflowAction, QueuePolicy};

type QueueDepthFn = Arc<dyn Fn() -> usize + Send + Sync>;

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

    pub(crate) fn describe_key_value(&self, key: &crate::Key) -> Option<Arc<str>> {
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
