use crate::program::{ProgramConfig, RuntimeEvent};
use futures::channel::mpsc::UnboundedSender;
use std::error::Error;
use std::fmt;
use std::sync::Arc;

/// Report that a message could not be enqueued because the program is no longer
/// alive.
///
/// This is returned only when every receiver for the program's internal queue
/// has been dropped, which happens after the mounted [`crate::Program`] has
/// been released.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DispatchError(());

impl DispatchError {
    pub(crate) fn unavailable() -> Self {
        Self(())
    }
}

impl fmt::Display for DispatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("program is no longer available")
    }
}

impl Error for DispatchError {}

type DispatchFn<Msg> = Arc<dyn Fn(Msg) -> Result<(), DispatchError> + Send + Sync>;

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
    pub(crate) fn new(sender: UnboundedSender<Msg>, config: ProgramConfig<Msg>) -> Self {
        Self {
            dispatch: Arc::new(move |msg: Msg| {
                let message_description = config.describe_message_value(&msg);
                let result = sender
                    .unbounded_send(msg)
                    .map_err(|_| DispatchError::unavailable());

                match &result {
                    Ok(()) => config.observe(RuntimeEvent::DispatchAccepted {
                        message_description,
                    }),
                    Err(_) => config.observe(RuntimeEvent::DispatchRejected {
                        message_description,
                    }),
                }

                result
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
    /// Returns [`DispatchError`] when the target program has already been
    /// released and can no longer accept messages.
    pub fn dispatch(&self, msg: Msg) -> Result<(), DispatchError> {
        (self.dispatch)(msg)
    }

    /// Transform a child or derived message type into this dispatcher's message
    /// type.
    ///
    /// This is useful when a subview wants to work with a narrower message type
    /// while the outer program still expects the model's full message enum.
    ///
    /// # Examples
    ///
    /// ```
    /// use gpui_tea::Dispatcher;
    ///
    /// enum ParentMsg {
    ///     Child(u32),
    /// }
    ///
    /// enum ChildMsg {
    ///     Clicked,
    /// }
    ///
    /// fn map_dispatcher(dispatcher: &Dispatcher<ParentMsg>) {
    ///     let child = dispatcher.map(|msg| match msg {
    ///         ChildMsg::Clicked => ParentMsg::Child(1),
    ///     });
    ///     let _ = child;
    /// }
    /// ```
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::program::RuntimeEvent;
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
                move |event: RuntimeEvent<'_, Msg>| match event {
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
            });

        let (sender, receiver) = unbounded();
        let dispatcher = Dispatcher::new(sender, config);

        assert_eq!(dispatcher.dispatch(Msg::Set(1)), Ok(()));
        drop(receiver);

        let error = dispatcher.dispatch(Msg::Set(2)).unwrap_err();
        assert_eq!(error.to_string(), "program is no longer available");
        assert_eq!(
            dispatch_events.lock().unwrap().as_slice(),
            &[
                DispatchEvent::Accepted(Some(String::from("set:1"))),
                DispatchEvent::Rejected(Some(String::from("set:2"))),
            ]
        );
    }
}
