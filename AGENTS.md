# Repository Guidelines

## Project Structure & Module Organization
`gpui-tea` is a Rust library crate. Core runtime code lives in `src/`:
`command.rs`, `dispatcher.rs`, `program.rs`, `runtime.rs`, `subscription.rs`,
`view.rs`, and `observability.rs`. Public exports are wired through
`src/lib.rs`, which also carries crate-level docs and examples.

Integration tests live in `tests/`, with runtime contract coverage in
`tests/runtime_contracts.rs`. Runnable examples live in `examples/`
(`counter.rs`, `subscriptions.rs`, `observability.rs`, etc.) and should stay
aligned with the public API. Repository tooling is defined in `Justfile`,
`rustfmt.toml`, `clippy.toml`, and `.github/workflows/ci.yml`.

## Build, Test, and Development Commands
Use the stable toolchain from `rust-toolchain.toml`.

- `just fmt`: format the workspace with `rustfmt`.
- `just fmt-check`: verify formatting without editing files.
- `just check`: compile all targets and features.
- `just clippy`: run Clippy with `-D warnings`.
- `just test`: run unit, integration, and example-adjacent test targets.
- `just doc`: build docs with `RUSTDOCFLAGS="-D warnings"`.
- `just qa`: run the full pre-push gate used by CI.
- `cargo run --example counter`: run a specific example locally.

## Coding Style & Naming Conventions
Follow idiomatic Rust 2024 with default `rustfmt` formatting (`style_edition =
"2024"`). Use snake_case for modules, files, and functions; PascalCase for
types and traits; SCREAMING_SNAKE_CASE for constants. Keep public APIs and
module names explicit and domain-focused. The crate forbids `unsafe` and warns
on missing docs, so document public items and avoid hidden behavior.

## Testing Guidelines
Prefer deterministic tests that exercise observable runtime behavior. GPUI
integration tests use `#[gpui::test]`; keep new coverage in `tests/` unless it
is tightly scoped to a private module. Name tests by behavior, for example
`dispatcher_preserves_fifo_delivery_for_enqueued_messages`. Run `just test`
before opening a PR and `just qa` before merging substantial changes.

## Commit & Pull Request Guidelines
Recent history follows Conventional Commit style with optional scopes and
breaking markers, for example `feat(view)!: ...`, `refactor(command)!: ...`,
and `docs(readme): ...`. Keep commit subjects imperative and specific.

PRs should explain the behavior change, note any API or runtime-contract impact,
link related issues, and include updated tests or examples when applicable. For
user-visible example or documentation changes, include the exact command you
used to validate them.
