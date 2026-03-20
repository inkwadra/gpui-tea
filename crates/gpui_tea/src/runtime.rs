use crate::Dispatcher;
use crate::keyed_tasks::KeyedTasks;
use crate::observability::Observability;
use crate::queue::{MessageQueue, QueueTracker};
use crate::subscription_registry::SubscriptionRegistry;
use gpui::Task;
use std::sync::Arc;

/// Internal runtime state.
pub(crate) struct Runtime<Msg: Send + 'static> {
    pub(crate) queue: MessageQueue<Msg>,
    pub(crate) queue_tracker: Arc<QueueTracker>,
    pub(crate) tasks: KeyedTasks,
    pub(crate) subscriptions: SubscriptionRegistry,
    pub(crate) dispatcher: Dispatcher<Msg>,
    pub(crate) observability: Observability<Msg>,
    pub(crate) _receive_task: Task<()>,
}

impl<Msg: Send + 'static> Runtime<Msg> {
    pub(crate) fn new(
        dispatcher: Dispatcher<Msg>,
        queue_tracker: Arc<QueueTracker>,
        receive_task: Task<()>,
        observability: Observability<Msg>,
    ) -> Self {
        Self {
            queue: MessageQueue::default(),
            queue_tracker,
            tasks: KeyedTasks::default(),
            subscriptions: SubscriptionRegistry::default(),
            dispatcher,
            observability,
            _receive_task: receive_task,
        }
    }
}
