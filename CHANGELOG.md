# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> Note: This repository publishes package-scoped release tags (`gpui_tea-v0.1.0` and
> `gpui_tea_macros-v0.1.0`) instead of plain `vX.Y.Z` tags. This changelog uses the shared
> workspace version heading for readability and links to the canonical `gpui_tea` release tag.

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
