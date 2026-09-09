#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Define how the runtime handles queue backpressure.
pub enum QueuePolicy {
    /// Accept every message without applying a capacity limit.
    Unbounded,
    /// Reject the newly dispatched message once `capacity` pending messages are tracked.
    RejectNew {
        /// The maximum number of pending messages allowed in the queue.
        capacity: usize,
    },
    /// Drop the newly dispatched message once `capacity` pending messages are tracked.
    DropNewest {
        /// The maximum number of pending messages allowed in the queue.
        capacity: usize,
    },
    /// Drop the oldest pending message to make room for a new one.
    DropOldest {
        /// The maximum number of pending messages allowed in the queue.
        capacity: usize,
    },
}

impl QueuePolicy {
    #[must_use]
    pub(crate) fn capacity(self) -> Option<usize> {
        match self {
            Self::Unbounded => None,
            Self::RejectNew { capacity }
            | Self::DropNewest { capacity }
            | Self::DropOldest { capacity } => Some(capacity),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Describe the action taken when a queue reaches capacity.
pub enum QueueOverflowAction {
    /// Reject the newly submitted message.
    RejectedNew,
    /// Drop the newly submitted message.
    DroppedNewest,
    /// Drop the oldest queued message.
    DroppedOldest,
}

impl QueueOverflowAction {
    /// Return a stable lowercase label suitable for metrics tags.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RejectedNew => "rejected_new",
            Self::DroppedNewest => "dropped_newest",
            Self::DroppedOldest => "dropped_oldest",
        }
    }
}
