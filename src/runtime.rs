use crate::command::CommandKind;
use crate::observability::{ProgramConfig, RuntimeEvent};
use crate::{Dispatcher, Key, SubHandle, SubscriptionContext, Subscriptions};
use gpui::{Context, Task};
use std::{collections::HashMap, sync::Arc};

pub(crate) struct MessageQueue<Msg> {
    pub(crate) pending: std::collections::VecDeque<Msg>,
    pub(crate) is_draining: bool,
}

impl<Msg> Default for MessageQueue<Msg> {
    fn default() -> Self {
        Self {
            pending: std::collections::VecDeque::new(),
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

struct RunningTask {
    generation: TaskGeneration,
    meta: CommandMeta,
    _task: Task<()>,
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
    ) -> Option<CommandMeta> {
        self.active
            .insert(
                key,
                RunningTask {
                    generation,
                    meta,
                    _task: task,
                },
            )
            .map(|running| running.meta)
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
        config: &ProgramConfig<Msg>,
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
                config.observe(RuntimeEvent::SubscriptionRetained {
                    key: &key,
                    key_description: config.describe_key_value(&key),
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
                config.observe(RuntimeEvent::SubscriptionBuilt {
                    key: &key,
                    key_description: config.describe_key_value(&key),
                    label: label.as_deref(),
                });
                next_active.insert(key, ActiveSubscription { handle, label });
            }
        }

        let removed = self.active.len();
        for (key, active) in self.active.drain() {
            config.observe(RuntimeEvent::SubscriptionRemoved {
                key: &key,
                key_description: config.describe_key_value(&key),
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
}

pub(crate) struct Runtime<Msg: Send + 'static> {
    pub(crate) queue: MessageQueue<Msg>,
    pub(crate) tasks: KeyedTasks,
    pub(crate) subscriptions: SubscriptionRegistry,
    pub(crate) dispatcher: Dispatcher<Msg>,
    pub(crate) config: ProgramConfig<Msg>,
    pub(crate) _receive_task: Task<()>,
}

impl<Msg: Send + 'static> Runtime<Msg> {
    pub(crate) fn new(
        dispatcher: Dispatcher<Msg>,
        receive_task: Task<()>,
        config: ProgramConfig<Msg>,
    ) -> Self {
        Self {
            queue: MessageQueue::default(),
            tasks: KeyedTasks::default(),
            subscriptions: SubscriptionRegistry::default(),
            dispatcher,
            config,
            _receive_task: receive_task,
        }
    }
}
