use crate::{CommandKind, Key};
use gpui::Task;
use std::{collections::HashMap, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Internal generation counter for tasks.
pub(crate) struct TaskGeneration(pub(crate) u64);

impl TaskGeneration {
    pub(crate) fn next(counter: &mut u64) -> Self {
        *counter = counter.wrapping_add(1);
        Self(*counter)
    }

    #[cfg(test)]
    pub(crate) fn previous(self) -> Self {
        Self(self.0.wrapping_sub(1))
    }
}

/// Internal metadata for a command.
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

/// Internal context for a keyed effect.
#[derive(Clone)]
pub(crate) struct KeyedEffectContext {
    pub(crate) key: Key,
    pub(crate) generation: TaskGeneration,
}

/// Internal state for a running task.
pub(crate) struct RunningTask {
    pub(crate) generation: TaskGeneration,
    pub(crate) meta: CommandMeta,
    pub(crate) task: Task<()>,
}

/// Internal registry for keyed tasks.
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
