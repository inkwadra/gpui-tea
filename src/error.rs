use crate::{Key, QueuePolicy};

/// Report recoverable failures exposed by `gpui_tea`.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// The target program has already been released and can no longer accept
    /// messages.
    #[error("program is no longer available")]
    ProgramUnavailable,

    /// The configured queue policy rejected a newly dispatched message.
    #[error("message queue rejected a new message because it is full under policy {policy:?}")]
    QueueFull {
        /// Hold the queue policy active when the enqueue failed.
        policy: QueuePolicy,
    },

    /// Two subscriptions declared the same key while building a validated
    /// subscription set.
    #[error("duplicate subscription key declared while building subscriptions: {key:?}")]
    DuplicateSubscriptionKey {
        /// The duplicated subscription key.
        key: Key,
    },
}

/// A convenient result alias for `gpui_tea` APIs.
pub type Result<T> = std::result::Result<T, Error>;
