#![warn(missing_docs)]
//! `gpui_tea` provides the runtime primitives for building TEA-style
//! applications on top of GPUI.
//!
//! The crate defines a `Model`-driven update loop, declarative `Command` and
//! `Subscription` types, and a `Program` runtime that coordinates message
//! delivery, queue draining, async effect execution, keyed task replacement,
//! and subscription reconciliation.
//!
//! The design stays intentionally GPUI-first. Models continue to work directly
//! with GPUI's `App`, views return [`View`] as a thin wrapper around ordinary
//! GPUI elements, and mounted programs remain standard GPUI entities rather
//! than being wrapped in a separate UI abstraction.
//!
//! This makes the crate suitable for applications that want explicit state
//! transitions, controlled follow-up work, and long-lived external event
//! sources, while preserving direct access to GPUI's rendering and application
//! model.
//!
//! ## Runtime Guarantees
//!
//! - `Program::mount` creates the runtime, calls `Model::init` exactly once,
//!   executes the initial command, and reconciles the initial subscription set.
//! - `Dispatcher::dispatch` is enqueue-only and never re-enters
//!   `Model::update` inline.
//! - The runtime drains its internal message queue in FIFO order.
//! - Re-entrant dispatch appends to the current queue and never runs
//!   `Model::update` recursively; the outermost queue pass drains the appended
//!   messages.
//! - The queue is unbounded by design.
//! - Keyed commands are latest-wins at completion time. If older keyed work
//!   completes after being replaced, its completion is ignored as stale.
//! - Subscription identity is key-based. Reusing a key reuses the existing
//!   handle, and changing a key rebuilds the subscription.
//! - Dispatching after the program is released fails with
//!   [`Error::ProgramUnavailable`].
//!
//! ## Examples
//!
//! ```no_run
//! use gpui::{App, ParentElement, Window, div};
//! use gpui_tea::{Command, Dispatcher, IntoView, Model, Program, View};
//!
//! #[derive(Clone, Copy)]
//! enum Msg {
//!     Increment,
//! }
//!
//! struct Counter(i32);
//!
//! impl Model for Counter {
//!     type Msg = Msg;
//!
//!     fn update(&mut self, msg: Self::Msg, _cx: &mut App) -> Command<Self::Msg> {
//!         match msg {
//!             Msg::Increment => self.0 += 1,
//!         }
//!
//!         Command::none()
//!     }
//!
//!     fn view(
//!         &self,
//!         _window: &mut Window,
//!         _cx: &mut App,
//!         _dispatcher: &Dispatcher<Self::Msg>,
//!     ) -> View {
//!         div().child(self.0.to_string()).into_view()
//!     }
//! }
//!
//! # fn mount(cx: &mut App) {
//! let _program = Program::mount(Counter(0), cx);
//! # }
//! ```
//!
//! A mounted program can also schedule an initial command through
//! [`Model::init`]:
//!
//! ```no_run
//! use gpui::{App, Window, div};
//! use gpui_tea::{Command, Dispatcher, IntoView, Model, View};
//!
//! enum Msg {
//!     BootstrapLoaded,
//! }
//!
//! struct Bootstrapped;
//!
//! impl Model for Bootstrapped {
//!     type Msg = Msg;
//!
//!     fn init(&mut self, _cx: &mut App) -> Command<Self::Msg> {
//!         Command::emit(Msg::BootstrapLoaded).label("bootstrap")
//!     }
//!
//!     fn update(&mut self, _msg: Self::Msg, _cx: &mut App) -> Command<Self::Msg> {
//!         Command::none()
//!     }
//!
//!     fn view(
//!         &self,
//!         _window: &mut Window,
//!         _cx: &mut App,
//!         _dispatcher: &Dispatcher<Self::Msg>,
//!     ) -> View {
//!         div().into_view()
//!     }
//! }
//! ```
//!
//! Keyed commands make the most recent completion authoritative for a stable
//! key:
//!
//! ```no_run
//! use gpui::{App, Window, div};
//! use gpui_tea::{Command, Dispatcher, IntoView, Model, View};
//! use std::time::Duration;
//!
//! enum Msg {
//!     Run,
//!     Loaded(&'static str),
//! }
//!
//! struct KeyedDemo;
//!
//! impl Model for KeyedDemo {
//!     type Msg = Msg;
//!
//!     fn update(&mut self, msg: Self::Msg, _cx: &mut App) -> Command<Self::Msg> {
//!         match msg {
//!             Msg::Run => Command::batch([
//!                 Command::background_keyed("load", |_| async move {
//!                     std::thread::sleep(Duration::from_millis(800));
//!                     Some(Msg::Loaded("slow"))
//!                 }),
//!                 Command::background_keyed("load", |_| async move {
//!                     std::thread::sleep(Duration::from_millis(150));
//!                     Some(Msg::Loaded("fast"))
//!                 }),
//!             ]),
//!             Msg::Loaded(_) => Command::none(),
//!         }
//!     }
//!
//!     fn view(
//!         &self,
//!         _window: &mut Window,
//!         _cx: &mut App,
//!         _dispatcher: &Dispatcher<Self::Msg>,
//!     ) -> View {
//!         div().into_view()
//!     }
//! }
//! ```
//!
//! Subscriptions declare long-lived external inputs by stable key:
//!
//! ```no_run
//! use gpui::{App, Window, div};
//! use gpui_tea::{
//!     Command, Dispatcher, IntoView, Model, SubHandle, Subscription, Subscriptions, View,
//! };
//!
//! enum Msg {
//!     FromSubscription(&'static str),
//! }
//!
//! struct SubscriptionDemo {
//!     active_key: Option<&'static str>,
//! }
//!
//! impl Model for SubscriptionDemo {
//!     type Msg = Msg;
//!
//!     fn update(&mut self, _msg: Self::Msg, _cx: &mut App) -> Command<Self::Msg> {
//!         Command::none()
//!     }
//!
//!     fn subscriptions(&self, _cx: &mut App) -> Subscriptions<Self::Msg> {
//!         let Some(key) = self.active_key else {
//!             return Subscriptions::none();
//!         };
//!
//!         Subscriptions::one(Subscription::new(key, move |cx| {
//!             cx.dispatch(Msg::FromSubscription(key))
//!                 .expect("the mounted program is alive while the subscription is being built");
//!             SubHandle::None
//!         }))
//!     }
//!
//!     fn view(
//!         &self,
//!         _window: &mut Window,
//!         _cx: &mut App,
//!         _dispatcher: &Dispatcher<Self::Msg>,
//!     ) -> View {
//!         div().into_view()
//!     }
//! }
//! ```
//!
//! [`ProgramConfig`] can attach runtime observers and structured telemetry:
//!
//! ```no_run
//! use gpui::{App, Window, div};
//! use gpui_tea::{
//!     Command, Dispatcher, IntoView, Model, Program, ProgramConfig, RuntimeEvent, View,
//! };
//!
//! enum Msg {
//!     Run,
//! }
//!
//! struct Observed;
//!
//! impl Model for Observed {
//!     type Msg = Msg;
//!
//!     fn update(&mut self, _msg: Self::Msg, _cx: &mut App) -> Command<Self::Msg> {
//!         Command::none()
//!     }
//!
//!     fn view(
//!         &self,
//!         _window: &mut Window,
//!         _cx: &mut App,
//!         _dispatcher: &Dispatcher<Self::Msg>,
//!     ) -> View {
//!         div().into_view()
//!     }
//! }
//!
//! # fn mount(cx: &mut App) {
//! let config = ProgramConfig::new()
//!     .describe_program(|program_id| format!("program:{program_id}"))
//!     .describe_message(|_msg: &Msg| "run")
//!     .describe_key(|key| format!("{key:?}"))
//!     .observer(|event| {
//!         if let RuntimeEvent::EffectCompleted { label, .. } = event {
//!             let _ = label;
//!         }
//!     })
//!     .telemetry_observer(|_envelope| {});
//!
//! let _program = Program::mount_with(Observed, config, cx);
//! # }
//! ```
//!
//! Runnable examples for these scenarios live in `examples/counter.rs`,
//! `examples/init_command.rs`, `examples/keyed_effect.rs`,
//! `examples/subscriptions.rs`, `examples/observability.rs`, and
//! `examples/telemetry.rs`.

mod command;
mod dispatcher;
mod error;
mod nested;
mod observability;
mod program;
mod runtime;
mod subscription;
mod view;

pub use command::{Command, CommandKind, Key};
pub use dispatcher::Dispatcher;
pub use error::{Error, Result};
pub use nested::{ChildPath, ChildScope, ModelContext, NestedModel};
#[cfg(feature = "metrics")]
pub use observability::observe_metrics_telemetry;
#[cfg(feature = "tracing")]
pub use observability::observe_tracing_telemetry;
pub use observability::{
    ProgramConfig, ProgramId, QueueOverflowAction, QueuePolicy, RuntimeEvent, TelemetryEnvelope,
    TelemetryEvent, TelemetryMetadata,
};
pub use program::{Model, ModelExt, Program, RuntimeSnapshot};
pub use subscription::{SubHandle, Subscription, SubscriptionContext, Subscriptions};
pub use view::{IntoView, View};
