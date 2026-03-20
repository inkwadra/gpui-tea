# gpui_tea

[![CI](https://github.com/inkwadra/gpui-tea/actions/workflows/ci.yml/badge.svg)](https://github.com/inkwadra/gpui-tea/actions/workflows/ci.yml) [![Crates.io](https://img.shields.io/crates/v/gpui_tea.svg)](https://crates.io/crates/gpui_tea) [![docs.rs](https://img.shields.io/docsrs/gpui_tea)](https://docs.rs/gpui_tea) [![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)

> [!WARNING]
> The public API is intended to be stable for real use, but future releases may still refine interfaces or introduce compatibility-affecting changes where needed.

TEA-style runtime primitives for Rust developers building desktop applications with GPUI.

`gpui_tea` is a Rust library for building Elm Architecture applications on top of
[GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui). You use it when you want a
mounted program with explicit state transitions, message-driven updates, and rendering that stays
inside GPUI's application model.

The crate is aimed at developers building desktop user interfaces with GPUI who want a structured
way to express initialization, synchronous updates, asynchronous effects, and long-lived event
sources. The public surface centers on `Model`, `Program`, `Command`, and `Subscription`, with
support for nested models through `ChildScope` and the `Composite` derive macro.

## Table of Contents

- [Features](#features)
- [Requirements](#requirements)
- [Installation](#installation)
- [Configuration](#configuration)
- [Usage](#usage)
- [API Reference](#api-reference)
- [Examples](#examples)
- [License](#license)

## Features

- TEA-style runtime for GPUI with a `Model` trait that separates `init`, `update`, `view`, and
  `subscriptions`.
- Command system for immediate messages, foreground effects, background effects, batching, and
  keyed latest-wins work whose stale completions are ignored.
- Declarative subscriptions that are retained, rebuilt, or removed by stable `Key` values.
- Nested model support through `ModelContext`, `ChildScope`, and `#[derive(Composite)]`.
- Runtime observability through `ProgramConfig`, `RuntimeEvent`, and `TelemetryEvent`, with
  optional adapters for `tracing` and `metrics`.

## Requirements

- Rust stable toolchain. The repository pins the `stable` channel in `rust-toolchain.toml`.
- A Cargo toolchain that supports Rust 2024 edition crates. The manifest does not declare a
  separate `rust-version`.
- For local development in this repository: `clippy`, `rustfmt`, and `typos`.
- For running the interactive examples: a desktop environment capable of opening GPUI windows.

The repository does not document additional external services such as databases, brokers, or
servers.

## Installation

Add the crate from crates.io:

```bash
cargo add gpui_tea
```

Enable optional telemetry integrations as needed:

```bash
cargo add gpui_tea --features tracing
cargo add gpui_tea --features metrics
```

To depend on the current repository state instead of a crates.io release, use a Git dependency:

```toml
[dependencies]
gpui_tea = { git = "https://github.com/inkwadra/gpui-tea" }
```

To build the workspace test suite from source:

```bash
git clone https://github.com/inkwadra/gpui-tea
cd gpui-tea
cargo test --workspace --all-targets --all-features
```

To run the repository's full validation gate, use:

```bash
just qa
```

## Configuration

`gpui_tea` does not use configuration files or required environment variables for normal library
use.

### Cargo Features

| Feature | Default | Description |
| --- | --- | --- |
| `metrics` | No | Enables `observe_metrics_telemetry`. |
| `tracing` | No | Enables `observe_tracing_telemetry` and the `telemetry` example. |

### Runtime Configuration

Use `ProgramConfig` when you need queue controls or observability hooks:

- `queue_policy(QueuePolicy)` selects unbounded, reject-new, drop-newest, or drop-oldest
  backpressure behavior. Under drop policies, `dispatch()` may still return `Ok(())` even when a
  message is discarded or an older queued message is displaced.
- `queue_warning_threshold(usize)` emits queue warning events whenever the current queue depth is
  greater than the threshold.
- `observer(...)` receives high-level `RuntimeEvent` values.
- `telemetry_observer(...)` receives structured `TelemetryEnvelope` values.
- `describe_message(...)`, `describe_key(...)`, and `describe_program(...)` attach readable
  descriptions to observability output.

The only environment variable referenced in repository examples is `RUST_LOG=debug`, which is used
when running the `telemetry` example.

## Usage

The usual flow is:

1. Define a message enum for your model.
2. Implement `Model` for your state type.
3. Return `Command` values from `init` or `update` for follow-up work.
4. Mount the model with `Program::mount(...)` or `ModelExt::into_program(...)`.

The smallest working shape looks like this:

```rust
use gpui::{App, Application, Bounds, Window, WindowBounds, WindowOptions, div, px, size};
use gpui::prelude::*;
use gpui_tea::{Command, Dispatcher, IntoView, Model, ModelContext, Program, View};

#[derive(Clone, Copy)]
enum Msg {
    Loaded,
}

struct Counter {
    value: i32,
}

impl Model for Counter {
    type Msg = Msg;

    fn init(&mut self, _cx: &mut App, _scope: &ModelContext<Self::Msg>) -> Command<Self::Msg> {
        Command::emit(Msg::Loaded)
    }

    fn update(
        &mut self,
        msg: Self::Msg,
        _cx: &mut App,
        _scope: &ModelContext<Self::Msg>,
    ) -> Command<Self::Msg> {
        match msg {
            Msg::Loaded => self.value = 1,
        }

        Command::none()
    }

    fn view(
        &self,
        _window: &mut Window,
        _cx: &mut App,
        _scope: &ModelContext<Self::Msg>,
        _dispatcher: &Dispatcher<Self::Msg>,
    ) -> View {
        div().child(format!("count: {}", self.value)).into_view()
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(640.0), px(480.0)), cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| Program::mount(Counter { value: 0 }, cx),
        )
        .unwrap();

        cx.activate(true);
    });
}
```

### Common Patterns

- Bootstrap state with `Model::init()` and return `Command::emit(...)` or an async command.
- Schedule asynchronous work with `Command::foreground(...)` or `Command::background(...)`.
- Replace in-flight work by key with `Command::foreground_keyed(...)` or
  `Command::background_keyed(...)`. Older tasks may still finish, but the runtime ignores stale
  completions.
- Cancel tracked keyed work with `Command::cancel_key(...)`.
- Declare long-lived external event sources in `subscriptions()` with `Subscription::new(...)`.
- Compose child models with `ModelContext::scope(...)` or `#[derive(Composite)]`.

## API Reference

### `Model`

Signature:

```rust
pub trait Model: Sized + 'static {
    type Msg: Send + 'static;

    fn init(&mut self, cx: &mut App, scope: &ModelContext<Self::Msg>) -> Command<Self::Msg>;

    fn update(
        &mut self,
        msg: Self::Msg,
        cx: &mut App,
        scope: &ModelContext<Self::Msg>,
    ) -> Command<Self::Msg>;

    fn view(
        &self,
        window: &mut Window,
        cx: &mut App,
        scope: &ModelContext<Self::Msg>,
        dispatcher: &Dispatcher<Self::Msg>,
    ) -> View;

    fn subscriptions(
        &self,
        cx: &mut App,
        scope: &ModelContext<Self::Msg>,
    ) -> Subscriptions<Self::Msg>;
}
```

- Parameters: `msg` is your domain message, `cx` is the GPUI application context, `scope`
  contains the current child path, and `dispatcher` sends messages back into the mounted program.
- Return type: `Command<Self::Msg>` from `init` and `update`, `View` from `view`,
  `Subscriptions<Self::Msg>` from `subscriptions`.

Example:

```rust
fn init(&mut self, _cx: &mut App, _scope: &ModelContext<Self::Msg>) -> Command<Self::Msg> {
    Command::emit(()).label("bootstrap")
}
```

### `Program::mount` And `Program::mount_with`

Signatures:

```rust
pub fn mount(model: M, cx: &mut App) -> Entity<Program<M>>;
pub fn mount_with(model: M, config: ProgramConfig<M::Msg>, cx: &mut App) -> Entity<Program<M>>;
```

- Parameters: `model` is the initial state, `config` customizes queue and observability behavior,
  and `cx` is the GPUI application context.
- Behavior: mounting immediately calls `Model::init()`, executes its returned command, and performs
  the initial subscription reconciliation before returning.
- Return type: `Entity<Program<M>>`.

Example:

```rust
let config = ProgramConfig::<Msg>::new().queue_warning_threshold(32);
let entity = Program::mount_with(Counter { value: 0 }, config, cx);
```

### `Command`

Representative constructors:

```rust
pub fn none() -> Command<Msg>;
pub fn emit(message: Msg) -> Command<Msg>;
pub fn batch(commands: impl IntoIterator<Item = Command<Msg>>) -> Command<Msg>;
pub fn foreground<AsyncFn>(effect: AsyncFn) -> Command<Msg>;
pub fn background<F, Fut>(effect: F) -> Command<Msg>;
pub fn foreground_keyed<AsyncFn>(key: impl Into<Key>, effect: AsyncFn) -> Command<Msg>;
pub fn background_keyed<F, Fut>(key: impl Into<Key>, effect: F) -> Command<Msg>;
pub fn cancel_key(key: impl Into<Key>) -> Command<Msg>;
pub fn map<F, NewMsg>(self, f: F) -> Command<NewMsg>;
```

- Parameters: commands take either a concrete message, an async effect closure, or a stable `Key`
  used for deduplication and cancellation.
- Keyed commands are latest-wins: scheduling a newer keyed command replaces the tracked task for
  that key, and any later completion from the older task is ignored rather than aborted.
- Return type: `Command<Msg>` or `Command<NewMsg>` for `map`.

Example:

```rust
Command::background_keyed("load-profile", |_| async move {
    Some(())
})
.label("profile-load")
```

### `Subscription` And `Subscriptions`

Signatures:

```rust
pub fn new<F>(key: impl Into<Key>, builder: F) -> Subscription<Msg>;
pub fn one(subscription: Subscription<Msg>) -> Subscriptions<Msg>;
pub fn batch(
    subscriptions: impl IntoIterator<Item = Subscription<Msg>>,
) -> Result<Subscriptions<Msg>>;
```

- Parameters: `key` is stable subscription identity, and `builder` receives a
  `SubscriptionContext<'_, Msg>` with access to `App` and the program `Dispatcher`.
- Constraint: keys must be unique within a `Subscriptions` set. `Subscriptions::batch(...)` and
  `push(...)` return `Error::DuplicateSubscriptionKey` when duplicates are declared.
- Return type: `Subscription<Msg>` or `Subscriptions<Msg>`.

Example:

```rust
Subscriptions::<()>::one(
    Subscription::new("clock", |cx| {
        cx.dispatch(()).expect("program should be mounted");
        gpui_tea::SubHandle::None
    })
    .label("clock-subscription"),
)
```

### `#[derive(Composite)]`

Syntax:

```rust
#[derive(Composite)]
#[composite(message = ParentMsg)]
struct Parent {
    #[child(path = "sidebar", lift = ParentMsg::Sidebar, extract = ParentMsg::into_sidebar)]
    sidebar: SidebarModel,
}
```

- Parameters: `message` declares the parent message type; each `child` attribute defines the child
  path, the lift function, and the extractor used to route parent messages back to the child.
- Generated helpers: the macro adds hidden aggregate methods `__composite_init`,
  `__composite_update`, and `__composite_subscriptions`, plus one hidden `<field>_view` helper per
  child field.
- Caveat: child subscriptions must still resolve to unique scoped keys. The generated composite
  subscription helper panics if two child subscriptions collide after scoping.

Example:

```rust
fn init(&mut self, cx: &mut App, scope: &ModelContext<Self::Msg>) -> Command<Self::Msg> {
    self.__composite_init(cx, scope)
}
```

## Examples

Run the packaged examples from the workspace root:

```bash
cargo run -p gpui_tea --example counter
cargo run -p gpui_tea --example init_command
cargo run -p gpui_tea --example keyed_effect
cargo run -p gpui_tea --example nested_models
cargo run -p gpui_tea --example subscriptions
cargo run -p gpui_tea --example observability
RUST_LOG=debug cargo run -p gpui_tea --example telemetry --features tracing
```

Each example focuses on one runtime behavior:

- `counter`: minimal mounted program and message dispatch from the view.
- `init_command`: bootstrap work triggered by `Model::init()`.
- `keyed_effect`: latest-wins async work on a stable key.
- `nested_models`: `Composite` composition and child path scoping.
- `subscriptions`: declarative subscription reconciliation by key.
- `observability`: `RuntimeEvent` hooks with readable labels.
- `telemetry`: structured tracing output for queue activity, keyed replacement, and cancellation.

For repository development, the `Justfile` mirrors CI:

```bash
just fmt
just fmt-check
just check
just clippy
just lint
just typos
just doc
just test
just qa
just fix
```

## License

Licensed under [Apache-2.0](LICENSE).
