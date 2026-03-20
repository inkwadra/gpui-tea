use crate::{ChildPath, Dispatcher, Error, Key, Result};
use gpui::App;
use std::{collections::HashSet, fmt, sync::Arc};

/// Hold the runtime resource that keeps a subscription alive.
pub enum SubHandle {
    /// Hold a GPUI subscription token.
    Subscription(gpui::Subscription),
    /// Hold a GPUI task token.
    Task(gpui::Task<()>),
    /// Represent a subscription that does not require a runtime handle.
    None,
}

impl fmt::Debug for SubHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Subscription(_) => formatter
                .debug_tuple("SubHandle::Subscription")
                .field(&"..")
                .finish(),
            Self::Task(_) => formatter
                .debug_tuple("SubHandle::Task")
                .field(&"..")
                .finish(),
            Self::None => formatter.write_str("SubHandle::None"),
        }
    }
}

/// A closure type for building a subscription context.
type SubscriptionBuilder<Msg> =
    Box<dyn for<'a> FnOnce(&'a mut SubscriptionContext<'a, Msg>) -> SubHandle>;

#[must_use]
/// Expose runtime services while constructing a [`Subscription`].
pub struct SubscriptionContext<'a, Msg> {
    app: &'a mut App,
    dispatcher: Dispatcher<Msg>,
}

impl<'a, Msg: Send + 'static> SubscriptionContext<'a, Msg> {
    /// Create a new subscription context.
    pub(crate) fn new(app: &'a mut App, dispatcher: Dispatcher<Msg>) -> Self {
        Self { app, dispatcher }
    }

    /// Return the mutable GPUI application handle used to create listeners.
    ///
    /// # Returns
    ///
    /// A mutable reference to the GPUI [`App`].
    pub fn app(&mut self) -> &mut App {
        self.app
    }

    /// Return the dispatcher for the mounted program.
    ///
    /// # Returns
    ///
    /// A reference to the [`Dispatcher`].
    #[must_use]
    pub fn dispatcher(&self) -> &Dispatcher<Msg> {
        &self.dispatcher
    }

    /// Dispatch `msg` through the subscription's dispatcher.
    ///
    /// # Arguments
    ///
    /// * `msg` - The message to dispatch.
    ///
    /// # Errors
    ///
    /// Returns [`Error::QueueFull`] when the queue policy rejects a new message.
    /// Returns [`Error::ProgramUnavailable`] when the program is no longer mounted.
    ///
    /// # Returns
    ///
    /// A [`Result`] indicating success or failure. Under [`crate::QueuePolicy::DropNewest`] this
    /// may still return `Ok(())` even though the new message was discarded, and under
    /// [`crate::QueuePolicy::DropOldest`] it may succeed after displacing the oldest queued
    /// message.
    pub fn dispatch(&self, msg: Msg) -> Result<()> {
        self.dispatcher.dispatch(msg)
    }
}

impl<Msg> fmt::Debug for SubscriptionContext<'_, Msg> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubscriptionContext")
            .finish_non_exhaustive()
    }
}

#[must_use]
/// Declare a long-lived external event source for a program.
///
/// The runtime reconciles subscriptions by [`Key`], retaining existing resources whose keys are
/// still present and building new resources for newly introduced keys. Keys must be unique within
/// a [`Subscriptions`] set.
pub struct Subscription<Msg: 'static> {
    pub(crate) key: Key,
    pub(crate) label: Option<Arc<str>>,
    pub(crate) builder: SubscriptionBuilder<Msg>,
}

impl<Msg: Send + 'static> Subscription<Msg> {
    /// Build a subscription with the given `key`.
    ///
    /// The `builder` runs during reconciliation and should register whatever GPUI subscription or
    /// task keeps the external event source alive.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to uniquely identify the subscription.
    /// * `builder` - A closure that runs when the subscription is activated.
    ///
    /// # Returns
    ///
    /// A new [`Subscription`].
    pub fn new<F>(key: impl Into<Key>, builder: F) -> Self
    where
        F: for<'a> FnOnce(&'a mut SubscriptionContext<'a, Msg>) -> SubHandle + 'static,
    {
        Self {
            key: key.into(),
            label: None,
            builder: Box::new(builder),
        }
    }

    /// Attach a human-readable label for observability output.
    ///
    /// # Arguments
    ///
    /// * `label` - The label to attach.
    ///
    /// # Returns
    ///
    /// The updated [`Subscription`].
    pub fn label(mut self, label: impl Into<Arc<str>>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Transform the message type emitted by this subscription.
    ///
    /// # Arguments
    ///
    /// * `f` - A mapping function to convert messages.
    ///
    /// # Returns
    ///
    /// A new [`Subscription`] emitting the mapped message type.
    pub fn map<F, NewMsg>(self, f: F) -> Subscription<NewMsg>
    where
        F: Fn(Msg) -> NewMsg + Clone + Send + Sync + 'static,
        NewMsg: Send + 'static,
    {
        Subscription {
            key: self.key,
            label: self.label,
            builder: Box::new(move |cx| {
                let dispatcher = cx.dispatcher().map(f);
                let mut mapped = SubscriptionContext::new(cx.app(), dispatcher);
                (self.builder)(&mut mapped)
            }),
        }
    }

    /// Scope this subscription's key under `path`.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to scope the key under.
    ///
    /// # Returns
    ///
    /// The scoped [`Subscription`].
    pub fn scoped(mut self, path: &ChildPath) -> Self {
        if !path.is_root() {
            self.key = self.key.scoped(path);
        }
        self
    }
}

impl<Msg> fmt::Debug for Subscription<Msg> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Subscription")
            .field("key", &self.key)
            .field("label", &self.label.as_deref())
            .finish_non_exhaustive()
    }
}

#[must_use]
/// Collect a set of unique [`Subscription`] values.
///
/// Insertion order is preserved so reconciliation happens deterministically.
pub struct Subscriptions<Msg: 'static> {
    subscriptions: Vec<Subscription<Msg>>,
    keys: HashSet<Key>,
}

impl<Msg: Send + 'static> Subscriptions<Msg> {
    /// Create a subscription set from an iterable of unique subscriptions.
    fn from_unique_iter(subscriptions: impl IntoIterator<Item = Subscription<Msg>>) -> Self {
        let subscriptions: Vec<_> = subscriptions.into_iter().collect();
        let keys = subscriptions
            .iter()
            .map(|subscription| subscription.key.clone())
            .collect();
        Self {
            subscriptions,
            keys,
        }
    }

    /// Attempt to insert a key into the set of unique keys.
    fn try_insert_key(&mut self, key: &Key) -> Result<()> {
        if self.keys.insert(key.clone()) {
            return Ok(());
        }

        Err(Error::DuplicateSubscriptionKey { key: key.clone() })
    }

    /// Return an empty subscription set.
    ///
    /// # Returns
    ///
    /// An empty [`Subscriptions`] set.
    pub fn none() -> Self {
        Self {
            subscriptions: Vec::new(),
            keys: HashSet::new(),
        }
    }

    /// Return a set containing a single subscription.
    ///
    /// # Arguments
    ///
    /// * `subscription` - The sole subscription for the set.
    ///
    /// # Returns
    ///
    /// A new [`Subscriptions`] set.
    pub fn one(subscription: Subscription<Msg>) -> Self {
        Self::from_unique_iter([subscription])
    }

    /// Build a subscription set from `subscriptions`.
    ///
    /// # Arguments
    ///
    /// * `subscriptions` - An iterator of subscriptions to include.
    ///
    /// # Errors
    ///
    /// Returns [`Error::DuplicateSubscriptionKey`] when two subscriptions use the same key.
    ///
    /// # Returns
    ///
    /// A new [`Subscriptions`] set.
    pub fn batch(subscriptions: impl IntoIterator<Item = Subscription<Msg>>) -> Result<Self> {
        let mut result = Self::none();
        for subscription in subscriptions {
            result.push(subscription)?;
        }
        Ok(result)
    }

    /// Append `subscription` to the set.
    ///
    /// # Arguments
    ///
    /// * `subscription` - The subscription to add.
    ///
    /// # Errors
    ///
    /// Returns [`Error::DuplicateSubscriptionKey`] when the key already exists in the set.
    ///
    /// # Returns
    ///
    /// A [`Result`] indicating success or failure.
    pub fn push(&mut self, subscription: Subscription<Msg>) -> Result<()> {
        self.try_insert_key(&subscription.key)?;
        self.subscriptions.push(subscription);
        Ok(())
    }

    /// Transform the message type emitted by every subscription in the set.
    ///
    /// # Arguments
    ///
    /// * `f` - A mapping function to apply to each subscription.
    ///
    /// # Returns
    ///
    /// A new [`Subscriptions`] set emitting the mapped message type.
    pub fn map<F, NewMsg>(self, f: F) -> Subscriptions<NewMsg>
    where
        F: Fn(Msg) -> NewMsg + Clone + Send + Sync + 'static,
        NewMsg: Send + 'static,
    {
        Subscriptions::from_unique_iter(
            self.subscriptions
                .into_iter()
                .map(|subscription| subscription.map(f.clone())),
        )
    }

    /// Scope every subscription key under `path`.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to scope keys under.
    ///
    /// # Returns
    ///
    /// The scoped [`Subscriptions`] set.
    pub fn scoped(self, path: &ChildPath) -> Self {
        if path.is_root() {
            return self;
        }

        Subscriptions::from_unique_iter(
            self.subscriptions
                .into_iter()
                .map(|subscription| subscription.scoped(path)),
        )
    }

    /// Return the number of subscriptions in the set.
    ///
    /// # Returns
    ///
    /// The count of subscriptions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.subscriptions.len()
    }

    /// Return `true` when the set contains no subscriptions.
    ///
    /// # Returns
    ///
    /// A boolean indicating if the set is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.subscriptions.is_empty()
    }
}

impl<Msg: Send + 'static> Default for Subscriptions<Msg> {
    fn default() -> Self {
        Self::none()
    }
}

impl<Msg: Send + 'static> From<Subscription<Msg>> for Subscriptions<Msg> {
    fn from(subscription: Subscription<Msg>) -> Self {
        Self::one(subscription)
    }
}

impl<Msg: Send + 'static> IntoIterator for Subscriptions<Msg> {
    type Item = Subscription<Msg>;
    type IntoIter = std::vec::IntoIter<Subscription<Msg>>;

    fn into_iter(self) -> Self::IntoIter {
        self.subscriptions.into_iter()
    }
}

impl<Msg> fmt::Debug for Subscriptions<Msg> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut keys = self
            .keys
            .iter()
            .map(|key| format!("{key:?}"))
            .collect::<Vec<_>>();
        keys.sort_unstable();

        formatter
            .debug_struct("Subscriptions")
            .field("len", &self.subscriptions.len())
            .field("keys", &keys)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProgramConfig, QueuePolicy, observability::Observability, queue::QueueTracker};
    use futures::{FutureExt, StreamExt, channel::mpsc::unbounded};
    use gpui::TestAppContext;
    use std::sync::Arc;

    #[test]
    fn batch_returns_duplicate_key_error() {
        let error = Subscriptions::<()>::batch([
            Subscription::<()>::new("dup", |_| SubHandle::None),
            Subscription::<()>::new("dup", |_| SubHandle::None),
        ])
        .unwrap_err();

        assert_eq!(
            error,
            Error::DuplicateSubscriptionKey {
                key: Key::new("dup"),
            }
        );
    }

    #[test]
    fn push_returns_duplicate_key_error() {
        let mut subscriptions =
            Subscriptions::one(Subscription::<()>::new("dup", |_| SubHandle::None));
        let error = subscriptions
            .push(Subscription::<()>::new("dup", |_| SubHandle::None))
            .unwrap_err();

        assert_eq!(
            error,
            Error::DuplicateSubscriptionKey {
                key: Key::new("dup"),
            }
        );
    }

    #[test]
    fn batch_and_push_preserve_uniqueness_and_order() {
        let mut subscriptions =
            Subscriptions::<()>::batch([Subscription::new("alpha", |_| SubHandle::None)])
                .expect("unique keys should build a subscription set");
        subscriptions
            .push(Subscription::new("beta", |_| SubHandle::None))
            .expect("a new key should append successfully");

        let keys = subscriptions
            .into_iter()
            .map(|subscription| subscription.key)
            .collect::<Vec<_>>();

        assert_eq!(keys, vec![Key::new("alpha"), Key::new("beta")]);
    }

    #[gpui::test]
    fn mapped_subscription_preserves_key_and_label_and_transforms_messages(
        cx: &mut TestAppContext,
    ) {
        #[derive(Clone, Debug, PartialEq, Eq)]
        enum OuterMsg {
            Wrapped(&'static str),
        }

        #[derive(Clone, Debug, PartialEq, Eq)]
        enum InnerMsg {
            Fire(&'static str),
        }

        let (sender, mut receiver) = unbounded();
        let queue_tracker = Arc::new(QueueTracker::new(QueuePolicy::Unbounded));
        let observability = Observability::new(
            ProgramConfig::default(),
            Arc::new({
                let queue_tracker = queue_tracker.clone();
                move || queue_tracker.depth()
            }),
        );
        let dispatcher = Dispatcher::new(sender, queue_tracker, observability);
        let mapped = Subscription::new("mapped", |cx| {
            cx.dispatch(InnerMsg::Fire("payload"))
                .expect("the mapped dispatcher should be alive while building the subscription");
            SubHandle::None
        })
        .label("mapped-subscription")
        .map(|msg| match msg {
            InnerMsg::Fire(value) => OuterMsg::Wrapped(value),
        });

        assert_eq!(mapped.key, Key::new("mapped"));
        assert_eq!(mapped.label.as_deref(), Some("mapped-subscription"));

        cx.update(|app| {
            let mut context = SubscriptionContext::new(app, dispatcher.clone());
            let handle = (mapped.builder)(&mut context);
            assert!(matches!(handle, SubHandle::None));
        });

        cx.run_until_parked();
        let message = receiver
            .next()
            .now_or_never()
            .flatten()
            .map(|queued| queued.message);
        assert_eq!(message, Some(OuterMsg::Wrapped("payload")));
    }
}
