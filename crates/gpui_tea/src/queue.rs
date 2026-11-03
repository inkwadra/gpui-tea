use crate::observability::QueuePolicy;
use std::{
    collections::{HashSet, VecDeque},
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    task::{Context, Poll, Waker},
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

/// A batch of external messages handed from the inbox to the program, in dispatch order.
pub(crate) type InboxBatch<Msg> = VecDeque<(QueueEntryId, Msg)>;

/// Result of offering an externally dispatched message to the queue.
pub(crate) enum EnqueueOutcome<Msg> {
    /// The message was accepted.
    Accepted {
        /// Total queue depth after the message was accepted.
        depth: usize,
        /// The oldest message displaced under [`QueuePolicy::DropOldest`], if any.
        displaced: Option<Displaced<Msg>>,
    },
    /// The policy rejected the message; it is handed back to the caller.
    Rejected(Msg),
    /// The policy dropped the message; it is handed back to the caller.
    DroppedNewest(Msg),
    /// The program is gone; the message is handed back to the caller.
    Closed(Msg),
}

/// A message displaced by [`QueuePolicy::DropOldest`].
pub(crate) struct Displaced<Msg> {
    pub(crate) description: Option<Arc<str>>,
    /// The displaced message when it was still waiting in the inbox. It is `None` when the
    /// program already took it, in which case the drain loop skips it.
    pub(crate) message: Option<Msg>,
}

/// Internal control state for a queue.
struct QueueControl<Msg> {
    policy: QueuePolicy,
    next_id: u64,
    /// External messages accepted but not processed yet, in dispatch order.
    active: VecDeque<(QueueEntryId, Option<Arc<str>>)>,
    /// External messages that have not been handed to the program yet.
    ///
    /// The program takes the whole inbox at once, so the inbox is always the newest suffix of
    /// `active`.
    inbox: InboxBatch<Msg>,
    /// Messages displaced after the program already took them out of the inbox.
    dropped: HashSet<QueueEntryId>,
    internal_pending: usize,
    closed: bool,
    waker: Option<Waker>,
}

impl<Msg> QueueControl<Msg> {
    fn new(policy: QueuePolicy) -> Self {
        Self {
            policy,
            next_id: 0,
            active: VecDeque::new(),
            inbox: VecDeque::new(),
            dropped: HashSet::new(),
            internal_pending: 0,
            closed: false,
            waker: None,
        }
    }

    fn next_id(&mut self) -> QueueEntryId {
        self.next_id = self.next_id.wrapping_add(1);
        QueueEntryId(self.next_id)
    }

    fn depth(&self) -> usize {
        self.active.len() + self.internal_pending
    }

    fn offer(&mut self, message: Msg, description: Option<Arc<str>>) -> EnqueueOutcome<Msg> {
        if self.closed {
            return EnqueueOutcome::Closed(message);
        }

        let displaced = match self.policy.capacity() {
            Some(capacity) if self.active.len() >= capacity => match self.policy {
                QueuePolicy::RejectNew { .. } => return EnqueueOutcome::Rejected(message),
                QueuePolicy::DropNewest { .. } => return EnqueueOutcome::DroppedNewest(message),
                // With nothing to displace (capacity 0 on an empty queue) the new message is
                // kept so DropOldest never silently drops everything.
                QueuePolicy::DropOldest { .. } => self.displace_oldest(),
                QueuePolicy::Unbounded => None,
            },
            _ => None,
        };

        let id = self.next_id();
        self.active.push_back((id, description));
        self.inbox.push_back((id, message));
        EnqueueOutcome::Accepted {
            depth: self.depth(),
            displaced,
        }
    }

    fn displace_oldest(&mut self) -> Option<Displaced<Msg>> {
        let (oldest, description) = self.active.pop_front()?;
        let message = if self.inbox.front().is_some_and(|(id, _)| *id == oldest) {
            self.inbox.pop_front().map(|(_, message)| message)
        } else {
            self.dropped.insert(oldest);
            None
        };

        Some(Displaced {
            description,
            message,
        })
    }

    fn begin_processing(&mut self, id: QueueEntryId) -> bool {
        if self.dropped.remove(&id) {
            return false;
        }

        if self.active.front().is_some_and(|(entry, _)| *entry == id) {
            self.active.pop_front();
        } else if let Some(position) = self.active.iter().position(|(entry, _)| *entry == id) {
            self.active.remove(position);
        }
        true
    }
}

/// Shared queue state between dispatchers and the mounted program.
///
/// External messages wait in an inbox guarded by the same lock as the queue policy, so a
/// displaced message is released as soon as it is displaced instead of lingering in a channel.
pub(crate) struct QueueTracker<Msg> {
    control: Mutex<QueueControl<Msg>>,
}

impl<Msg> QueueTracker<Msg> {
    pub(crate) fn new(policy: QueuePolicy) -> Self {
        Self {
            control: Mutex::new(QueueControl::new(policy)),
        }
    }

    fn lock(&self) -> MutexGuard<'_, QueueControl<Msg>> {
        // Critical sections never run user code, so a poisoned lock still holds valid state.
        self.control.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Offer an externally dispatched message and wake the program when it is accepted.
    pub(crate) fn offer(&self, message: Msg, description: Option<Arc<str>>) -> EnqueueOutcome<Msg> {
        let (outcome, waker) = {
            let mut control = self.lock();
            let outcome = control.offer(message, description);
            let waker = match outcome {
                EnqueueOutcome::Accepted { .. } => control.waker.take(),
                _ => None,
            };
            (outcome, waker)
        };

        if let Some(waker) = waker {
            waker.wake();
        }
        outcome
    }

    /// Take every message waiting in the inbox, or register `cx` to be woken by the next one.
    ///
    /// Resolves to `None` once the queue is closed and the inbox is empty.
    pub(crate) fn poll_take(&self, cx: &mut Context<'_>) -> Poll<Option<InboxBatch<Msg>>> {
        let mut control = self.lock();
        if !control.inbox.is_empty() {
            return Poll::Ready(Some(std::mem::take(&mut control.inbox)));
        }
        if control.closed {
            return Poll::Ready(None);
        }

        match &mut control.waker {
            Some(waker) if waker.will_wake(cx.waker()) => {}
            slot => *slot = Some(cx.waker().clone()),
        }
        Poll::Pending
    }

    /// Reject further dispatches and release every message still waiting in the inbox.
    pub(crate) fn close(&self) {
        let (inbox, waker) = {
            let mut control = self.lock();
            control.closed = true;
            control.active.clear();
            control.dropped.clear();
            control.internal_pending = 0;
            (std::mem::take(&mut control.inbox), control.waker.take())
        };
        drop(inbox);
        drop(waker);
    }

    /// Mark an external message as being processed.
    ///
    /// Returns `false` when the message was displaced after the program took it, in which case
    /// it must be skipped.
    pub(crate) fn begin_processing(&self, id: QueueEntryId) -> bool {
        self.lock().begin_processing(id)
    }

    /// Record an internal message entering the pending queue and return the new total depth.
    pub(crate) fn internal_enqueued(&self) -> usize {
        let mut control = self.lock();
        control.internal_pending += 1;
        control.depth()
    }

    /// Record an internal message leaving the pending queue.
    pub(crate) fn internal_dequeued(&self) {
        let mut control = self.lock();
        control.internal_pending = control.internal_pending.saturating_sub(1);
    }

    /// Return the number of accepted messages that have not been processed yet, external and
    /// internal combined.
    pub(crate) fn depth(&self) -> usize {
        self.lock().depth()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::task::{ArcWake, waker};

    struct WakeFlag(AtomicBool);

    impl ArcWake for WakeFlag {
        fn wake_by_ref(arc_self: &Arc<Self>) {
            arc_self.0.store(true, Ordering::SeqCst);
        }
    }

    fn accepted<Msg>(outcome: EnqueueOutcome<Msg>) -> (usize, Option<Displaced<Msg>>) {
        match outcome {
            EnqueueOutcome::Accepted { depth, displaced } => (depth, displaced),
            _ => panic!("message must be accepted"),
        }
    }

    fn take_ready<Msg>(tracker: &QueueTracker<Msg>) -> Option<InboxBatch<Msg>> {
        let flag = Arc::new(WakeFlag(AtomicBool::new(false)));
        let waker = waker(flag);
        match tracker.poll_take(&mut Context::from_waker(&waker)) {
            Poll::Ready(batch) => batch,
            Poll::Pending => panic!("inbox must be ready"),
        }
    }

    #[test]
    fn drop_oldest_capacity_zero_accepts_when_nothing_to_displace() {
        let tracker = QueueTracker::new(QueuePolicy::DropOldest { capacity: 0 });
        let (depth, displaced) = accepted(tracker.offer(1, None));
        assert!(displaced.is_none());
        assert_eq!(depth, 1);
    }

    #[test]
    fn drop_oldest_releases_displaced_message_still_in_inbox() {
        let tracker = QueueTracker::new(QueuePolicy::DropOldest { capacity: 1 });
        accepted(tracker.offer("first", Some(Arc::from("first"))));
        let (depth, displaced) = accepted(tracker.offer("second", Some(Arc::from("second"))));

        let displaced = displaced.expect("first message must be displaced");
        assert_eq!(displaced.description.as_deref(), Some("first"));
        assert_eq!(displaced.message, Some("first"));
        assert_eq!(depth, 1);

        let batch = take_ready(&tracker).expect("queue is open");
        assert_eq!(
            batch.into_iter().map(|(_, msg)| msg).collect::<Vec<_>>(),
            vec!["second"]
        );
    }

    #[test]
    fn drop_oldest_marks_taken_message_as_dropped() {
        let tracker = QueueTracker::new(QueuePolicy::DropOldest { capacity: 1 });
        accepted(tracker.offer("first", None));
        let batch = take_ready(&tracker).expect("queue is open");
        let (first_id, _) = batch[0];

        let (_, displaced) = accepted(tracker.offer("second", None));
        let displaced = displaced.expect("first message must be displaced");
        assert!(displaced.message.is_none());
        assert!(!tracker.begin_processing(first_id));
        assert_eq!(tracker.depth(), 1);
    }

    #[test]
    fn reject_new_and_drop_newest_hand_the_message_back() {
        let reject = QueueTracker::new(QueuePolicy::RejectNew { capacity: 1 });
        accepted(reject.offer(1, None));
        assert!(matches!(reject.offer(2, None), EnqueueOutcome::Rejected(2)));
        assert_eq!(reject.depth(), 1);

        let drop_newest = QueueTracker::new(QueuePolicy::DropNewest { capacity: 1 });
        accepted(drop_newest.offer(1, None));
        assert!(matches!(
            drop_newest.offer(2, None),
            EnqueueOutcome::DroppedNewest(2)
        ));
        assert_eq!(drop_newest.depth(), 1);
    }

    #[test]
    fn poll_take_hands_over_the_whole_inbox_in_order_and_wakes_on_offer() {
        let tracker = QueueTracker::new(QueuePolicy::Unbounded);
        let flag = Arc::new(WakeFlag(AtomicBool::new(false)));
        let waker = waker(flag.clone());
        let mut cx = Context::from_waker(&waker);

        assert!(tracker.poll_take(&mut cx).is_pending());
        accepted(tracker.offer(1, None));
        accepted(tracker.offer(2, None));
        assert!(flag.0.load(Ordering::SeqCst));

        let Poll::Ready(Some(batch)) = tracker.poll_take(&mut cx) else {
            panic!("inbox must be ready");
        };
        assert_eq!(
            batch.into_iter().map(|(_, msg)| msg).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(tracker.depth(), 2);
    }

    #[test]
    fn close_rejects_new_messages_and_finishes_the_inbox_stream() {
        let tracker = QueueTracker::new(QueuePolicy::Unbounded);
        accepted(tracker.offer(1, None));
        tracker.close();

        assert!(matches!(tracker.offer(2, None), EnqueueOutcome::Closed(2)));
        assert!(take_ready(&tracker).is_none());
        assert_eq!(tracker.depth(), 0);
    }

    #[test]
    fn depth_counts_external_and_internal_messages() {
        let tracker = QueueTracker::new(QueuePolicy::Unbounded);
        accepted(tracker.offer(1, None));
        assert_eq!(tracker.internal_enqueued(), 2);

        let batch = take_ready(&tracker).expect("queue is open");
        assert!(tracker.begin_processing(batch[0].0));
        assert_eq!(tracker.depth(), 1);
        tracker.internal_dequeued();
        assert_eq!(tracker.depth(), 0);
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
}
