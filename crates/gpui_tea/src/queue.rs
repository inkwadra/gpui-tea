use crate::observability::{QueueOverflowAction, QueuePolicy};
use std::{
    collections::{HashSet, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
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
///
/// Control flags use shared atomics so RAII guards can restore them on panic without
/// holding a borrow of the queue (which would conflict with nested `&mut` access).
pub(crate) struct MessageQueue<Msg> {
    pub(crate) pending: VecDeque<QueuedMessage<Msg>>,
    is_draining: Arc<AtomicBool>,
    auto_drain_suspended_depth: Arc<AtomicUsize>,
}

impl<Msg> Default for MessageQueue<Msg> {
    fn default() -> Self {
        Self {
            pending: VecDeque::new(),
            is_draining: Arc::new(AtomicBool::new(false)),
            auto_drain_suspended_depth: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl<Msg> MessageQueue<Msg> {
    pub(crate) fn is_draining(&self) -> bool {
        self.is_draining.load(Ordering::Acquire)
    }

    pub(crate) fn should_auto_drain(&self) -> bool {
        !self.is_draining() && self.auto_drain_suspended_depth.load(Ordering::Acquire) == 0
    }
}

/// Restore auto-drain suspension depth even if the guarded body panics.
pub(crate) struct AutoDrainGuard {
    depth: Arc<AtomicUsize>,
}

impl AutoDrainGuard {
    pub(crate) fn suspend<Msg>(queue: &MessageQueue<Msg>) -> Self {
        let depth = queue.auto_drain_suspended_depth.clone();
        let previous = depth.fetch_add(1, Ordering::AcqRel);
        assert!(
            previous < usize::MAX,
            "auto-drain suspension depth overflow"
        );
        Self { depth }
    }
}

impl Drop for AutoDrainGuard {
    fn drop(&mut self) {
        self.depth
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                Some(value.saturating_sub(1))
            })
            .ok();
    }
}

/// Clear `is_draining` even if the guarded body panics.
pub(crate) struct DrainGuard {
    flag: Arc<AtomicBool>,
}

impl DrainGuard {
    pub(crate) fn start<Msg>(queue: &MessageQueue<Msg>) -> Self {
        queue.is_draining.store(true, Ordering::Release);
        Self {
            flag: queue.is_draining.clone(),
        }
    }
}

impl Drop for DrainGuard {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Release);
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
            QueuePolicy::Unbounded => {
                let id = self.next_id();
                self.active.push_back(id);
                QueueReservation::Accepted {
                    id,
                    overflow_action: None,
                    displaced: None,
                }
            }
            QueuePolicy::RejectNew { .. } => QueueReservation::Rejected,
            QueuePolicy::DropNewest { .. } => QueueReservation::DroppedNewest,
            QueuePolicy::DropOldest { .. } => {
                // capacity == 0 (or a temporarily empty queue at capacity) still accepts by
                // displacing the oldest entry when one exists; with nothing to displace the
                // newest message is kept so DropOldest never silently drops everything.
                let Some(oldest) = self.active.pop_front() else {
                    let id = self.next_id();
                    self.active.push_back(id);
                    return QueueReservation::Accepted {
                        id,
                        overflow_action: None,
                        displaced: None,
                    };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observability::QueuePolicy;

    #[test]
    fn drop_oldest_capacity_zero_accepts_when_nothing_to_displace() {
        let tracker = QueueTracker::new(QueuePolicy::DropOldest { capacity: 0 });
        let reservation = tracker.reserve_enqueue();
        assert!(matches!(
            reservation,
            QueueReservation::Accepted {
                overflow_action: None,
                displaced: None,
                ..
            }
        ));
        assert_eq!(tracker.depth(), 1);
    }

    #[test]
    fn drop_oldest_capacity_zero_displaces_previous_entry() {
        let tracker = QueueTracker::new(QueuePolicy::DropOldest { capacity: 0 });
        let first = tracker.reserve_enqueue();
        let second = tracker.reserve_enqueue();

        let QueueReservation::Accepted { id: first_id, .. } = first else {
            panic!("first reservation must be accepted");
        };
        assert!(matches!(
            second,
            QueueReservation::Accepted {
                overflow_action: Some(QueueOverflowAction::DroppedOldest),
                ..
            }
        ));
        assert!(tracker.take_if_dropped(first_id));
        assert_eq!(tracker.depth(), 1);
    }

    #[test]
    fn drain_guard_resets_flag_on_drop() {
        let queue = MessageQueue::<()>::default();
        assert!(!queue.is_draining());
        {
            let _guard = DrainGuard::start(&queue);
            assert!(queue.is_draining());
            assert!(!queue.should_auto_drain());
        }
        assert!(!queue.is_draining());
        assert!(queue.should_auto_drain());
    }

    #[test]
    fn auto_drain_guard_resets_depth_on_drop() {
        let queue = MessageQueue::<()>::default();
        assert!(queue.should_auto_drain());
        {
            let _outer = AutoDrainGuard::suspend(&queue);
            {
                let _inner = AutoDrainGuard::suspend(&queue);
                assert!(!queue.should_auto_drain());
            }
            assert!(!queue.should_auto_drain());
        }
        assert!(queue.should_auto_drain());
    }
}
