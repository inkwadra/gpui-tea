set shell := ["bash", "-euo", "pipefail", "-c"]

default: help

help:
    @printf '%s\n' \
      'Available recipes:' \
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

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all --check

check:
    cargo check --all-targets --all-features

clippy:
    cargo clippy --all-targets --all-features -- -D warnings

doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features

test:
    cargo test --all-targets --all-features

typos:
    typos

lint:
    just clippy
    just typos

qa:
    just fmt-check
    just check
    just lint
    just doc
    just test

fix:
    cargo fmt --all
    cargo clippy --fix --all-targets --all-features --allow-dirty --allow-staged -- -D warnings

clean:
    cargo clean
