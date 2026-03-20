use crate::{Key, QueuePolicy};

/// Report runtime and subscription construction failures.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// Signal that the program entity no longer exists.
    #[error("program is no longer available")]
    ProgramUnavailable,

    /// Signal that the queue policy refused to accept another message.
    #[error("message queue rejected a new message because it is full under policy {policy:?}")]
    QueueFull {
        /// The policy that was in effect when the queue filled up.
        policy: QueuePolicy,
    },

    /// Signal that two subscriptions in the same set use the same key.
    #[error("duplicate subscription key declared while building subscriptions: {key:?}")]
    DuplicateSubscriptionKey {
        /// The conflicting key that was encountered.
        key: Key,
    },
}

/// Alias the crate-wide result type.
pub type Result<T> = std::result::Result<T, Error>;
