#[cfg(any(feature = "tracing", feature = "metrics"))]
use super::{TelemetryEnvelope, TelemetryEvent};

#[cfg(feature = "tracing")]
use std::sync::Arc;

#[cfg(feature = "tracing")]
/// Record a [`TelemetryEnvelope`] as `tracing` events.
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
/// Record a [`TelemetryEnvelope`] as `metrics` counters and gauges.
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
        observability::{ProgramId, TelemetryEnvelope, TelemetryEvent, TelemetryMetadata},
    };
    use std::sync::Arc;

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
}
