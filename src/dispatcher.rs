use crate::{
    Error, Result,
    observability::{Observability, QueueOverflowAction, RuntimeEvent, TelemetryEvent},
    runtime::{QueueReservation, QueueTracker, QueuedMessage},
};
use futures::channel::mpsc::UnboundedSender;
use std::fmt;
use std::sync::Arc;

type DispatchFn<Msg> = Arc<dyn Fn(Msg) -> Result<()> + Send + Sync>;

/// Send messages back into a mounted [`crate::Program`].
///
/// Dispatchers are enqueue-only handles. Calling [`Dispatcher::dispatch`] never
/// runs model logic inline; it only appends to the program's FIFO queue.
///
/// Cloning a dispatcher is cheap. All clones target the same underlying queue
/// and share the same shutdown behavior.
pub struct Dispatcher<Msg> {
    dispatch: DispatchFn<Msg>,
}

impl<Msg: Send + 'static> Dispatcher<Msg> {
    pub(crate) fn new(
        sender: UnboundedSender<QueuedMessage<Msg>>,
        queue_tracker: Arc<QueueTracker>,
        observability: Observability<Msg>,
    ) -> Self {
        Self {
            dispatch: Arc::new(move |msg: Msg| {
                let message_description = observability.describe_message_value(&msg);
                let reservation = queue_tracker.reserve_enqueue();

                match reservation {
                    QueueReservation::Rejected => {
                        observability.observe_telemetry(TelemetryEvent::QueueOverflow {
                            policy: observability.queue_policy(),
                            action: QueueOverflowAction::RejectedNew,
                            message_description,
                        });
                        Err(Error::QueueFull {
                            policy: observability.queue_policy(),
                        })
                    }
                    QueueReservation::DroppedNewest => {
                        observability.observe_telemetry(TelemetryEvent::QueueOverflow {
                            policy: observability.queue_policy(),
                            action: QueueOverflowAction::DroppedNewest,
                            message_description,
                        });
                        Ok(())
                    }
                    accepted @ QueueReservation::Accepted {
                        id,
                        overflow_action,
                        ..
                    } => {
                        let result = sender
                            .unbounded_send(QueuedMessage { id, message: msg })
                            .map_err(|_| Error::ProgramUnavailable);

                        if let Ok(()) = &result {
                            if let Some(action) = overflow_action {
                                observability.observe_telemetry(TelemetryEvent::QueueOverflow {
                                    policy: observability.queue_policy(),
                                    action,
                                    message_description: message_description.clone(),
                                });
                            }
                            observability.observe_runtime(RuntimeEvent::DispatchAccepted {
                                message_description: message_description.clone(),
                            });
                            observability.observe_telemetry(TelemetryEvent::DispatchAccepted {
                                message_description,
                            });
                        } else {
                            queue_tracker.rollback_enqueue(accepted);
                            observability.observe_runtime(RuntimeEvent::DispatchRejected {
                                message_description: message_description.clone(),
                            });
                            observability.observe_telemetry(TelemetryEvent::DispatchRejected {
                                message_description,
                            });
                        }

                        result
                    }
                }
            }),
        }
    }

    /// Enqueue a message to the program in FIFO order.
    ///
    /// Dispatching only appends to the queue. The update loop runs later on the
    /// foreground thread, so dispatching from inside an update or view callback
    /// never recurses into [`crate::Model::update`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::ProgramUnavailable`] when the target program has
    /// already been released and can no longer accept messages.
    ///
    /// Returns [`Error::QueueFull`] when the configured queue policy rejects
    /// new messages.
    pub fn dispatch(&self, msg: Msg) -> Result<()> {
        (self.dispatch)(msg)
    }

    /// Transform a child or derived message type into this dispatcher's message
    /// type.
    ///
    /// This is useful when a subview wants to work with a narrower message type
    /// while the outer program still expects the model's full message enum.
    pub fn map<F, NewMsg>(&self, f: F) -> Dispatcher<NewMsg>
    where
        F: Fn(NewMsg) -> Msg + Clone + Send + Sync + 'static,
        NewMsg: Send + 'static,
    {
        let inner = self.dispatch.clone();

        Dispatcher {
            dispatch: Arc::new(move |msg| inner(f(msg))),
        }
    }
}

impl<Msg> Clone for Dispatcher<Msg> {
    fn clone(&self) -> Self {
        Self {
            dispatch: self.dispatch.clone(),
        }
    }
}

impl<Msg> fmt::Debug for Dispatcher<Msg> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("Dispatcher").field(&"..").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        observability::{ProgramConfig, QueuePolicy},
        runtime::QueueTracker,
    };
    use futures::channel::mpsc::unbounded;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {
        Set(i32),
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum DispatchEvent {
        Accepted(Option<String>),
        Rejected(Option<String>),
        Overflow(String),
    }

    #[allow(clippy::missing_panics_doc)]
    #[test]
    fn dispatch_reports_acceptance_and_rejection_with_message_descriptions() {
        let dispatch_events = Arc::new(Mutex::new(Vec::new()));
        let config = ProgramConfig::default()
            .describe_message(|msg: &Msg| match msg {
                Msg::Set(value) => format!("set:{value}"),
            })
            .observer({
                let dispatch_events = dispatch_events.clone();
                move |event| match event {
                    RuntimeEvent::DispatchAccepted {
                        message_description,
                    } => dispatch_events
                        .lock()
                        .unwrap()
                        .push(DispatchEvent::Accepted(
                            message_description.map(|value| value.to_string()),
                        )),
                    RuntimeEvent::DispatchRejected {
                        message_description,
                    } => dispatch_events
                        .lock()
                        .unwrap()
                        .push(DispatchEvent::Rejected(
                            message_description.map(|value| value.to_string()),
                        )),
                    _ => {}
                }
            })
            .telemetry_observer({
                let dispatch_events = dispatch_events.clone();
                move |event| {
                    if let TelemetryEvent::QueueOverflow {
                        message_description,
                        ..
                    } = event.event
                    {
                        dispatch_events
                            .lock()
                            .unwrap()
                            .push(DispatchEvent::Overflow(message_description.map_or_else(
                                || String::from("<none>"),
                                |value| value.to_string(),
                            )));
                    }
                }
            });

        let queue_tracker = Arc::new(QueueTracker::new(QueuePolicy::Unbounded));
        let observability = Observability::new(
            config,
            Arc::new({
                let queue_tracker = queue_tracker.clone();
                move || queue_tracker.depth()
            }),
        );
        let (sender, receiver) = unbounded();
        let dispatcher = Dispatcher::new(sender, queue_tracker, observability);

        assert_eq!(dispatcher.dispatch(Msg::Set(1)), Ok(()));
        drop(receiver);

        let error = dispatcher.dispatch(Msg::Set(2)).unwrap_err();
        assert_eq!(error, Error::ProgramUnavailable);
        assert_eq!(error.to_string(), "program is no longer available");
        assert_eq!(
            dispatch_events.lock().unwrap().as_slice(),
            &[
                DispatchEvent::Accepted(Some(String::from("set:1"))),
                DispatchEvent::Rejected(Some(String::from("set:2"))),
            ]
        );
    }

    #[allow(clippy::missing_panics_doc)]
    #[test]
    fn dispatch_returns_queue_full_when_policy_rejects_new_messages() {
        let config =
            ProgramConfig::<Msg>::default().queue_policy(QueuePolicy::RejectNew { capacity: 0 });
        let queue_tracker = Arc::new(QueueTracker::new(QueuePolicy::RejectNew { capacity: 0 }));
        let observability = Observability::new(
            config,
            Arc::new({
                let queue_tracker = queue_tracker.clone();
                move || queue_tracker.depth()
            }),
        );
        let (sender, _receiver) = unbounded();
        let dispatcher = Dispatcher::new(sender, queue_tracker, observability);

        let error = dispatcher.dispatch(Msg::Set(7)).unwrap_err();
        assert_eq!(
            error,
            Error::QueueFull {
                policy: QueuePolicy::RejectNew { capacity: 0 },
            }
        );
    }
}
