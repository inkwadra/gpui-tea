# gpui-tea

[![CI](https://github.com/inkwadra/gpui-tea/actions/workflows/ci.yml/badge.svg)](https://github.com/inkwadra/gpui-tea/actions/workflows/ci.yml) [![Crates.io](https://img.shields.io/crates/v/gpui_tea.svg)](https://crates.io/crates/gpui_tea) [![docs.rs](https://img.shields.io/docsrs/gpui_tea)](https://docs.rs/gpui_tea) [![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)

> [!WARNING]
> The public API is intended to be stable for real use, but future releases may still refine interfaces or introduce compatibility-affecting changes where needed.

`gpui_tea` provides the runtime primitives for building TEA-style applications on top of GPUI.

The crate defines a `Model`-driven update loop, declarative `Command` and
`Subscription` types, and a `Program` runtime that coordinates message
delivery, queue draining, async effect execution, keyed task replacement, and
subscription reconciliation.

The design stays intentionally GPUI-first. Models continue to work directly
with GPUI's `App`, views return `gpui_tea::View` as a thin wrapper around
ordinary GPUI elements, and mounted programs remain standard GPUI entities
rather than being wrapped in a separate UI abstraction.

This makes the crate suitable for applications that want explicit state
transitions, controlled follow-up work, and long-lived external event sources,
while preserving direct access to GPUI's rendering and application model.

## Table of Contents

- [Features](#features)
- [Requirements](#requirements)
- [Installation](#installation)
- [Usage](#usage)
- [Core Concepts](#core-concepts)
- [Runtime Guarantees](#runtime-guarantees)
- [Observability](#observability)
- [Development](#development)
- [License](#license)

## Features

- Explicit message handling through `Model::update`
- FIFO queue processing with enqueue-only dispatch
- Non-reentrant updates, even when handlers emit more messages
- Keyed commands with latest-wins completion semantics
- Declarative subscriptions keyed by stable identity
- Runtime hooks for queue, command, effect, and subscription lifecycle events

## Requirements

- Stable Rust toolchain
- `clippy` and `rustfmt` are included in the repository toolchain configuration
- GPUI has platform-specific system requirements; follow the upstream
  [`gpui` documentation](https://docs.rs/gpui/latest/gpui/) for environment setup details

## Installation

Install the published crate from `crates.io`:

```toml
[dependencies]
gpui_tea = "0.1.0"
```

If you prefer `cargo add`:

```bash
cargo add gpui_tea
```

Use a Git dependency when you want unreleased changes from the repository:

```toml
[dependencies]
gpui_tea = { git = "https://github.com/inkwadra/gpui-tea" }
```

The crate name is `gpui_tea` in code and dependency declarations.

## Usage

The snippets below show the shape of the API. Each scenario has a corresponding
runnable program in [`examples/`](examples) that keeps the full GPUI setup and
UI code out of the README.

`View` is the render boundary for `Model::view`. It stays GPUI-first by
accepting any GPUI `IntoElement`, and `.into_view()` is the default ergonomic
conversion in user code.

### Minimal program

Start with a `Model` that owns state, updates in response to messages, renders
GPUI elements, and mounts through `Program::mount`.

```rust
use gpui::{App, ParentElement, Window, div};
use gpui_tea::{Command, Dispatcher, IntoView, Model, View};

enum Msg {
    Increment,
}

struct Counter {
    value: i32,
}

impl Model for Counter {
    type Msg = Msg;

    fn update(&mut self, msg: Self::Msg, _cx: &mut App) -> Command<Self::Msg> {
        match msg {
            Msg::Increment => self.value += 1,
        }

        Command::none()
    }

    fn view(
        &self,
        _window: &mut Window,
        _cx: &mut App,
        _dispatcher: &Dispatcher<Self::Msg>,
    ) -> View {
        div().child(self.value.to_string()).into_view()
    }
}
```

Run it: [`examples/counter.rs`](examples/counter.rs) with `cargo run --example counter`

### Initialize with an initial command

Use `Model::init` when the mounted program should schedule work before handling
normal user-driven messages.

```rust
use gpui::{App, Window, div};
use gpui_tea::{Command, Dispatcher, IntoView, Model, View};

enum Msg {
    BootstrapLoaded,
}

struct BootstrappedCounter;

impl Model for BootstrappedCounter {
    type Msg = Msg;

    fn init(&mut self, _cx: &mut App) -> Command<Self::Msg> {
        Command::emit(Msg::BootstrapLoaded).label("bootstrap")
    }

    fn update(&mut self, _msg: Self::Msg, _cx: &mut App) -> Command<Self::Msg> {
        Command::none()
    }

    fn view(
        &self,
        _window: &mut Window,
        _cx: &mut App,
        _dispatcher: &Dispatcher<Self::Msg>,
    ) -> View {
        div().into_view()
    }
}
```

Run it: [`examples/init_command.rs`](examples/init_command.rs) with `cargo run --example init_command`

### Run keyed async work

Use keyed commands when later work should become authoritative for the same
logical operation. Earlier completions on the same key are ignored as stale.

```rust
use gpui::{App, Window, div};
use gpui_tea::{Command, Dispatcher, IntoView, Model, View};
use std::time::Duration;

enum Msg {
    Run,
    Loaded(&'static str),
}

struct Demo;

impl Model for Demo {
    type Msg = Msg;

    fn update(&mut self, msg: Self::Msg, _cx: &mut App) -> Command<Self::Msg> {
        match msg {
            Msg::Run => Command::batch([
                Command::background_keyed("load", |_| async move {
                    std::thread::sleep(Duration::from_millis(800));
                    Some(Msg::Loaded("slow"))
                }),
                Command::background_keyed("load", |_| async move {
                    std::thread::sleep(Duration::from_millis(150));
                    Some(Msg::Loaded("fast"))
                }),
            ]),
            Msg::Loaded(_) => Command::none(),
        }
    }

    fn view(
        &self,
        _window: &mut Window,
        _cx: &mut App,
        _dispatcher: &Dispatcher<Self::Msg>,
    ) -> View {
        div().into_view()
    }
}
```

Run it: [`examples/keyed_effect.rs`](examples/keyed_effect.rs) with `cargo run --example keyed_effect`

### Declare subscriptions

Use `Subscription` values to declare long-lived external inputs by stable key.
Reusing a key retains the existing handle; changing it rebuilds the
subscription.

```rust
use gpui::{App, Window, div};
use gpui_tea::{
    Command, Dispatcher, IntoView, Model, SubHandle, Subscription, Subscriptions, View,
};

enum Msg {
    FromSubscription(&'static str),
}

struct Demo {
    active_key: Option<&'static str>,
}

impl Model for Demo {
    type Msg = Msg;

    fn update(&mut self, _msg: Self::Msg, _cx: &mut App) -> Command<Self::Msg> {
        Command::none()
    }

    fn subscriptions(&self, _cx: &mut App) -> Subscriptions<Self::Msg> {
        let Some(key) = self.active_key else {
            return Subscriptions::none();
        };

        Subscriptions::one(Subscription::new(key, move |cx| {
            cx.dispatch(Msg::FromSubscription(key)).unwrap();
            SubHandle::None
        }))
    }

    fn view(
        &self,
        _window: &mut Window,
        _cx: &mut App,
        _dispatcher: &Dispatcher<Self::Msg>,
    ) -> View {
        div().into_view()
    }
}
```

Run it: [`examples/subscriptions.rs`](examples/subscriptions.rs) with `cargo run --example subscriptions`

### Observe runtime activity

Use `ProgramConfig` when the mounted program should emit runtime-level events
for dispatch, queue, effect, and subscription activity.

```rust
use gpui::App;
use gpui_tea::{Program, ProgramConfig, RuntimeEvent};

enum Msg {
    Run,
}

let config = ProgramConfig::new()
    .describe_message(|_msg: &Msg| "run")
    .describe_key(|key| format!("{key:?}"))
    .observer(|event| {
        if let RuntimeEvent::EffectCompleted { label, .. } = event {
            eprintln!("completed: {}", label.unwrap_or("<none>"));
        }
    });

// Pass config to Program::mount_with(model, config, cx).
```

Run it: [`examples/observability.rs`](examples/observability.rs) with `cargo run --example observability`

For the complete public API surface, published documentation lives on
[`docs.rs`](https://docs.rs/gpui_tea).

## Core Concepts

- `Msg`
  The application protocol shared by `Model::update`, `Dispatcher::dispatch`, commands, and subscriptions.
- `Model`
  Defines the state transition contract. `init` returns the initial `Command`, `update` handles one message,
  `view` renders GPUI elements, and `subscriptions` declares the active subscription set for the current state.
- `Program`
  Hosts one mounted `Model` inside a GPUI entity and owns the runtime for that program instance.
- `Dispatcher`
  An enqueue-only handle for sending messages back into a mounted program. Cloning it is cheap and intended for UI
  event handlers and subscription builders.
- `Command`
  Describes follow-up work after a state transition. Use it for immediate emits, foreground effects, background
  effects, keyed work, and ordered batches.
- `Subscription` and `Subscriptions`
  Declare long-lived external inputs with stable keys. The declared set is reconciled against the active runtime
  state after initialization and after queue drains that process messages.
- `Key`
  The stable identity used by keyed commands and subscriptions. Key reuse is what gives keyed effects and
  subscriptions their lifecycle semantics.
- `ProgramConfig` and `RuntimeObserver`
  Configure runtime instrumentation before mounting a program, including observers, queue warning thresholds, and
  message and key formatters for emitted events.

## Runtime Guarantees

- `Program::mount` and `Program::mount_with` create the runtime, call `Model::init` once, execute the initial
  command, and reconcile the initial subscription set
- `Dispatcher::dispatch` is enqueue-only and never re-enters `Model::update` inline
- The runtime drains its internal message queue in FIFO order
- Messages dispatched while a queue drain is already in progress are appended and processed later instead of
  recursively re-entering `Model::update`
- The internal message queue is intentionally unbounded
- A newer keyed command replaces the previously tracked task for the same key and makes that newer work
  authoritative
- Replacing a keyed command releases the previously tracked task handle; if older keyed work still completes after
  being replaced, its completion is treated as stale and does not enqueue a message
- Subscription identity is key-based; reusing a key retains the existing handle, and changing or removing the key
  rebuilds or drops the subscription during reconciliation
- Dispatching after a program is released fails with `DispatchError`

These guarantees are enforced by the repository's runtime contract tests in
[`tests/runtime_contracts.rs`](tests/runtime_contracts.rs).

## Observability

`ProgramConfig` lets you observe runtime activity synchronously through a `RuntimeObserver`. The runtime can emit
events for dispatch acceptance and rejection, queue drain boundaries, queue growth warnings, command scheduling,
keyed replacement, effect completion, stale keyed completions, and subscription lifecycle changes.

Use `ProgramConfig::describe_message` when event payloads should include a human-readable message description, and
use `ProgramConfig::describe_key` when keyed commands or subscriptions should report stable identifiers in a more
meaningful form.

Use `Command::label` and `Subscription::label` when you need event streams to carry names for specific effects or
subscription builders. Labels become visible in runtime events such as `CommandScheduled`, `EffectCompleted`,
`SubscriptionBuilt`, `SubscriptionRetained`, and `SubscriptionRemoved`.

## Development

The repository keeps its local and CI workflow centered on [`Justfile`](Justfile).
For a full pre-push validation pass:

```bash
just qa
```

`just qa` runs the same core gates the project expects to stay green:

```bash
cargo fmt --all --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
cargo test --all-targets --all-features
typos
```

Use `just help` to inspect the rest of the local recipes.

## License

Licensed under Apache-2.0. See [`LICENSE`](LICENSE).
