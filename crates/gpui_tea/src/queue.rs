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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Origin of a queued message indicating whether it came from an external dispatch or internal command.
pub(crate) enum QueueOrigin {
    External(QueueEntryId),
    Internal,
}

/// Internal wrapper for a queued message.
#[derive(Debug)]
pub(crate) struct QueuedMessage<Msg> {
    pub(crate) origin: QueueOrigin,
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum QueueReservation {
    Accepted {
        id: QueueEntryId,
        overflow_action: Option<QueueOverflowAction>,
        displaced: Option<QueueEntryId>,
        dropped_message_description: Option<Arc<str>>,
    },
    Rejected,
    DroppedNewest,
}

/// Internal control state for a queue.
#[derive(Debug)]
struct QueueControl {
    policy: QueuePolicy,
    next_id: u64,
    active: VecDeque<(QueueEntryId, Option<Arc<str>>)>,
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

    fn reserve_enqueue(&mut self, description: Option<Arc<str>>) -> QueueReservation {
        let Some(capacity) = self.policy.capacity() else {
            let id = self.next_id();
            self.active.push_back((id, description));
            return QueueReservation::Accepted {
                id,
                overflow_action: None,
                displaced: None,
                dropped_message_description: None,
            };
        };

        if self.active.len() < capacity {
            let id = self.next_id();
            self.active.push_back((id, description));
            return QueueReservation::Accepted {
                id,
                overflow_action: None,
                displaced: None,
                dropped_message_description: None,
            };
        }

        match self.policy {
            QueuePolicy::Unbounded => {
                let id = self.next_id();
                self.active.push_back((id, description));
                QueueReservation::Accepted {
                    id,
                    overflow_action: None,
                    displaced: None,
                    dropped_message_description: None,
                }
            }
            QueuePolicy::RejectNew { .. } => QueueReservation::Rejected,
            QueuePolicy::DropNewest { .. } => QueueReservation::DroppedNewest,
            QueuePolicy::DropOldest { .. } => {
                // capacity == 0 (or a temporarily empty queue at capacity) still accepts by
                // displacing the oldest entry when one exists; with nothing to displace the
                // newest message is kept so DropOldest never silently drops everything.
                let Some((oldest, oldest_description)) = self.active.pop_front() else {
                    let id = self.next_id();
                    self.active.push_back((id, description));
                    return QueueReservation::Accepted {
                        id,
                        overflow_action: None,
                        displaced: None,
                        dropped_message_description: None,
                    };
                };
                self.dropped.insert(oldest);
                let id = self.next_id();
                self.active.push_back((id, description));
                QueueReservation::Accepted {
                    id,
                    overflow_action: Some(QueueOverflowAction::DroppedOldest),
                    displaced: Some(oldest),
                    dropped_message_description: oldest_description,
                }
            }
        }
    }

    fn rollback_enqueue(&mut self, reservation: &QueueReservation) {
        let QueueReservation::Accepted {
            id,
            overflow_action,
            displaced,
            dropped_message_description,
        } = reservation
        else {
            return;
        };

        if let Some(position) = self.active.iter().position(|(entry, _)| entry == id) {
            self.active.remove(position);
        }

        if *overflow_action == Some(QueueOverflowAction::DroppedOldest)
            && let Some(displaced) = displaced
            && self.dropped.remove(displaced)
        {
            self.active
                .push_front((*displaced, dropped_message_description.clone()));
        }
    }

    fn complete_processed(&mut self, id: QueueEntryId) {
        if let Some(position) = self.active.iter().position(|(entry, _)| *entry == id) {
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

    pub(crate) fn reserve_enqueue(&self, description: Option<Arc<str>>) -> QueueReservation {
        self.control.lock().unwrap().reserve_enqueue(description)
    }

    pub(crate) fn rollback_enqueue(&self, reservation: &QueueReservation) {
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
        let reservation = tracker.reserve_enqueue(None);
        assert!(matches!(
            reservation,
            QueueReservation::Accepted {
                overflow_action: None,
                displaced: None,
                dropped_message_description: None,
                ..
            }
        ));
        assert_eq!(tracker.depth(), 1);
    }

    #[test]
    fn drop_oldest_capacity_zero_displaces_previous_entry() {
        let tracker = QueueTracker::new(QueuePolicy::DropOldest { capacity: 0 });
        let first = tracker.reserve_enqueue(Some(Arc::from("first")));
        let second = tracker.reserve_enqueue(Some(Arc::from("second")));

        let QueueReservation::Accepted { id: first_id, .. } = first else {
            panic!("first reservation must be accepted");
        };
        assert_eq!(
            second,
            QueueReservation::Accepted {
                id: QueueEntryId(2),
                overflow_action: Some(QueueOverflowAction::DroppedOldest),
                displaced: Some(first_id),
                dropped_message_description: Some(Arc::from("first")),
            }
        );
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

    #[test]
    fn drain_guard_preserves_remaining_messages_on_panic() {
        let mut queue = MessageQueue::<i32>::default();
        queue.pending.push_back(QueuedMessage {
            origin: QueueOrigin::External(QueueEntryId(1)),
            message: 1,
        });
        queue.pending.push_back(QueuedMessage {
            origin: QueueOrigin::Internal,
            message: 2,
        });
        queue.pending.push_back(QueuedMessage {
            origin: QueueOrigin::External(QueueEntryId(3)),
            message: 3,
        });

        let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = DrainGuard::start(&queue);
            let first = queue.pending.pop_front().unwrap();
            assert_eq!(first.message, 1);
            panic!("panic while processing first message");
        }));
        assert!(panic_result.is_err());

        assert!(!queue.is_draining());
        assert!(queue.should_auto_drain());
        assert_eq!(queue.pending.len(), 2);
        assert_eq!(queue.pending.pop_front().unwrap().message, 2);
        assert_eq!(queue.pending.pop_front().unwrap().message, 3);
    }

    #[test]
    fn auto_drain_guard_restores_suspension_on_panic() {
        let queue = MessageQueue::<i32>::default();
        let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = AutoDrainGuard::suspend(&queue);
            assert!(!queue.should_auto_drain());
            panic!("panic inside auto drain guard");
        }));
        assert!(panic_result.is_err());
        assert!(queue.should_auto_drain());
    }

    #[test]
    fn rollback_enqueue_without_displacement_removes_entry() {
        let tracker = QueueTracker::new(QueuePolicy::Unbounded);
        let res = tracker.reserve_enqueue(Some(Arc::from("first")));
        assert_eq!(tracker.depth(), 1);
        tracker.rollback_enqueue(&res);
        assert_eq!(tracker.depth(), 0);
    }

    #[test]
    fn rollback_enqueue_restores_displaced_oldest_entry() {
        let tracker = QueueTracker::new(QueuePolicy::DropOldest { capacity: 1 });
        let first = tracker.reserve_enqueue(Some(Arc::from("first")));
        let second = tracker.reserve_enqueue(Some(Arc::from("second")));

        let QueueReservation::Accepted { id: first_id, .. } = first else {
            panic!("first must be accepted");
        };
        assert!(matches!(
            second,
            QueueReservation::Accepted {
                overflow_action: Some(QueueOverflowAction::DroppedOldest),
                displaced: Some(id),
                ..
            } if id == first_id
        ));

        // Rolling back the second reservation should restore the displaced first reservation.
        tracker.rollback_enqueue(&second);
        assert_eq!(tracker.depth(), 1);
        // The first ID should no longer be marked as dropped:
        assert!(!tracker.take_if_dropped(first_id));
    }

    #[test]
    fn interleaved_internal_and_external_preserves_fifo_order_and_protects_internal() {
        let tracker = QueueTracker::new(QueuePolicy::DropOldest { capacity: 1 });
        let mut queue = MessageQueue::<&'static str>::default();

        // 1. External message E1 arrives and is reserved
        let res1 = tracker.reserve_enqueue(Some(Arc::from("e1")));
        let QueueReservation::Accepted { id: id1, .. } = res1 else {
            panic!("e1 must be accepted");
        };
        queue.pending.push_back(QueuedMessage {
            origin: QueueOrigin::External(id1),
            message: "e1",
        });

        // 2. Internal message I1 is queued directly into pending (guaranteed delivery)
        queue.pending.push_back(QueuedMessage {
            origin: QueueOrigin::Internal,
            message: "i1",
        });

        // 3. External message E2 arrives; with capacity 1, it displaces E1 in tracker's active pool
        let res2 = tracker.reserve_enqueue(Some(Arc::from("e2")));
        let QueueReservation::Accepted {
            id: id2,
            overflow_action,
            displaced,
            ..
        } = res2
        else {
            panic!("e2 must be accepted");
        };
        assert_eq!(overflow_action, Some(QueueOverflowAction::DroppedOldest));
        assert_eq!(displaced, Some(id1));
        queue.pending.push_back(QueuedMessage {
            origin: QueueOrigin::External(id2),
            message: "e2",
        });

        // 4. Internal message I2 is queued directly into pending
        queue.pending.push_back(QueuedMessage {
            origin: QueueOrigin::Internal,
            message: "i2",
        });

        // Drain the pending queue and simulate program drain logic
        let mut processed = Vec::new();
        while let Some(queued) = queue.pending.pop_front() {
            if let QueueOrigin::External(id) = queued.origin {
                if tracker.take_if_dropped(id) {
                    continue;
                }
                tracker.complete_processed(id);
            }
            processed.push(queued.message);
        }

        // E1 was displaced, I1 was kept, E2 was processed, I2 was kept. FIFO preserved!
        assert_eq!(processed, vec!["i1", "e2", "i2"]);
        assert_eq!(tracker.depth(), 0);
    }

    #[test]
    fn reject_new_saturation_rejects_further_entries() {
        let tracker = QueueTracker::new(QueuePolicy::RejectNew { capacity: 2 });
        let first = tracker.reserve_enqueue(Some(Arc::from("first")));
        assert!(matches!(first, QueueReservation::Accepted { .. }));
        assert_eq!(tracker.depth(), 1);

        let second = tracker.reserve_enqueue(Some(Arc::from("second")));
        assert!(matches!(second, QueueReservation::Accepted { .. }));
        assert_eq!(tracker.depth(), 2);

        let third = tracker.reserve_enqueue(Some(Arc::from("third")));
        assert_eq!(third, QueueReservation::Rejected);
        assert_eq!(tracker.depth(), 2);
    }

    #[test]
    fn drop_newest_saturation_drops_further_entries() {
        let tracker = QueueTracker::new(QueuePolicy::DropNewest { capacity: 2 });
        let first = tracker.reserve_enqueue(Some(Arc::from("first")));
        assert!(matches!(first, QueueReservation::Accepted { .. }));
        assert_eq!(tracker.depth(), 1);

        let second = tracker.reserve_enqueue(Some(Arc::from("second")));
        assert!(matches!(second, QueueReservation::Accepted { .. }));
        assert_eq!(tracker.depth(), 2);

        let third = tracker.reserve_enqueue(Some(Arc::from("third")));
        assert_eq!(third, QueueReservation::DroppedNewest);
        assert_eq!(tracker.depth(), 2);
    }

    #[test]
    fn interleaved_internal_and_external_under_reject_new() {
        let tracker = QueueTracker::new(QueuePolicy::RejectNew { capacity: 1 });
        let mut queue = MessageQueue::<&'static str>::default();

        let res1 = tracker.reserve_enqueue(Some(Arc::from("e1")));
        let QueueReservation::Accepted { id: id1, .. } = res1 else {
            panic!("e1 must be accepted");
        };
        queue.pending.push_back(QueuedMessage {
            origin: QueueOrigin::External(id1),
            message: "e1",
        });

        // External is saturated, new external is rejected
        let res2 = tracker.reserve_enqueue(Some(Arc::from("e2")));
        assert_eq!(res2, QueueReservation::Rejected);

        // Internal message is enqueued directly (guaranteed delivery)
        queue.pending.push_back(QueuedMessage {
            origin: QueueOrigin::Internal,
            message: "i1",
        });

        let mut processed = Vec::new();
        while let Some(queued) = queue.pending.pop_front() {
            if let QueueOrigin::External(id) = queued.origin {
                tracker.complete_processed(id);
            }
            processed.push(queued.message);
        }

        assert_eq!(processed, vec!["e1", "i1"]);
        assert_eq!(tracker.depth(), 0);
    }

    #[test]
    fn interleaved_internal_and_external_under_drop_newest() {
        let tracker = QueueTracker::new(QueuePolicy::DropNewest { capacity: 1 });
        let mut queue = MessageQueue::<&'static str>::default();

        let res1 = tracker.reserve_enqueue(Some(Arc::from("e1")));
        let QueueReservation::Accepted { id: id1, .. } = res1 else {
            panic!("e1 must be accepted");
        };
        queue.pending.push_back(QueuedMessage {
            origin: QueueOrigin::External(id1),
            message: "e1",
        });

        // External is saturated, new external is dropped newest
        let res2 = tracker.reserve_enqueue(Some(Arc::from("e2")));
        assert_eq!(res2, QueueReservation::DroppedNewest);

        // Internal message is enqueued directly (guaranteed delivery)
        queue.pending.push_back(QueuedMessage {
            origin: QueueOrigin::Internal,
            message: "i1",
        });

        let mut processed = Vec::new();
        while let Some(queued) = queue.pending.pop_front() {
            if let QueueOrigin::External(id) = queued.origin {
                tracker.complete_processed(id);
            }
            processed.push(queued.message);
        }

        assert_eq!(processed, vec!["e1", "i1"]);
        assert_eq!(tracker.depth(), 0);
    }
}
