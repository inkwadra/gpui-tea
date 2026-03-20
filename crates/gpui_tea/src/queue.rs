use crate::observability::{QueueOverflowAction, QueuePolicy};
use std::{
    collections::{HashSet, VecDeque},
    sync::Mutex,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// Internal identifier for a queued message.
pub(crate) struct QueueEntryId(pub(crate) u64);

/// Internal wrapper for a queued message.
#[derive(Debug)]
pub(crate) struct QueuedMessage<Msg> {
    pub(crate) id: QueueEntryId,
    pub(crate) message: Msg,
}

/// Internal message queue.
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

/// Internal reservation state for a queue.
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

/// Internal control state for a queue.
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

/// Internal tracker for a queue.
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

    pub(crate) fn reserve_enqueue(&self) -> QueueReservation {
        self.control.lock().unwrap().reserve_enqueue()
    }

    pub(crate) fn rollback_enqueue(&self, reservation: QueueReservation) {
        self.control.lock().unwrap().rollback_enqueue(reservation);
    }

    pub(crate) fn complete_processed(&self, id: QueueEntryId) {
        self.control.lock().unwrap().complete_processed(id);
    }

    pub(crate) fn take_if_dropped(&self, id: QueueEntryId) -> bool {
        self.control.lock().unwrap().take_if_dropped(id)
    }

    pub(crate) fn depth(&self) -> usize {
        self.control.lock().unwrap().depth()
    }
}
