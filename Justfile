set shell := ["bash", "-euo", "pipefail", "-c"]

default: help

help:
    @printf '%s\n' \
      'Available recipes:' \
      '  just metadata       - validate workspace metadata resolution' \
      '  just package        - inspect packaged file sets for publishable crates' \
      '  just fmt            - format the workspace' \
      '  just fmt-check      - verify formatting without changes' \
      '  just check          - compile all targets and features' \
      '  just clippy         - run Clippy with warnings denied' \
      '  just doc            - build docs with rustdoc warnings denied' \
      '  just test           - run the full test suite' \
      '  just typos          - run the typo checker' \
      '  just lint           - run clippy and typos' \
      '  just qa             - run the full pre-push quality gate' \
      '  just fix            - apply local formatting and Clippy fixes'

metadata:
    cargo metadata --no-deps --format-version 1 > /dev/null

package:
    cargo package -p gpui_tea --allow-dirty --list > /dev/null
    cargo package -p gpui_tea_macros --allow-dirty --list > /dev/null

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all --check

check:
    cargo check --workspace --all-targets --all-features

clippy:
    cargo clippy --workspace --all-targets --all-features -- -D clippy::all -D clippy::cargo -A clippy::multiple-crate-versions

doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features

test:
    cargo test --workspace --all-targets --all-features

typos:
    typos

lint:
    just clippy
    just typos

qa:
    just metadata
    just fmt-check
    just check
    just lint
    just doc
    just test

fix:
    cargo fmt --all
    cargo clippy --workspace --fix --all-targets --all-features --allow-dirty --allow-staged -- -D warnings

clean:
    cargo clean
