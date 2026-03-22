use crate::Dispatcher;
use crate::keyed_tasks::KeyedTasks;
use crate::observability::Observability;
use crate::queue::{MessageQueue, QueueTracker};
use crate::subscription_registry::SubscriptionRegistry;
use gpui::Task;
use std::{collections::HashMap, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// Internal identifier for a non-keyed running effect.
pub(crate) struct EffectId(u64);

impl EffectId {
    fn next(counter: &mut u64) -> Self {
        *counter = counter.wrapping_add(1);
        Self(*counter)
    }
}

/// Internal registry for non-keyed running effects.
#[derive(Default)]
pub(crate) struct PendingEffects {
    next_effect_id: u64,
    active: HashMap<EffectId, Task<()>>,
}

impl PendingEffects {
    pub(crate) fn next_id(&mut self) -> EffectId {
        EffectId::next(&mut self.next_effect_id)
    }

    pub(crate) fn insert(&mut self, id: EffectId, task: Task<()>) {
        let replaced = self.active.insert(id, task);
        debug_assert!(replaced.is_none(), "effect ids must be unique while active");
    }

    pub(crate) fn finish(&mut self, id: EffectId) -> bool {
        self.active.remove(&id).is_some()
    }
}

/// Internal runtime state.
pub(crate) struct Runtime<Msg: Send + 'static> {
    pub(crate) queue: MessageQueue<Msg>,
    pub(crate) queue_tracker: Arc<QueueTracker>,
    pub(crate) tasks: KeyedTasks,
    pub(crate) effects: PendingEffects,
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
            effects: PendingEffects::default(),
            subscriptions: SubscriptionRegistry::default(),
            dispatcher,
            observability,
            _receive_task: receive_task,
        }
    }
}
