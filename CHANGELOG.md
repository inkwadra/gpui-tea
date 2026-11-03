# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> Note: This repository publishes package-scoped release tags (`gpui_tea-v0.1.0` and
> `gpui_tea_macros-v0.1.0`) instead of plain `vX.Y.Z` tags. This changelog uses the shared
> workspace version heading for readability and links to the canonical `gpui_tea` release tag.

## [Unreleased]

### Added

- Add `RuntimeEvent::QueueOverflow` so runtime observers can see queue backpressure without
  enabling the telemetry pipeline.
- Allow `DropOldest` with `capacity: 0` to accept a message when there is nothing to displace.
- Add `MetricsAdapterConfig` and `MetricsAdapter` builder to customize metric label emissions,
  supporting stable `"program"` labels from `program_name` or `program_description` and keeping
  monotonic `program_id` optional.
- Add `CommandKind::Cancel` variant so keyed command cancellations are observable via
  `TelemetryEvent::CommandScheduled`.
- Add diagnostic compiler errors in `Composite` derive macro when `#[child]` is placed on a struct
  or `#[composite]` is placed on a field.
- Add trybuild UI compile-fail tests for unsupported item types (enums, unions, tuple structs) and
  missing child attributes (`path`, `lift`, `extract`).
- Add trybuild UI compile-pass tests for generic `Composite` structs, closures in `lift`/`extract`,
  raw identifiers, and zero-child models.
- Add integration tests for bounded queue policies under mounted programs, internal command delivery
  guarantees, and effect / subscription cancellations on program drop.
- Add release workflow checks validating git tag version against `Cargo.toml` and ensuring macro
  dependencies are published to crates.io before `gpui_tea`.
- Add `ProgramConfig::drain_budget` (default `1024`): a drain that exhausts its budget reconciles
  subscriptions, notifies the view, and continues on the next executor turn, so long `emit`
  chains no longer block rendering or starve external dispatches.
- Add `RuntimeEvent::ProgramUnmounted` and `TelemetryEvent::ProgramUnmounted`, emitted when a
  program is dropped, plus the `gpui_tea.program.unmounted` metrics counter.

### Changed

- **Breaking:** depend on GPUI from Zed's Git repository (default branch) instead of `gpui` 0.2.2
  from crates.io, and raise the minimum supported Rust version to 1.98 to match Zed's toolchain.
  Applications must declare `gpui = { git = "https://github.com/zed-industries/zed" }` so both
  crates share one GPUI build. Examples start through `gpui_platform::application()`.
- Stop enabling `gpui`'s `test-support` feature for downstream consumers; it is now a
  dev-dependency only.
- Move `tracing-subscriber` out of the `tracing` feature so library consumers no longer compile
  it; examples and tests still use it via dev-dependencies.
- **Breaking:** mark `CommandKind`, `RuntimeEvent`, `TelemetryEvent`, `QueuePolicy`,
  `QueueOverflowAction`, `SubHandle`, `RuntimeSnapshot`, and `TelemetryMetadata` as
  `#[non_exhaustive]` so future variants and fields are not breaking changes. Together with the
  new event variants and the `MetricsAdapter` change below, the next release must be `0.2.0`.
- **Breaking:** `MetricsAdapter` no longer implements `PartialEq`/`Eq`; it now carries shared
  gauge bookkeeping. Compare `MetricsAdapter::config()` instead.
- `RuntimeSnapshot::queue_depth`, `TelemetryMetadata::queue_depth`, and queue warnings now share
  one definition: accepted external and internal messages that have not been processed yet.
- Deliver externally dispatched messages in batches: one drain, one subscription reconciliation,
  and one view notification per wake-up instead of per message.
- `Composite` helpers now share the struct's visibility so the `Model` implementation can live in
  another module, and generated code reaches GPUI through `gpui_tea` instead of `::gpui`.
- Document that panics in `Model` callbacks are fatal: GPUI leaves the panicking program entity
  permanently leased.
- Document retain-by-key subscription semantics: retained keys keep the previously built resource
  and do not re-run the builder.
- Reject empty `#[child(path = "...")]` values in the `Composite` derive.
- Generate `Composite` helpers with `#[allow(dead_code)]` and point type errors at the child field.
- Change `observe_metrics_telemetry` to omit monotonic `program_id` labels by default to prevent
  high cardinality in Prometheus-compatible metric collectors.
- Guarantee delivery for internal model messages (`Command::emit` and async effect completions) so
  they bypass bounded `QueuePolicy` limits and are never dropped or displaced by external dispatches.
- Change `Composite` expansion for zero-child structs to emit `_cx` and `_scope` parameters,
  avoiding unused variable warnings under `#![deny(warnings)]`.

### Fixed

- Do not emit `EffectCompleted` for stale keyed completions; only `StaleKeyedCompletionIgnored`
  is emitted.
- Avoid `usize` underflow when resuming auto-drain suspension in release builds.
- Fix `QueuePolicy::DropOldest` overflow telemetry to describe the displaced/dropped old message
  rather than the newly accepted message.
- Fix `Command::cancel_key` to preserve command labels and emit `TelemetryEvent::CommandScheduled`.
- Fix `QueuePolicy::DropOldest` keeping displaced messages alive in an unbounded channel; displaced
  messages are now released as soon as they are displaced.
- Fix queue warnings ignoring the external dispatch backlog; they are now emitted by the
  dispatching side as soon as the queue depth exceeds the threshold.
- Fix `MetricsAdapter` gauges overwriting each other when several programs share a label set;
  `gpui_tea.queue.depth` and `gpui_tea.subscription.active` now report the sum over those programs.
- Release messages still waiting in the queue when a program is dropped instead of keeping them
  alive for as long as a `Dispatcher` clone exists.

## [0.1.1] - 2026-03-24

### Added

- Add lifetime-bound effect cancellation so in-flight work is canceled when a program drops and
  initialization commands follow the same runtime semantics as update commands.
- Add program identifiers to metrics telemetry so downstream consumers can distinguish runtime data
  from multiple mounted programs.

### Changed

- Change `Composite` derive validation to require string-literal child paths and reject duplicate
  sibling paths earlier.

### Fixed

- Fix mapped and batched emit commands to preserve labels so observability output remains accurate.
- Fix subscription rebuilds to remove superseded active subscriptions before replacement handlers
  are installed.
- Fix runtime event identifiers to remain globally monotonic so telemetry consumers can rely on
  stable ordering.

## [0.1.0] - 2026-03-21

### Added

- Add the initial public release of `gpui_tea` with Elm-style runtime primitives for GPUI,
  including models, programs, commands, and subscriptions.
- Add nested model composition through `ModelContext`, `ChildScope`, and the `Composite` derive
  macro so applications can structure complex UI trees with explicit message routing.
- Add runtime observability features, including telemetry hooks, queue policies, and keyed effect
  controls for long-running asynchronous work.
- Add runnable examples that demonstrate counters, initialization flows, keyed effects,
  subscriptions, nested models, and telemetry integration.

[unreleased]: https://github.com/inkwadra/gpui-tea/compare/gpui_tea-v0.1.1...HEAD
[0.1.1]: https://github.com/inkwadra/gpui-tea/releases/tag/gpui_tea-v0.1.1
[0.1.0]: https://github.com/inkwadra/gpui-tea/releases/tag/gpui_tea-v0.1.0
