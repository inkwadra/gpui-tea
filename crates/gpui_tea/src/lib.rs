//! Build Elm-style GPUI programs with composable models, commands, and subscriptions.
//!
//! `gpui_tea` packages the runtime primitives needed to drive a
//! [The Elm Architecture](https://guide.elm-lang.org/architecture/) style application on top of
//! `gpui`. The crate exposes:
//!
//! - [`Model`] for state transitions and rendering
//! - [`Program`] for mounting and running a model inside GPUI
//! - [`Command`] for immediate, foreground, and background effects
//! - [`Subscription`] and [`Subscriptions`] for long-lived external event sources
//! - [`ModelContext`] and [`ChildScope`] for nesting child models under explicit paths
//!
//! Most applications implement [`Model`] and mount it with [`Program::mount()`] or
//! [`ModelExt::into_program()`].

mod command;
mod dispatcher;
mod error;
mod key;
mod keyed_tasks;
mod model;
mod observability;
mod program;
mod queue;
mod runtime;
mod scope;
mod subscription;
mod subscription_registry;
mod view;

pub use command::{Command, CommandKind};
pub use dispatcher::Dispatcher;
pub use error::{Error, Result};
pub use gpui_tea_macros::Composite;
pub use key::Key;
pub use model::{Model, ModelExt};
#[cfg(feature = "metrics")]
pub use observability::observe_metrics_telemetry;
#[cfg(feature = "tracing")]
pub use observability::observe_tracing_telemetry;
pub use observability::{
    ProgramConfig, ProgramId, QueueOverflowAction, QueuePolicy, RuntimeEvent, TelemetryEnvelope,
    TelemetryEvent, TelemetryMetadata,
};
pub use program::{Program, RuntimeSnapshot};
pub use scope::{ChildPath, ChildScope, ModelContext};
pub use subscription::{SubHandle, Subscription, SubscriptionContext, Subscriptions};
pub use view::{IntoView, View};
