# gpui-tea Project Instructions

This file provides context and instructions for AI agents working in this repository.

## Project Overview

`gpui-tea` is a Rust workspace that provides runtime primitives for building Elm-style (The Elm Architecture) applications on top of the `gpui` framework (by Zed). It enables composable models, commands, and subscriptions.

Key features and concepts include:
- **`Model`**: Defines state transitions and rendering.
- **`Program`**: Used for mounting and running a model inside GPUI.
- **`Command`**: Handles immediate, foreground, and background effects.
- **`Subscription` & `Subscriptions`**: Manages long-lived external event sources.
- **Nested Models**: Features like `ModelContext` and `ChildScope` support nesting child models under explicit paths.
- **Observability**: Built-in support for telemetry and tracing.
- **Workspace Crates**:
  - `crates/gpui_tea`: The main library containing the runtime and core concepts.
  - `crates/gpui_tea_macros`: Procedural macros, such as `Composite`.

## Building, Running, and Testing

The project uses `just` as its command runner.

Local tooling prerequisites for the documented workflow:
- `just`
- `typos`
- Rust `stable` with `clippy` and `rustfmt` components

Key development commands:
- `just fmt`: Format the workspace.
- `just fmt-check`: Verify formatting without changes.
- `just check`: Compile all targets and features.
- `just clippy`: Run Clippy with warnings denied.
- `just typos`: Run the typo checker.
- `just lint`: Run Clippy and typos.
- `just test`: Run the full test suite.
- `just doc`: Build docs with rustdoc warnings denied.
- `just qa`: Run the full pre-push quality gate (formats, checks, lints, docs, and tests).
- `just fix`: Apply local formatting and Clippy fixes.

## Development Conventions

- **Strict Lints**: The workspace enforces strict rustc and clippy lints (`unsafe_code` is forbidden, missing docs trigger warnings).
- **Quality Gates**: Always run `just qa` before submitting or committing changes to ensure formatting, typing, clippy, doc generation, and tests all pass.
- **Testing**: Tests are located within the `tests/` directories or in inline `#[cfg(test)]` modules. Ensure that tests are updated or added for any new functionality.
- **Documentation**: All public APIs must be documented. Run `just doc` to ensure no warnings are present.
