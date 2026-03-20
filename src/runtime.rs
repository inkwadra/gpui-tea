use crate::command::CommandKind;
use crate::observability::{
    Observability, QueueOverflowAction, QueuePolicy, RuntimeEvent, TelemetryEvent,
};
use crate::{Dispatcher, Key, SubHandle, SubscriptionContext, Subscriptions};
use gpui::{Context, Task};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct QueueEntryId(u64);

#[derive(Debug)]
pub(crate) struct QueuedMessage<Msg> {
    pub(crate) id: QueueEntryId,
    pub(crate) message: Msg,
}

pub(crate) struct MessageQueue<Msg> {
    pub(crate) pending: VecDeque<QueuedMessage<Msg>>,
    pub(crate) is_draining: bool,
}

impl<Msg> Default for MessageQueue<Msg> {
    fn default() -> Self {
        Self {
            pending: VecDeque::new(),
            is_draining: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TaskGeneration(u64);

impl TaskGeneration {
    fn next(counter: &mut u64) -> Self {
        *counter = counter.wrapping_add(1);
        Self(*counter)
    }

    #[cfg(test)]
    pub(crate) fn previous(self) -> Self {
        Self(self.0.wrapping_sub(1))
    }
}

#[derive(Clone)]
pub(crate) struct CommandMeta {
    pub(crate) kind: CommandKind,
    label: Option<Arc<str>>,
}

impl CommandMeta {
    pub(crate) fn new(kind: CommandKind, label: Option<Arc<str>>) -> Self {
        Self { kind, label }
    }

    pub(crate) fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }
}

#[derive(Clone)]
pub(crate) struct KeyedEffectContext {
    pub(crate) key: Key,
    pub(crate) generation: TaskGeneration,
}

pub(crate) struct RunningTask {
    pub(crate) generation: TaskGeneration,
    pub(crate) meta: CommandMeta,
    pub(crate) task: Task<()>,
}

#[derive(Default)]
pub(crate) struct KeyedTasks {
    next_generation: u64,
    active: HashMap<Key, RunningTask>,
}

impl KeyedTasks {
    pub(crate) fn next_generation(&mut self) -> TaskGeneration {
        TaskGeneration::next(&mut self.next_generation)
    }

    pub(crate) fn insert(
        &mut self,
        key: Key,
        generation: TaskGeneration,
        meta: CommandMeta,
        task: Task<()>,
    ) -> Option<RunningTask> {
        self.active.insert(
            key,
            RunningTask {
                generation,
                meta,
                task,
            },
        )
    }

    pub(crate) fn is_current(&self, key: &Key, generation: TaskGeneration) -> bool {
        self.active
            .get(key)
            .is_some_and(|active| active.generation == generation)
    }

    pub(crate) fn clear_current(&mut self, key: &Key, generation: TaskGeneration) {
        if self.is_current(key, generation) {
            self.active.remove(key);
        }
    }

    pub(crate) fn cancel(&mut self, key: &Key) -> Option<RunningTask> {
        self.active.remove(key)
    }

    pub(crate) fn len(&self) -> usize {
        self.active.len()
    }
}

struct ActiveSubscription {
    handle: SubHandle,
    label: Option<Arc<str>>,
}

#[derive(Default)]
pub(crate) struct SubscriptionRegistry {
    active: HashMap<Key, ActiveSubscription>,
}

pub(crate) struct SubscriptionReconcileStats {
    pub(crate) active: usize,
    pub(crate) added: usize,
    pub(crate) removed: usize,
    pub(crate) retained: usize,
}

impl SubscriptionRegistry {
    pub(crate) fn reconcile<Msg: Send + 'static, T: 'static>(
        &mut self,
        subscriptions: Subscriptions<Msg>,
        dispatcher: &Dispatcher<Msg>,
        observability: &Observability<Msg>,
        cx: &mut Context<'_, T>,
    ) -> SubscriptionReconcileStats {
        let mut next_active = HashMap::with_capacity(subscriptions.len());
        let mut added = 0;
        let mut retained = 0;

        for subscription in subscriptions {
            let key = subscription.key;
            let label = subscription.label;

            if let Some(active) = self.active.remove(&key) {
                retained += 1;
                observability.observe_runtime(RuntimeEvent::SubscriptionRetained {
                    key: &key,
                    key_description: observability.describe_key_value(&key),
                    label: label.as_deref(),
                });
                observability.observe_telemetry(TelemetryEvent::SubscriptionRetained {
                    key: &key,
                    key_description: observability.describe_key_value(&key),
                    label: label.as_deref(),
                });
                next_active.insert(
                    key,
                    ActiveSubscription {
                        handle: active.handle,
                        label,
                    },
                );
            } else {
                added += 1;
                let mut subscription_context = SubscriptionContext::new(cx, dispatcher.clone());
                let handle = (subscription.builder)(&mut subscription_context);
                observability.observe_runtime(RuntimeEvent::SubscriptionBuilt {
                    key: &key,
                    key_description: observability.describe_key_value(&key),
                    label: label.as_deref(),
                });
                observability.observe_telemetry(TelemetryEvent::SubscriptionBuilt {
                    key: &key,
                    key_description: observability.describe_key_value(&key),
                    label: label.as_deref(),
                });
                next_active.insert(key, ActiveSubscription { handle, label });
            }
        }

        let removed = self.active.len();
        for (key, active) in self.active.drain() {
            observability.observe_runtime(RuntimeEvent::SubscriptionRemoved {
                key: &key,
                key_description: observability.describe_key_value(&key),
                label: active.label.as_deref(),
            });
            observability.observe_telemetry(TelemetryEvent::SubscriptionRemoved {
                key: &key,
                key_description: observability.describe_key_value(&key),
                label: active.label.as_deref(),
            });
        }
        self.active = next_active;

        SubscriptionReconcileStats {
            active: self.active.len(),
            added,
            removed,
            retained,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.active.len()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QueueReservation {
    Accepted {
        id: QueueEntryId,
        overflow_action: Option<QueueOverflowAction>,
        displaced: Option<QueueEntryId>,
    },
    Rejected,
    DroppedNewest,
}

#[derive(Debug)]
struct QueueControl {
    policy: QueuePolicy,
    next_id: u64,
    active: VecDeque<QueueEntryId>,
    dropped: HashSet<QueueEntryId>,
}

impl QueueControl {
    fn new(policy: QueuePolicy) -> Self {
        Self {
            policy,
            next_id: 0,
            active: VecDeque::new(),
            dropped: HashSet::new(),
        }
    }

    fn next_id(&mut self) -> QueueEntryId {
        self.next_id = self.next_id.wrapping_add(1);
        QueueEntryId(self.next_id)
    }

    fn reserve_enqueue(&mut self) -> QueueReservation {
        let Some(capacity) = self.policy.capacity() else {
            let id = self.next_id();
            self.active.push_back(id);
            return QueueReservation::Accepted {
                id,
                overflow_action: None,
                displaced: None,
            };
        };

        if self.active.len() < capacity {
            let id = self.next_id();
            self.active.push_back(id);
            return QueueReservation::Accepted {
                id,
                overflow_action: None,
                displaced: None,
            };
        }

        match self.policy {
            QueuePolicy::Unbounded => unreachable!("unbounded queues return early"),
            QueuePolicy::RejectNew { .. } => QueueReservation::Rejected,
            QueuePolicy::DropNewest { .. } => QueueReservation::DroppedNewest,
            QueuePolicy::DropOldest { .. } => {
                let Some(oldest) = self.active.pop_front() else {
                    return QueueReservation::DroppedNewest;
                };
                self.dropped.insert(oldest);
                let id = self.next_id();
                self.active.push_back(id);
                QueueReservation::Accepted {
                    id,
                    overflow_action: Some(QueueOverflowAction::DroppedOldest),
                    displaced: Some(oldest),
                }
            }
        }
    }

    fn rollback_enqueue(&mut self, reservation: QueueReservation) {
        let QueueReservation::Accepted {
            id,
            overflow_action,
            displaced,
        } = reservation
        else {
            return;
        };

        if let Some(position) = self.active.iter().position(|entry| *entry == id) {
            self.active.remove(position);
        }

        if overflow_action == Some(QueueOverflowAction::DroppedOldest)
            && let Some(displaced) = displaced
            && self.dropped.remove(&displaced)
        {
            self.active.push_front(displaced);
        }
    }

    fn complete_processed(&mut self, id: QueueEntryId) {
        if let Some(position) = self.active.iter().position(|entry| *entry == id) {
            self.active.remove(position);
        }
    }

    fn take_if_dropped(&mut self, id: QueueEntryId) -> bool {
        self.dropped.remove(&id)
    }

    fn depth(&self) -> usize {
        self.active.len()
    }
}

#[derive(Debug)]
pub(crate) struct QueueTracker {
    control: Mutex<QueueControl>,
}

impl QueueTracker {
    pub(crate) fn new(policy: QueuePolicy) -> Self {
        Self {
            control: Mutex::new(QueueControl::new(policy)),
        }
    }

    #[allow(clippy::missing_panics_doc)]
    pub(crate) fn reserve_enqueue(&self) -> QueueReservation {
        self.control.lock().unwrap().reserve_enqueue()
    }

    #[allow(clippy::missing_panics_doc)]
    pub(crate) fn rollback_enqueue(&self, reservation: QueueReservation) {
        self.control.lock().unwrap().rollback_enqueue(reservation);
    }

    #[allow(clippy::missing_panics_doc)]
    pub(crate) fn complete_processed(&self, id: QueueEntryId) {
        self.control.lock().unwrap().complete_processed(id);
    }

    #[allow(clippy::missing_panics_doc)]
    pub(crate) fn take_if_dropped(&self, id: QueueEntryId) -> bool {
        self.control.lock().unwrap().take_if_dropped(id)
    }

    #[allow(clippy::missing_panics_doc)]
    pub(crate) fn depth(&self) -> usize {
        self.control.lock().unwrap().depth()
    }
}

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
