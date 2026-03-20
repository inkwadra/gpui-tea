use crate::{ChildPath, Dispatcher, Error, Key, Result};
use gpui::App;
use std::{collections::HashSet, fmt, sync::Arc};

/// Hold the runtime resource associated with an active subscription.
///
/// Returning a handle lets the runtime keep the underlying GPUI resource alive
/// for as long as the subscription's key remains active.
pub enum SubHandle {
    /// Keep a GPUI subscription handle alive while the subscription remains
    /// active.
    Subscription(gpui::Subscription),
    /// Keep a long-lived task alive for the lifetime of the subscription.
    ///
    /// Short-lived tasks are not automatically recreated while the same
    /// subscription key remains active.
    Task(gpui::Task<()>),
    /// Represent a subscription that does not need to retain any external
    /// handle.
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

/// Build the [`SubHandle`] for a newly activated [`Subscription`].
///
/// The runtime calls this builder only when a subscription key is newly added
/// during reconciliation. Reusing the same key on a later render keeps the
/// previous handle instead of invoking the builder again.
type SubscriptionBuilder<Msg> =
    Box<dyn for<'a> FnOnce(&'a mut SubscriptionContext<'a, Msg>) -> SubHandle>;

/// Provide runtime access while building a [`Subscription`].
#[must_use]
pub struct SubscriptionContext<'a, Msg> {
    app: &'a mut App,
    dispatcher: Dispatcher<Msg>,
}

impl<'a, Msg: Send + 'static> SubscriptionContext<'a, Msg> {
    pub(crate) fn new(app: &'a mut App, dispatcher: Dispatcher<Msg>) -> Self {
        Self { app, dispatcher }
    }

    /// Access the GPUI app while building a subscription.
    pub fn app(&mut self) -> &mut App {
        self.app
    }

    /// Access the program dispatcher associated with this subscription.
    ///
    /// Cloning the returned dispatcher is cheap and targets the same mounted
    /// program.
    #[must_use]
    pub fn dispatcher(&self) -> &Dispatcher<Msg> {
        &self.dispatcher
    }

    /// Dispatch a message through the associated program dispatcher.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ProgramUnavailable`] when the associated program has
    /// already been released and can no longer accept messages.
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

/// Describe a subscription to external events declaratively.
///
/// The key is the full identity of the subscription. If any captured state or
/// behavior changes, the subscription must use a different key so the runtime
/// can rebuild it.
#[must_use]
pub struct Subscription<Msg: 'static> {
    pub(crate) key: Key,
    pub(crate) label: Option<Arc<str>>,
    pub(crate) builder: SubscriptionBuilder<Msg>,
}

impl<Msg: Send + 'static> Subscription<Msg> {
    /// Create a new subscription with a stable identity.
    ///
    /// The builder runs only when the key is newly introduced to the active
    /// subscription set.
    ///
    /// # Examples
    ///
    /// ```
    /// use gpui_tea::{SubHandle, Subscription};
    ///
    /// let subscription = Subscription::<u8>::new("clock", |_cx| SubHandle::None);
    /// let _ = subscription;
    /// ```
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

    /// Attach a descriptive label used for runtime observability.
    ///
    /// The runtime includes this label in subscription lifecycle events such as
    /// [`crate::RuntimeEvent::SubscriptionBuilt`],
    /// [`crate::RuntimeEvent::SubscriptionRetained`], and
    /// [`crate::RuntimeEvent::SubscriptionRemoved`].
    ///
    /// Re-declaring the same key with a different label updates the observed
    /// label without forcing the runtime to rebuild the underlying handle.
    pub fn label(mut self, label: impl Into<Arc<str>>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Transform the messages dispatched by this subscription.
    ///
    /// This preserves the subscription key and label, so mapping alone does not
    /// force the runtime to rebuild the subscription.
    ///
    /// # Examples
    ///
    /// ```
    /// use gpui_tea::{SubHandle, Subscription};
    ///
    /// let subscription = Subscription::new("mapped", |_cx| SubHandle::None)
    ///     .label("mapped")
    ///     .map(|value: u8| usize::from(value));
    /// let _ = subscription;
    /// ```
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

    /// Rebase this subscription into a nested child path.
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

/// Hold a validated collection of subscriptions with unique keys.
#[must_use]
pub struct Subscriptions<Msg: 'static> {
    subscriptions: Vec<Subscription<Msg>>,
    keys: HashSet<Key>,
}

impl<Msg: Send + 'static> Subscriptions<Msg> {
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

    #[allow(clippy::missing_errors_doc)]
    fn try_insert_key(&mut self, key: &Key) -> Result<()> {
        if self.keys.insert(key.clone()) {
            return Ok(());
        }

        Err(Error::DuplicateSubscriptionKey { key: key.clone() })
    }

    /// Create an empty subscription set.
    ///
    /// # Examples
    ///
    /// ```
    /// use gpui_tea::Subscriptions;
    ///
    /// let subscriptions = Subscriptions::<()>::none();
    /// assert!(subscriptions.is_empty());
    /// ```
    pub fn none() -> Self {
        Self {
            subscriptions: Vec::new(),
            keys: HashSet::new(),
        }
    }

    /// Create a subscription set containing a single subscription.
    ///
    /// # Examples
    ///
    /// ```
    /// use gpui_tea::{SubHandle, Subscription, Subscriptions};
    ///
    /// let subscriptions =
    ///     Subscriptions::one(Subscription::<u8>::new("timer", |_cx| SubHandle::None));
    ///
    /// assert_eq!(subscriptions.len(), 1);
    /// ```
    pub fn one(subscription: Subscription<Msg>) -> Self {
        Self::from_unique_iter([subscription])
    }

    /// Build a subscription set from an iterator of subscriptions.
    ///
    /// # Errors
    ///
    /// Returns [`Error::DuplicateSubscriptionKey`] if two subscriptions in the
    /// iterator share the same key.
    pub fn batch(subscriptions: impl IntoIterator<Item = Subscription<Msg>>) -> Result<Self> {
        let mut result = Self::none();
        for subscription in subscriptions {
            result.push(subscription)?;
        }
        Ok(result)
    }

    /// Append a subscription to the set.
    ///
    /// # Errors
    ///
    /// Returns [`Error::DuplicateSubscriptionKey`] if `subscription.key` is
    /// already present in the set.
    pub fn push(&mut self, subscription: Subscription<Msg>) -> Result<()> {
        self.try_insert_key(&subscription.key)?;
        self.subscriptions.push(subscription);
        Ok(())
    }

    /// Transform the messages dispatched by every subscription in the set.
    ///
    /// This preserves every subscription key and label, so mapping alone does
    /// not force a rebuild during reconciliation.
    ///
    /// # Examples
    ///
    /// ```
    /// use gpui_tea::{SubHandle, Subscription, Subscriptions};
    ///
    /// let subscriptions = Subscriptions::one(Subscription::new("timer", |_cx| {
    ///     SubHandle::None
    /// }))
    /// .map(|value: u8| usize::from(value));
    ///
    /// assert_eq!(subscriptions.len(), 1);
    /// ```
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

    /// Rebase all subscription keys into a nested child path.
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
    #[must_use]
    pub fn len(&self) -> usize {
        self.subscriptions.len()
    }

    /// Return `true` when the set contains no subscriptions.
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
    use crate::ProgramConfig;
    use futures::{FutureExt, StreamExt, channel::mpsc::unbounded};
    use gpui::TestAppContext;

    #[allow(clippy::missing_panics_doc)]
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

    #[allow(clippy::missing_panics_doc)]
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

    #[allow(clippy::missing_panics_doc)]
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

    #[allow(clippy::missing_panics_doc)]
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
        let dispatcher = Dispatcher::new(sender, ProgramConfig::default());
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
        let message = receiver.next().now_or_never().flatten();
        assert_eq!(message, Some(OuterMsg::Wrapped("payload")));
    }
}
