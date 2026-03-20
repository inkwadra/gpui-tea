use crate::ChildPath;
use futures::{
    FutureExt,
    future::{BoxFuture, LocalBoxFuture},
};
use gpui::{AsyncApp, BackgroundExecutor};
use std::{fmt, future::Future, sync::Arc};

pub(crate) type ForegroundEffectFn<Msg> =
    Box<dyn for<'a> FnOnce(&'a mut AsyncApp) -> LocalBoxFuture<'a, Option<Msg>> + 'static>;

pub(crate) type BackgroundEffectFn<Msg> =
    Box<dyn FnOnce(BackgroundExecutor) -> BoxFuture<'static, Option<Msg>> + 'static>;

/// Classify how the runtime executes a [`Command`].
///
/// This enum appears in [`crate::RuntimeEvent`] values so observers can tell
/// whether a command completed synchronously, on the GPUI foreground executor,
/// or on the background executor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandKind {
    /// Deliver a queued message immediately after the current transition ends.
    Emit,
    /// Run work on GPUI's foreground executor with access to [`AsyncApp`].
    Foreground,
    /// Run work on GPUI's background executor.
    Background,
}

pub(crate) enum Effect<Msg> {
    Foreground(ForegroundEffectFn<Msg>),
    Background(BackgroundEffectFn<Msg>),
}

impl<Msg> Effect<Msg> {
    pub(crate) fn kind(&self) -> CommandKind {
        match self {
            Self::Foreground(_) => CommandKind::Foreground,
            Self::Background(_) => CommandKind::Background,
        }
    }
}

pub(crate) enum CommandInner<Msg> {
    None,
    Emit(Msg),
    Batch(Vec<Command<Msg>>),
    Effect(Effect<Msg>),
    Keyed { key: Key, effect: Effect<Msg> },
}

/// Identify keyed commands and subscriptions by stable value.
///
/// Keys compare by value instead of pointer identity. Reusing the same key for
/// a later keyed command makes that later command authoritative, and any stale
/// completion from earlier work with the same key is ignored. Reusing the same
/// key for a [`crate::Subscription`] keeps the existing subscription handle
/// alive instead of rebuilding it.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Key {
    id: Arc<str>,
    child_path: Option<ChildPath>,
    local_id: Option<Arc<str>>,
}

impl Key {
    /// Create a key from a stable identifier.
    ///
    /// Prefer identifiers whose meaning is stable across renders or state
    /// transitions so the runtime can reconcile keyed work correctly.
    ///
    /// # Examples
    ///
    /// ```
    /// use gpui_tea::Key;
    ///
    /// let first = Key::new("search");
    /// let second = Key::new(String::from("search"));
    ///
    /// assert_eq!(first, second);
    /// ```
    pub fn new(id: impl Into<Arc<str>>) -> Self {
        Self {
            id: id.into(),
            child_path: None,
            local_id: None,
        }
    }

    /// Return the runtime identity used to track this key.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Return the nested child path associated with this key, if any.
    #[must_use]
    pub fn child_path(&self) -> Option<&ChildPath> {
        self.child_path.as_ref()
    }

    /// Return the child-local key identifier before path rebasing, if any.
    #[must_use]
    pub fn local_id(&self) -> Option<&str> {
        self.local_id.as_deref()
    }

    pub(crate) fn scoped(&self, path: &ChildPath) -> Self {
        if path.is_root() || self.child_path.is_some() {
            return self.clone();
        }

        let local_id: Arc<str> = Arc::from(self.id());
        let mut runtime_id = String::from(path.runtime_prefix().as_ref());
        runtime_id.push('#');
        runtime_id.push_str(&local_id.len().to_string());
        runtime_id.push(':');
        runtime_id.push_str(local_id.as_ref());

        Self {
            id: Arc::from(runtime_id),
            child_path: Some(path.clone()),
            local_id: Some(local_id),
        }
    }
}

impl<T: Into<Arc<str>>> From<T> for Key {
    fn from(id: T) -> Self {
        Self::new(id)
    }
}

impl fmt::Debug for Key {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let (Some(path), Some(local_id)) = (&self.child_path, &self.local_id) {
            return write!(formatter, "Key(\"{path}:{local_id}\")");
        }

        write!(formatter, "Key(\"{}\")", self.id)
    }
}

/// Describe work for the TEA runtime to execute after a state transition.
///
/// Commands are declarative values returned from [`crate::Model::init`] or
/// [`crate::Model::update`]. The runtime executes them only after the current
/// transition finishes, which keeps model updates non-recursive even when a
/// command immediately emits another message.
///
/// Attach a label with [`Command::label`] when runtime observability should
/// report human-readable command names.
#[must_use]
pub struct Command<Msg> {
    pub(crate) inner: CommandInner<Msg>,
    pub(crate) label: Option<Arc<str>>,
}

impl<Msg: Send + 'static> Command<Msg> {
    fn new(inner: CommandInner<Msg>) -> Self {
        Self { inner, label: None }
    }

    fn effect(effect: Effect<Msg>) -> Self {
        Self::new(CommandInner::Effect(effect))
    }

    fn keyed_effect(key: impl Into<Key>, effect: Effect<Msg>) -> Self {
        Self::new(CommandInner::Keyed {
            key: key.into(),
            effect,
        })
    }

    /// Create a command that performs no work and emits no message.
    ///
    /// This is the default value returned by [`Default::default`].
    pub fn none() -> Self {
        Self::new(CommandInner::None)
    }

    /// Emit a message immediately after the current transition completes.
    ///
    /// This is equivalent to a synchronous follow-up dispatch. The message is
    /// still appended to the runtime queue, so it never re-enters
    /// [`crate::Model::update`] recursively.
    ///
    /// # Examples
    ///
    /// ```
    /// use gpui_tea::Command;
    ///
    /// let command = Command::emit(7_u8);
    /// let _ = command;
    /// ```
    pub fn emit(message: Msg) -> Self {
        Self::new(CommandInner::Emit(message))
    }

    /// Run an async effect on GPUI's foreground executor.
    ///
    /// Use this when the effect needs access to [`AsyncApp`] and may emit a
    /// follow-up message. The future does not need to be [`Send`] because it
    /// stays on the foreground executor.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use gpui::AsyncApp;
    /// use gpui_tea::Command;
    ///
    /// # async fn load(_cx: &mut AsyncApp) -> Option<u32> { Some(1) }
    /// let command = Command::<u32>::foreground(load);
    /// let _ = command;
    /// ```
    pub fn foreground<AsyncFn>(effect: AsyncFn) -> Self
    where
        AsyncFn: AsyncFnOnce(&mut AsyncApp) -> Option<Msg> + 'static,
    {
        Self::effect(Effect::Foreground(Box::new(move |cx| Box::pin(effect(cx)))))
    }

    /// Run a keyed async effect on GPUI's foreground executor.
    ///
    /// Reusing `key` for a later keyed command replaces the tracked task for
    /// that key. If an older task still completes, the runtime ignores its
    /// message as stale.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use gpui::AsyncApp;
    /// use gpui_tea::Command;
    ///
    /// # async fn load(_cx: &mut AsyncApp) -> Option<u32> { Some(1) }
    /// let command = Command::<u32>::foreground_keyed("load-user", load);
    /// let _ = command;
    /// ```
    pub fn foreground_keyed<AsyncFn>(key: impl Into<Key>, effect: AsyncFn) -> Self
    where
        AsyncFn: AsyncFnOnce(&mut AsyncApp) -> Option<Msg> + 'static,
    {
        Self::keyed_effect(
            key,
            Effect::Foreground(Box::new(move |cx| Box::pin(effect(cx)))),
        )
    }

    /// Run background work and dispatch its optional output as a message.
    ///
    /// Use this for work that does not need foreground-only GPUI access. The
    /// produced future must be [`Send`] because it runs on the background
    /// executor.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use gpui_tea::Command;
    ///
    /// let command = Command::<u32>::background(|_executor| async { Some(1) });
    /// let _ = command;
    /// ```
    pub fn background<F, Fut>(effect: F) -> Self
    where
        F: FnOnce(BackgroundExecutor) -> Fut + 'static,
        Fut: Future<Output = Option<Msg>> + Send + 'static,
    {
        Self::effect(Effect::Background(Box::new(move |executor| {
            let spawned_executor = executor.clone();
            effect(spawned_executor).boxed()
        })))
    }

    /// Run keyed background work and dispatch its optional output as a message.
    ///
    /// Later commands with the same `key` become authoritative for that key.
    /// Any completion from replaced work is observed as stale and does not
    /// enqueue a message.
    pub fn background_keyed<F, Fut>(key: impl Into<Key>, effect: F) -> Self
    where
        F: FnOnce(BackgroundExecutor) -> Fut + 'static,
        Fut: Future<Output = Option<Msg>> + Send + 'static,
    {
        Self::keyed_effect(
            key,
            Effect::Background(Box::new(move |executor| {
                let spawned_executor = executor.clone();
                effect(spawned_executor).boxed()
            })),
        )
    }

    /// Run a foreground effect for its side effect without dispatching a
    /// follow-up message.
    ///
    /// This is the foreground analogue of [`Command::foreground`] when the
    /// effect exists only for its side effects.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use gpui::AsyncApp;
    /// use gpui_tea::Command;
    ///
    /// # async fn effect(_cx: &mut AsyncApp) {}
    /// let command = Command::<()>::perform(effect);
    /// let _ = command;
    /// ```
    pub fn perform<AsyncFn>(effect: AsyncFn) -> Self
    where
        AsyncFn: AsyncFnOnce(&mut AsyncApp) -> () + 'static,
    {
        Self::effect(Effect::Foreground(Box::new(move |cx| {
            async move {
                effect(cx).await;
                None
            }
            .boxed_local()
        })))
    }

    /// Run a keyed foreground effect for its side effect without dispatching a
    /// follow-up message.
    ///
    /// Reusing `key` keeps only the latest keyed side effect authoritative for
    /// runtime completion handling.
    pub fn perform_keyed<AsyncFn>(key: impl Into<Key>, effect: AsyncFn) -> Self
    where
        AsyncFn: AsyncFnOnce(&mut AsyncApp) -> () + 'static,
    {
        Self::keyed_effect(
            key,
            Effect::Foreground(Box::new(move |cx| {
                async move {
                    effect(cx).await;
                    None
                }
                .boxed_local()
            })),
        )
    }

    /// Attach a descriptive label used for runtime observability.
    ///
    /// The runtime includes this label in [`crate::RuntimeEvent`] values such as
    /// [`crate::RuntimeEvent::CommandScheduled`] and
    /// [`crate::RuntimeEvent::EffectCompleted`].
    pub fn label(mut self, label: impl Into<Arc<str>>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Batch multiple commands into a single command value.
    ///
    /// Nested batches are flattened, and [`Command::none`] entries are
    /// discarded. The runtime executes the remaining commands in iterator order.
    ///
    /// # Examples
    ///
    /// ```
    /// use gpui_tea::Command;
    ///
    /// let command = Command::batch([
    ///     Command::none(),
    ///     Command::emit(1_u8),
    ///     Command::batch([Command::emit(2_u8)]),
    /// ]);
    /// let _ = command;
    /// ```
    pub fn batch(commands: impl IntoIterator<Item = Command<Msg>>) -> Self {
        let commands: Vec<_> = commands
            .into_iter()
            // Flatten nested declarative batches before the runtime sees them.
            .flat_map(|command| match command.inner {
                CommandInner::Batch(inner) => inner,
                CommandInner::None => Vec::new(),
                other => vec![Command {
                    inner: other,
                    label: command.label,
                }],
            })
            .collect();

        match commands.len() {
            0 => Self::none(),
            1 => {
                let mut commands = commands.into_iter();
                commands.next().unwrap_or_default()
            }
            _ => Self::new(CommandInner::Batch(commands)),
        }
    }

    /// Transform the message yielded by this command.
    ///
    /// Any keyed identity and label attached to the command are preserved, so
    /// mapping changes only the message type seen by the caller.
    ///
    /// # Examples
    ///
    /// ```
    /// use gpui_tea::Command;
    ///
    /// let command = Command::emit(21_u8)
    ///     .label("load")
    ///     .map(|value| value * 2);
    /// let _ = command;
    /// ```
    pub fn map<F, NewMsg>(self, f: F) -> Command<NewMsg>
    where
        F: Fn(Msg) -> NewMsg + Clone + Send + Sync + 'static,
        NewMsg: Send + 'static,
    {
        let label = self.label;

        match self.inner {
            CommandInner::None => Command::none(),
            CommandInner::Emit(message) => Command::emit(f(message)),
            CommandInner::Batch(commands) => {
                Command::batch(commands.into_iter().map(|command| command.map(f.clone())))
            }
            CommandInner::Effect(effect) => Command {
                inner: CommandInner::Effect(map_effect(effect, f)),
                label,
            },
            CommandInner::Keyed { key, effect } => Command {
                inner: CommandInner::Keyed {
                    key,
                    effect: map_effect(effect, f),
                },
                label,
            },
        }
    }

    /// Rebase keyed identities into a nested child path.
    ///
    /// Non-keyed commands are unchanged. Keyed commands keep their local key
    /// metadata while becoming unique within the provided path.
    pub fn scoped(self, path: &ChildPath) -> Self {
        if path.is_root() {
            return self;
        }

        let label = self.label;

        match self.inner {
            CommandInner::None => Command {
                inner: CommandInner::None,
                label,
            },
            CommandInner::Emit(message) => Command {
                inner: CommandInner::Emit(message),
                label,
            },
            CommandInner::Effect(effect) => Command {
                inner: CommandInner::Effect(effect),
                label,
            },
            CommandInner::Batch(commands) => Command {
                inner: CommandInner::Batch(
                    commands
                        .into_iter()
                        .map(|command| command.scoped(path))
                        .collect(),
                ),
                label,
            },
            CommandInner::Keyed { key, effect } => Command {
                inner: CommandInner::Keyed {
                    key: key.scoped(path),
                    effect,
                },
                label,
            },
        }
    }

    pub(crate) fn into_parts(self) -> (Option<Arc<str>>, CommandInner<Msg>) {
        (self.label, self.inner)
    }
}

fn map_effect<Msg, NewMsg, F>(effect: Effect<Msg>, f: F) -> Effect<NewMsg>
where
    Msg: Send + 'static,
    NewMsg: Send + 'static,
    F: Fn(Msg) -> NewMsg + Clone + Send + Sync + 'static,
{
    match effect {
        Effect::Foreground(inner) => Effect::Foreground(Box::new(move |cx| {
            let future = inner(cx);
            let mapper = f.clone();
            // Preserve the original execution context and rewrite only the
            // produced message.
            async move { future.await.map(mapper) }.boxed_local()
        })),
        Effect::Background(inner) => Effect::Background(Box::new(move |executor| {
            let future = inner(executor);
            let mapper = f.clone();
            async move { future.await.map(mapper) }.boxed()
        })),
    }
}

impl<Msg> fmt::Debug for Command<Msg> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = formatter.debug_struct("Command");
        debug.field("variant", &command_variant(&self.inner));
        debug.field("execution_kind", &command_execution_kind(&self.inner));
        debug.field("label", &self.label.as_deref());
        debug.field("key", &command_key(&self.inner));
        if let CommandInner::Batch(commands) = &self.inner {
            debug.field("batch_len", &commands.len());
        }
        debug.finish_non_exhaustive()
    }
}

fn command_variant<Msg>(inner: &CommandInner<Msg>) -> &'static str {
    match inner {
        CommandInner::None => "None",
        CommandInner::Emit(_) => "Emit",
        CommandInner::Batch(_) => "Batch",
        CommandInner::Effect(_) => "Effect",
        CommandInner::Keyed { .. } => "Keyed",
    }
}

fn command_execution_kind<Msg>(inner: &CommandInner<Msg>) -> Option<CommandKind> {
    match inner {
        CommandInner::Effect(effect) | CommandInner::Keyed { effect, .. } => Some(effect.kind()),
        CommandInner::None | CommandInner::Emit(_) | CommandInner::Batch(_) => None,
    }
}

fn command_key<Msg>(inner: &CommandInner<Msg>) -> Option<&Key> {
    match inner {
        CommandInner::Keyed { key, .. } => Some(key),
        CommandInner::None
        | CommandInner::Emit(_)
        | CommandInner::Batch(_)
        | CommandInner::Effect(_) => None,
    }
}

impl<Msg: Send + 'static> Default for Command<Msg> {
    fn default() -> Self {
        Self::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AsyncApp, TestAppContext};

    #[allow(clippy::missing_panics_doc)]
    #[test]
    fn batch_discards_none_and_flattens_nested_batches() {
        let command = Command::batch([
            Command::none(),
            Command::emit(1),
            Command::batch([Command::none(), Command::emit(2)]),
        ]);

        let (label, inner) = command.into_parts();
        assert!(label.is_none());

        match inner {
            CommandInner::Batch(commands) => {
                assert_eq!(commands.len(), 2);

                let first = commands
                    .into_iter()
                    .map(Command::into_parts)
                    .collect::<Vec<_>>();
                assert!(matches!(first[0].1, CommandInner::Emit(1)));
                assert!(matches!(first[1].1, CommandInner::Emit(2)));
            }
            other => panic!(
                "expected flattened batch, got unexpected command: {:?}",
                kind(&other)
            ),
        }
    }

    #[allow(clippy::missing_panics_doc)]
    #[gpui::test]
    async fn mapped_keyed_foreground_command_preserves_label_and_key_and_transforms_message(
        cx: &mut TestAppContext,
    ) {
        async fn effect(_cx: &mut AsyncApp) -> Option<i32> {
            Some(21)
        }

        let command = Command::foreground_keyed("load", effect)
            .label("mapped-load")
            .map(|value| format!("value:{value}"));

        let (label, inner) = command.into_parts();
        assert_eq!(label.as_deref(), Some("mapped-load"));

        match inner {
            CommandInner::Keyed {
                key,
                effect: Effect::Foreground(effect),
            } => {
                assert_eq!(key, Key::new("load"));

                let mut async_cx = cx.to_async();
                let message = effect(&mut async_cx).await;
                assert_eq!(message.as_deref(), Some("value:21"));
            }
            other => panic!("expected keyed foreground effect, got {:?}", kind(&other)),
        }
    }

    fn kind<Msg>(inner: &CommandInner<Msg>) -> &'static str {
        match inner {
            CommandInner::None => "none",
            CommandInner::Emit(_) => "emit",
            CommandInner::Batch(_) => "batch",
            CommandInner::Effect(_) => "effect",
            CommandInner::Keyed { .. } => "keyed",
        }
    }
}
