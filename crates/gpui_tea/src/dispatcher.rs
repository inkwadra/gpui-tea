use crate::{
    Error, Result,
    observability::{Observability, QueueOverflowAction, RuntimeEvent, TelemetryEvent},
    queue::{QueueReservation, QueueTracker, QueuedMessage},
};
use futures::channel::mpsc::UnboundedSender;
use std::fmt;
use std::sync::Arc;

/// A type alias for the internal dispatch function.
type DispatchFn<Msg> = Arc<dyn Fn(Msg) -> Result<()> + Send + Sync>;

/// Send messages back into a mounted [`crate::Program`].
///
/// Dispatchers are cheap to clone and can be moved into event handlers, subscriptions, and async
/// effects. A dispatcher remains valid only while the owning program is mounted.
pub struct Dispatcher<Msg> {
    dispatch: DispatchFn<Msg>,
}

impl<Msg: Send + 'static> Dispatcher<Msg> {
    /// Create a new dispatcher.
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

    /// Enqueue `msg` for processing by the mounted program.
    ///
    /// # Arguments
    ///
    /// * `msg` - The message to dispatch into the program.
    ///
    /// # Errors
    ///
    /// Returns [`Error::QueueFull`] when the active [`crate::QueuePolicy`] rejects new messages.
    /// Returns [`Error::ProgramUnavailable`] when the program has already been dropped.
    ///
    /// # Returns
    ///
    /// A [`Result`] indicating success or failure of dispatching the message. Under
    /// [`crate::QueuePolicy::DropNewest`] this may still return `Ok(())` even though the new
    /// message was discarded, and under [`crate::QueuePolicy::DropOldest`] it may succeed after
    /// displacing the oldest queued message.
    pub fn dispatch(&self, msg: Msg) -> Result<()> {
        (self.dispatch)(msg)
    }

    /// Adapt this dispatcher to accept `NewMsg` values.
    ///
    /// The mapper runs synchronously at dispatch time before the message enters the runtime queue.
    ///
    /// # Arguments
    ///
    /// * `f` - A mapping closure that converts a `NewMsg` into a `Msg`.
    ///
    /// # Returns
    ///
    /// A new dispatcher that accepts `NewMsg` values and maps them before dispatching.
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
        queue::QueueTracker,
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
