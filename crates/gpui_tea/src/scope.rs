use crate::{Command, Dispatcher, Subscriptions, View, model::Model};
use gpui::{App, Window};
use std::{fmt, marker::PhantomData, sync::Arc};

#[derive(Clone, PartialEq, Eq, Hash)]
/// Identify a model's position inside a nested program tree.
///
/// Child paths scope keyed commands and subscriptions so sibling models can reuse local keys
/// safely.
pub struct ChildPath {
    segments: Arc<[Arc<str>]>,
}

impl ChildPath {
    /// Return the root path.
    #[must_use]
    pub fn root() -> Self {
        Self {
            segments: Arc::from([]),
        }
    }

    /// Return a path with a single child segment.
    #[must_use]
    pub fn new(segment: impl Into<Arc<str>>) -> Self {
        Self::root().child(segment)
    }

    /// Return `true` when this path points at the root model.
    #[must_use]
    pub fn is_root(&self) -> bool {
        self.segments.is_empty()
    }

    /// Append `segment` to this path and return the resulting child path.
    #[must_use]
    pub fn child(&self, segment: impl Into<Arc<str>>) -> Self {
        let mut segments = Vec::with_capacity(self.segments.len() + 1);
        segments.extend(self.segments.iter().cloned());
        segments.push(segment.into());
        Self {
            segments: Arc::from(segments),
        }
    }

    /// Concatenate this path with `suffix`.
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

    /// Return the path segments in order from root to leaf.
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

#[derive(Debug)]
/// Carry structural context for a [`Model`] invocation.
///
/// A model context currently records only the model path, but it is threaded through the public
/// API so child-aware helpers can derive scoped dispatchers, commands, and subscriptions.
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
    /// Return the root context used by a top-level [`crate::Program`].
    #[must_use]
    pub fn root() -> Self {
        Self {
            path: ChildPath::root(),
            _message: PhantomData,
        }
    }

    /// Return the current model path.
    #[must_use]
    pub fn path(&self) -> &ChildPath {
        &self.path
    }

    /// Create a scope for a child model under `path`.
    ///
    /// The caller is responsible for choosing a path segment that stays stable for the lifetime of
    /// the child model and remains unique among siblings. Scoped keyed commands and subscriptions
    /// derive their identity from this path.
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

/// Bridge a child [`Model`] into a parent model's message space.
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
    /// Return the fully qualified path of the child model.
    #[must_use]
    pub fn path(&self) -> &ChildPath {
        self.context.path()
    }

    /// Return the child model context used for nested lifecycle calls.
    #[must_use]
    pub fn context(&self) -> &ModelContext<ChildMsg> {
        &self.context
    }

    /// Derive a child dispatcher from a parent dispatcher.
    #[must_use]
    pub fn dispatcher(&self, parent: &Dispatcher<ParentMsg>) -> Dispatcher<ChildMsg> {
        parent.map(self.lift.clone())
    }

    /// Run the child's [`Model::init()`] in this scope.
    pub fn init<M>(&self, child: &mut M, cx: &mut App) -> Command<ParentMsg>
    where
        M: Model<Msg = ChildMsg>,
    {
        child
            .init(cx, &self.context)
            .scoped(self.context.path())
            .map(self.lift.clone())
    }

    /// Run the child's [`Model::update()`] in this scope.
    pub fn update<M>(&self, child: &mut M, msg: ChildMsg, cx: &mut App) -> Command<ParentMsg>
    where
        M: Model<Msg = ChildMsg>,
    {
        child
            .update(msg, cx, &self.context)
            .scoped(self.context.path())
            .map(self.lift.clone())
    }

    /// Render the child's [`Model::view()`] with a scoped dispatcher.
    pub fn view<M>(
        &self,
        child: &M,
        window: &mut Window,
        cx: &mut App,
        parent_dispatcher: &Dispatcher<ParentMsg>,
    ) -> View
    where
        M: Model<Msg = ChildMsg>,
    {
        let dispatcher = self.dispatcher(parent_dispatcher);
        child.view(window, cx, &self.context, &dispatcher)
    }

    /// Build the child's [`Subscriptions`] and scope their keys under this child path.
    pub fn subscriptions<M>(&self, child: &M, cx: &mut App) -> Subscriptions<ParentMsg>
    where
        M: Model<Msg = ChildMsg>,
    {
        child
            .subscriptions(cx, &self.context)
            .scoped(self.context.path())
            .map(self.lift.clone())
    }
}
