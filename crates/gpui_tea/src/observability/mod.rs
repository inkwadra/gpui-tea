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

#[cfg(feature = "tracing")]
pub use adapters::observe_tracing_telemetry;
#[cfg(feature = "metrics")]
pub use adapters::{MetricsAdapter, MetricsAdapterConfig, observe_metrics_telemetry};
pub use config::{ProgramConfig, ProgramId};
pub use events::{RuntimeEvent, TelemetryEnvelope, TelemetryEvent, TelemetryMetadata};
pub use policy::{QueueOverflowAction, QueuePolicy};

type QueueDepthFn = Arc<dyn Fn() -> usize + Send + Sync>;
static NEXT_EVENT_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) struct Observability<Msg> {
    config: ProgramConfig<Msg>,
    program_id: ProgramId,
    program_description: Option<Arc<str>>,
    queue_depth: QueueDepthFn,
}

impl<Msg> Clone for Observability<Msg> {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            program_id: self.program_id,
            program_description: self.program_description.clone(),
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

    pub(crate) fn queue_policy(&self) -> QueuePolicy {
        self.config.queue_policy
    }

    pub(crate) fn drain_budget(&self) -> usize {
        self.config.drain_budget
    }

    /// Emit a queue warning when `queued` exceeds the configured threshold.
    pub(crate) fn observe_queue_depth(&self, queued: usize) {
        let Some(threshold) = self.config.queue_warning_threshold else {
            return;
        };
        if queued <= threshold {
            return;
        }

        self.observe_runtime(RuntimeEvent::QueueWarning { queued, threshold });
        self.observe_telemetry(TelemetryEvent::QueueWarning { queued, threshold });
    }

    /// Emit an overflow event for `action` to both observers.
    pub(crate) fn observe_queue_overflow(
        &self,
        action: QueueOverflowAction,
        message_description: Option<Arc<str>>,
    ) {
        let policy = self.queue_policy();
        self.observe_runtime(RuntimeEvent::QueueOverflow {
            policy,
            action,
            message_description: message_description.clone(),
        });
        self.observe_telemetry(TelemetryEvent::QueueOverflow {
            policy,
            action,
            message_description,
        });
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
                    event_id: NEXT_EVENT_ID.fetch_add(1, Ordering::Relaxed),
                    emitted_at: SystemTime::now(),
                    queue_depth: self.queue_depth(),
                },
                event,
            });
        }
    }
}
