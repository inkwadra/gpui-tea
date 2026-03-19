use crate::{DispatchError, Dispatcher, Key};
use gpui::App;
use std::{collections::HashSet, sync::Arc};

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

/// Build the [`SubHandle`] for a newly activated [`Subscription`].
///
/// The runtime calls this builder only when a subscription key is newly added
/// during reconciliation. Reusing the same key on a later render keeps the
/// previous handle instead of invoking the builder again.
pub type SubscriptionBuilder<Msg> =
    Box<dyn for<'a> FnOnce(&'a mut SubscriptionContext<'a, Msg>) -> SubHandle>;

/// Provide runtime access while building a [`Subscription`].
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
    /// Returns [`DispatchError`] when the associated program has already been
    /// released and can no longer accept messages.
    pub fn dispatch(&self, msg: Msg) -> Result<(), DispatchError> {
        self.dispatcher.dispatch(msg)
    }
}

/// Describe a subscription to external events declaratively.
///
/// The key is the full identity of the subscription. If any captured state or
/// behavior changes, the subscription must use a different key so the runtime
/// can rebuild it.
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
    #[must_use]
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
}

/// Hold a validated collection of subscriptions with unique keys.
pub struct Subscriptions<Msg: 'static> {
    subscriptions: Vec<Subscription<Msg>>,
    keys: HashSet<Key>,
}

impl<Msg: Send + 'static> Subscriptions<Msg> {
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
    #[must_use]
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
    #[must_use]
    pub fn one(subscription: Subscription<Msg>) -> Self {
        let mut subscriptions = Self::none();
        subscriptions.push(subscription);
        subscriptions
    }

    /// Build a subscription set from an iterator of subscriptions.
    ///
    /// Duplicate keys are treated as a programmer error and will panic.
    ///
    /// # Panics
    ///
    /// Panics if two subscriptions in the iterator share the same key.
    pub fn batch(subscriptions: impl IntoIterator<Item = Subscription<Msg>>) -> Self {
        let mut result = Self::none();
        for subscription in subscriptions {
            result.push(subscription);
        }
        result
    }

    /// Append a subscription to the set.
    ///
    /// Duplicate keys are treated as a programmer error and will panic.
    ///
    /// # Panics
    ///
    /// Panics if `subscription.key` is already present in the set.
    pub fn push(&mut self, subscription: Subscription<Msg>) {
        assert!(
            self.keys.insert(subscription.key.clone()),
            "duplicate subscription key declared while building subscriptions: {:?}",
            subscription.key
        );
        self.subscriptions.push(subscription);
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
        Subscriptions::batch(
            self.subscriptions
                .into_iter()
                .map(|subscription| subscription.map(f.clone())),
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

impl<Msg: Send + 'static> Extend<Subscription<Msg>> for Subscriptions<Msg> {
    fn extend<T: IntoIterator<Item = Subscription<Msg>>>(&mut self, iter: T) {
        for subscription in iter {
            self.push(subscription);
        }
    }
}

impl<Msg: Send + 'static> FromIterator<Subscription<Msg>> for Subscriptions<Msg> {
    fn from_iter<T: IntoIterator<Item = Subscription<Msg>>>(iter: T) -> Self {
        let mut subscriptions = Self::none();
        subscriptions.extend(iter);
        subscriptions
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProgramConfig;
    use futures::{FutureExt, StreamExt, channel::mpsc::unbounded};
    use gpui::TestAppContext;

    #[test]
    #[should_panic(
        expected = "duplicate subscription key declared while building subscriptions: Key(\"dup\")"
    )]
    fn batch_panics_on_duplicate_keys() {
        let _ = Subscriptions::<()>::batch([
            Subscription::<()>::new("dup", |_| SubHandle::None),
            Subscription::<()>::new("dup", |_| SubHandle::None),
        ]);
    }

    #[test]
    #[should_panic(
        expected = "duplicate subscription key declared while building subscriptions: Key(\"dup\")"
    )]
    fn push_panics_on_duplicate_keys() {
        let mut subscriptions =
            Subscriptions::one(Subscription::<()>::new("dup", |_| SubHandle::None));
        subscriptions.push(Subscription::<()>::new("dup", |_| SubHandle::None));
    }

    #[allow(clippy::missing_panics_doc)]
    #[test]
    fn from_iter_and_extend_preserve_uniqueness_and_order() {
        let mut subscriptions: Subscriptions<()> =
            [Subscription::new("alpha", |_| SubHandle::None)]
                .into_iter()
                .collect();
        subscriptions.extend([Subscription::new("beta", |_| SubHandle::None)]);

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
            cx.dispatch(InnerMsg::Fire("payload")).unwrap();
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
