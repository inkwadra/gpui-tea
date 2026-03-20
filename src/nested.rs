use crate::{Command, Dispatcher, Subscriptions, View};
use gpui::{App, Window};
use std::{fmt, marker::PhantomData, sync::Arc};

/// Identify a mounted child model within a program's nested model tree.
///
/// Paths are explicit and stable. Parents assign them deliberately so keyed
/// commands and subscriptions from sibling children never collide.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ChildPath {
    segments: Arc<[Arc<str>]>,
}

impl ChildPath {
    /// Create the root path.
    #[must_use]
    pub fn root() -> Self {
        Self {
            segments: Arc::from([]),
        }
    }

    /// Create a single-segment path.
    #[must_use]
    pub fn new(segment: impl Into<Arc<str>>) -> Self {
        Self::root().child(segment)
    }

    /// Return `true` when this path refers to the root model.
    #[must_use]
    pub fn is_root(&self) -> bool {
        self.segments.is_empty()
    }

    /// Append a child segment and return the new path.
    #[must_use]
    pub fn child(&self, segment: impl Into<Arc<str>>) -> Self {
        let mut segments = Vec::with_capacity(self.segments.len() + 1);
        segments.extend(self.segments.iter().cloned());
        segments.push(segment.into());
        Self {
            segments: Arc::from(segments),
        }
    }

    /// Concatenate another relative path onto this path.
    #[must_use]
    pub fn join(&self, suffix: &Self) -> Self {
        if self.is_root() {
            return suffix.clone();
        }
        if suffix.is_root() {
            return self.clone();
        }

        let mut segments = Vec::with_capacity(self.segments.len() + suffix.segments.len());
        segments.extend(self.segments.iter().cloned());
        segments.extend(suffix.segments.iter().cloned());
        Self {
            segments: Arc::from(segments),
        }
    }

    /// Return the path segments in order from parent to child.
    #[must_use]
    pub fn segments(&self) -> &[Arc<str>] {
        &self.segments
    }

    #[must_use]
    pub(crate) fn runtime_prefix(&self) -> Arc<str> {
        if self.is_root() {
            return Arc::from("root");
        }

        let mut encoded = String::new();
        for segment in self.segments() {
            encoded.push_str(&segment.len().to_string());
            encoded.push(':');
            encoded.push_str(segment);
            encoded.push('/');
        }

        Arc::from(encoded)
    }
}

impl Default for ChildPath {
    fn default() -> Self {
        Self::root()
    }
}

impl fmt::Debug for ChildPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ChildPath(\"{self}\")")
    }
}

impl fmt::Display for ChildPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_root() {
            return formatter.write_str("<root>");
        }

        let mut first = true;
        for segment in self.segments() {
            if !first {
                formatter.write_str("/")?;
            }
            formatter.write_str(segment)?;
            first = false;
        }
        Ok(())
    }
}

impl<T: Into<Arc<str>>> From<T> for ChildPath {
    fn from(segment: T) -> Self {
        Self::new(segment)
    }
}

/// Compose nested child models relative to the current model path.
#[derive(Debug)]
pub struct ModelContext<Msg> {
    path: ChildPath,
    _message: PhantomData<fn(Msg)>,
}

impl<Msg> Clone for ModelContext<Msg> {
    fn clone(&self) -> Self {
        Self {
            path: self.path.clone(),
            _message: PhantomData,
        }
    }
}

impl<Msg> ModelContext<Msg> {
    /// Create the root model context used by mounted programs.
    #[must_use]
    pub fn root() -> Self {
        Self {
            path: ChildPath::root(),
            _message: PhantomData,
        }
    }

    /// Return the absolute path of the current model within the nested tree.
    #[must_use]
    pub fn path(&self) -> &ChildPath {
        &self.path
    }

    /// Create a child scope relative to this model's path.
    ///
    /// The returned scope namespaces keyed commands and subscriptions under the
    /// derived child path and lifts child messages into the current model's
    /// message type.
    #[must_use]
    pub fn scope<ChildMsg, F>(
        &self,
        path: impl Into<ChildPath>,
        lift: F,
    ) -> ChildScope<ChildMsg, Msg, F>
    where
        ChildMsg: Send + 'static,
        Msg: Send + 'static,
        F: Fn(ChildMsg) -> Msg + Clone + Send + Sync + 'static,
    {
        let path = self.path.join(&path.into());
        ChildScope {
            context: ModelContext {
                path,
                _message: PhantomData,
            },
            lift,
            _messages: PhantomData,
        }
    }
}

/// A composable model contract that supports both root and nested use.
pub trait NestedModel: Sized + 'static {
    /// Name the message type consumed by this model.
    type Msg: Send + 'static;

    /// Initialize the model and return an initial command.
    fn init(&mut self, _cx: &mut App, _scope: &ModelContext<Self::Msg>) -> Command<Self::Msg> {
        Command::none()
    }

    /// Update the model in response to a single message.
    fn update(
        &mut self,
        msg: Self::Msg,
        cx: &mut App,
        scope: &ModelContext<Self::Msg>,
    ) -> Command<Self::Msg>;

    /// Render the current model state as GPUI elements.
    fn view(
        &self,
        window: &mut Window,
        cx: &mut App,
        scope: &ModelContext<Self::Msg>,
        dispatcher: &Dispatcher<Self::Msg>,
    ) -> View;

    /// Return the declarative subscriptions required by the current model state.
    fn subscriptions(
        &self,
        _cx: &mut App,
        _scope: &ModelContext<Self::Msg>,
    ) -> Subscriptions<Self::Msg> {
        Subscriptions::none()
    }
}

/// Compose a child model under a stable path and lift its messages into the
/// parent message type.
pub struct ChildScope<ChildMsg, ParentMsg, Lift> {
    context: ModelContext<ChildMsg>,
    lift: Lift,
    _messages: PhantomData<fn(ChildMsg) -> ParentMsg>,
}

impl<ChildMsg, ParentMsg, Lift> Clone for ChildScope<ChildMsg, ParentMsg, Lift>
where
    Lift: Clone,
{
    fn clone(&self) -> Self {
        Self {
            context: self.context.clone(),
            lift: self.lift.clone(),
            _messages: PhantomData,
        }
    }
}

impl<ChildMsg, ParentMsg, Lift> fmt::Debug for ChildScope<ChildMsg, ParentMsg, Lift> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ChildScope")
            .field("path", self.context.path())
            .finish_non_exhaustive()
    }
}

impl<ChildMsg, ParentMsg, Lift> ChildScope<ChildMsg, ParentMsg, Lift>
where
    ChildMsg: Send + 'static,
    ParentMsg: Send + 'static,
    Lift: Fn(ChildMsg) -> ParentMsg + Clone + Send + Sync + 'static,
{
    /// Return the absolute path assigned to this child instance.
    #[must_use]
    pub fn path(&self) -> &ChildPath {
        self.context.path()
    }

    /// Borrow the child-local model context.
    #[must_use]
    pub fn context(&self) -> &ModelContext<ChildMsg> {
        &self.context
    }

    /// Build a dispatcher that lifts child messages into the parent message
    /// type.
    #[must_use]
    pub fn dispatcher(&self, parent: &Dispatcher<ParentMsg>) -> Dispatcher<ChildMsg> {
        parent.map(self.lift.clone())
    }

    /// Initialize a child model inside this scope.
    pub fn init<M>(&self, child: &mut M, cx: &mut App) -> Command<ParentMsg>
    where
        M: NestedModel<Msg = ChildMsg>,
    {
        child
            .init(cx, &self.context)
            .scoped(self.context.path())
            .map(self.lift.clone())
    }

    /// Route a child message through the nested model and lift the produced
    /// command into the parent message type.
    pub fn update<M>(&self, child: &mut M, msg: ChildMsg, cx: &mut App) -> Command<ParentMsg>
    where
        M: NestedModel<Msg = ChildMsg>,
    {
        child
            .update(msg, cx, &self.context)
            .scoped(self.context.path())
            .map(self.lift.clone())
    }

    /// Render a child model through the parent dispatcher.
    pub fn view<M>(
        &self,
        child: &M,
        window: &mut Window,
        cx: &mut App,
        parent_dispatcher: &Dispatcher<ParentMsg>,
    ) -> View
    where
        M: NestedModel<Msg = ChildMsg>,
    {
        let dispatcher = self.dispatcher(parent_dispatcher);
        child.view(window, cx, &self.context, &dispatcher)
    }

    /// Collect child subscriptions rebased into the parent namespace.
    pub fn subscriptions<M>(&self, child: &M, cx: &mut App) -> Subscriptions<ParentMsg>
    where
        M: NestedModel<Msg = ChildMsg>,
    {
        child
            .subscriptions(cx, &self.context)
            .scoped(self.context.path())
            .map(self.lift.clone())
    }
}
