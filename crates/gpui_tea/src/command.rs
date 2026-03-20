use crate::{ChildPath, Key};
use futures::{
    FutureExt,
    future::{BoxFuture, LocalBoxFuture},
};
use gpui::{AsyncApp, BackgroundExecutor};
use std::{fmt, future::Future, sync::Arc};

/// A function that returns a future running on the foreground.
pub(crate) type ForegroundEffectFn<Msg> =
    Box<dyn for<'a> FnOnce(&'a mut AsyncApp) -> LocalBoxFuture<'a, Option<Msg>> + 'static>;

/// A function that returns a future running on the background.
pub(crate) type BackgroundEffectFn<Msg> =
    Box<dyn FnOnce(BackgroundExecutor) -> BoxFuture<'static, Option<Msg>> + 'static>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Classify how a [`Command`] interacts with the runtime.
pub enum CommandKind {
    /// Emit a message synchronously back into the program queue.
    Emit,
    /// Run an effect on the GPUI foreground executor.
    Foreground,
    /// Run an effect on the background executor.
    Background,
}

/// The internal representation of an effect.
pub(crate) enum Effect<Msg> {
    /// A foreground effect.
    Foreground(ForegroundEffectFn<Msg>),
    /// A background effect.
    Background(BackgroundEffectFn<Msg>),
}

impl<Msg> Effect<Msg> {
    /// Return the execution kind for this effect.
    pub(crate) fn kind(&self) -> CommandKind {
        match self {
            Self::Foreground(_) => CommandKind::Foreground,
            Self::Background(_) => CommandKind::Background,
        }
    }
}

/// The internal representation of a command's execution payload.
pub(crate) enum CommandInner<Msg> {
    /// No action.
    None,
    /// Emit a single message.
    Emit(Msg),
    /// Execute multiple commands.
    Batch(Vec<Command<Msg>>),
    /// Execute an asynchronous effect.
    Effect(Effect<Msg>),
    /// Execute a keyed effect, potentially replacing or cancelling earlier ones.
    Keyed {
        /// The key identifying the effect.
        key: Key,
        /// The effect itself.
        effect: Effect<Msg>,
    },
    /// Cancel a running keyed effect.
    Cancel(Key),
}

#[must_use]
/// Describe work the runtime should perform after a model transition.
///
/// Commands are pure values. A model returns them from [`crate::Model::init()`] and
/// [`crate::Model::update()`], and [`crate::Program`] executes them after the current transition
/// completes.
pub struct Command<Msg> {
    pub(crate) inner: CommandInner<Msg>,
    pub(crate) label: Option<Arc<str>>,
}

impl<Msg: Send + 'static> Command<Msg> {
    /// Construct a new command from an inner definition.
    fn new(inner: CommandInner<Msg>) -> Self {
        Self { inner, label: None }
    }

    /// Construct an effect command.
    fn effect(effect: Effect<Msg>) -> Self {
        Self::new(CommandInner::Effect(effect))
    }

    /// Construct a keyed effect command.
    fn keyed_effect(key: impl Into<Key>, effect: Effect<Msg>) -> Self {
        Self::new(CommandInner::Keyed {
            key: key.into(),
            effect,
        })
    }

    /// Return a command that performs no work.
    ///
    /// # Returns
    ///
    /// An empty command.
    ///
    /// # Examples
    ///
    /// ```
    /// use gpui_tea::Command;
    ///
    /// let cmd: Command<()> = Command::none();
    /// ```
    pub fn none() -> Self {
        Self::new(CommandInner::None)
    }

    /// Return a command that enqueues `message` immediately.
    ///
    /// # Arguments
    ///
    /// * `message` - The message to enqueue.
    ///
    /// # Returns
    ///
    /// A command representing the emitted message.
    ///
    /// # Examples
    ///
    /// ```
    /// use gpui_tea::Command;
    ///
    /// let cmd = Command::emit("Hello");
    /// ```
    pub fn emit(message: Msg) -> Self {
        Self::new(CommandInner::Emit(message))
    }

    /// Return a command that runs an effect on the GPUI foreground executor.
    ///
    /// The effect receives an [`AsyncApp`] tied to the mounted program and may emit a follow-up
    /// message by returning `Some`.
    ///
    /// # Arguments
    ///
    /// * `effect` - The asynchronous closure to run on the foreground.
    ///
    /// # Returns
    ///
    /// A command representing the foreground effect.
    ///
    /// # Examples
    ///
    /// ```
    /// use gpui_tea::Command;
    /// use gpui::AsyncApp;
    ///
    /// async fn my_effect(cx: &mut AsyncApp) -> Option<()> {
    ///     None
    /// }
    ///
    /// let cmd = Command::foreground(my_effect);
    /// ```
    pub fn foreground<AsyncFn>(effect: AsyncFn) -> Self
    where
        AsyncFn: AsyncFnOnce(&mut AsyncApp) -> Option<Msg> + 'static,
    {
        Self::effect(Effect::Foreground(Box::new(move |cx| Box::pin(effect(cx)))))
    }

    /// Return a keyed foreground effect that replaces any in-flight effect with the same key.
    ///
    /// When a newer keyed command with the same [`Key`] is scheduled, the runtime ignores the
    /// completion of the older one.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to associate with this effect.
    /// * `effect` - The asynchronous closure to run.
    ///
    /// # Returns
    ///
    /// A command representing the keyed foreground effect.
    pub fn foreground_keyed<AsyncFn>(key: impl Into<Key>, effect: AsyncFn) -> Self
    where
        AsyncFn: AsyncFnOnce(&mut AsyncApp) -> Option<Msg> + 'static,
    {
        Self::keyed_effect(
            key,
            Effect::Foreground(Box::new(move |cx| Box::pin(effect(cx)))),
        )
    }

    /// Return a command that runs an effect on the background executor.
    ///
    /// The closure receives the executor so nested background work can be spawned without
    /// capturing external runtime state.
    ///
    /// # Arguments
    ///
    /// * `effect` - The closure to run on the background executor.
    ///
    /// # Returns
    ///
    /// A command representing the background effect.
    ///
    /// # Examples
    ///
    /// ```
    /// use gpui_tea::Command;
    ///
    /// let cmd = Command::<()>::background(|_executor| async move {
    ///     None
    /// });
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

    /// Return a keyed background effect that replaces any in-flight effect with the same key.
    ///
    /// When a newer keyed command with the same [`Key`] is scheduled, the runtime ignores the
    /// completion of the older one.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to associate with this effect.
    /// * `effect` - The closure to run on the background executor.
    ///
    /// # Returns
    ///
    /// A command representing the keyed background effect.
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

    /// Return a foreground effect that performs work without emitting a message.
    ///
    /// # Arguments
    ///
    /// * `effect` - The closure to run on the foreground executor.
    ///
    /// # Returns
    ///
    /// A command representing the foreground effect.
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

    /// Return a keyed foreground effect that performs work without emitting a message.
    ///
    /// When a newer keyed command with the same [`Key`] is scheduled, the runtime ignores the
    /// completion of the older one.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to associate with this effect.
    /// * `effect` - The closure to run on the foreground executor.
    ///
    /// # Returns
    ///
    /// A command representing the keyed foreground effect.
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

    /// Return a command that cancels the keyed effect associated with `key`.
    ///
    /// Canceling a key detaches the running task from runtime bookkeeping. If that task later
    /// completes, its result is ignored.
    ///
    /// # Arguments
    ///
    /// * `key` - The key identifying the effect to cancel.
    ///
    /// # Returns
    ///
    /// A command that performs the cancellation.
    ///
    /// # Examples
    ///
    /// ```
    /// use gpui_tea::Command;
    ///
    /// let cmd: Command<()> = Command::cancel_key("my_key");
    /// ```
    pub fn cancel_key(key: impl Into<Key>) -> Self {
        Self::new(CommandInner::Cancel(key.into()))
    }

    /// Attach a human-readable label for observability output.
    ///
    /// # Arguments
    ///
    /// * `label` - The label to attach to this command.
    ///
    /// # Returns
    ///
    /// The updated command with the label attached.
    pub fn label(mut self, label: impl Into<Arc<str>>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Combine multiple commands into a single command.
    ///
    /// Nested batches are flattened and [`Command::none()`] entries are discarded so the runtime
    /// does not spend work traversing empty structure.
    ///
    /// # Arguments
    ///
    /// * `commands` - The commands to combine.
    ///
    /// # Returns
    ///
    /// A new batched command.
    ///
    /// # Examples
    ///
    /// ```
    /// use gpui_tea::Command;
    ///
    /// let cmd = Command::batch([
    ///     Command::emit(1),
    ///     Command::emit(2),
    /// ]);
    /// ```
    pub fn batch(commands: impl IntoIterator<Item = Command<Msg>>) -> Self {
        let commands: Vec<_> = commands
            .into_iter()
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

    /// Transform the message type produced by this command.
    ///
    /// The mapper is applied to immediate messages as well as messages emitted by foreground and
    /// background effects.
    ///
    /// # Arguments
    ///
    /// * `f` - The mapping function.
    ///
    /// # Returns
    ///
    /// A new command producing the mapped message type.
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
            CommandInner::Cancel(key) => Command {
                inner: CommandInner::Cancel(key),
                label,
            },
        }
    }

    /// Scope keyed effects and cancellations under `path`.
    ///
    /// Commands that do not carry a [`Key`] are forwarded unchanged. The operation is recursive for
    /// nested batches.
    ///
    /// # Arguments
    ///
    /// * `path` - The [`ChildPath`] to scope the commands under.
    ///
    /// # Returns
    ///
    /// The scoped command.
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
            CommandInner::Cancel(key) => Command {
                inner: CommandInner::Cancel(key.scoped(path)),
                label,
            },
        }
    }

    /// Extract the label and inner command parts.
    pub(crate) fn into_parts(self) -> (Option<Arc<str>>, CommandInner<Msg>) {
        (self.label, self.inner)
    }
}

/// Map an effect's message type.
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

/// Get the debug name of a command inner variant.
fn command_variant<Msg>(inner: &CommandInner<Msg>) -> &'static str {
    match inner {
        CommandInner::None => "None",
        CommandInner::Emit(_) => "Emit",
        CommandInner::Batch(_) => "Batch",
        CommandInner::Effect(_) => "Effect",
        CommandInner::Keyed { .. } => "Keyed",
        CommandInner::Cancel(_) => "Cancel",
    }
}

/// Extract the execution kind from a command inner.
fn command_execution_kind<Msg>(inner: &CommandInner<Msg>) -> Option<CommandKind> {
    match inner {
        CommandInner::Effect(effect) | CommandInner::Keyed { effect, .. } => Some(effect.kind()),
        CommandInner::None
        | CommandInner::Emit(_)
        | CommandInner::Batch(_)
        | CommandInner::Cancel(_) => None,
    }
}

/// Extract the key from a command inner.
fn command_key<Msg>(inner: &CommandInner<Msg>) -> Option<&Key> {
    match inner {
        CommandInner::Keyed { key, .. } | CommandInner::Cancel(key) => Some(key),
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
            CommandInner::Cancel(_) => "cancel",
        }
    }
}
