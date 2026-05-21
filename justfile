set unstable
set shell := ["bash", "-cu"]

[private]
default:
    @just --list

# Build the workspace
build:
    cargo build --workspace

# Run the CLI: just run -- parse examples/basic.wcl
run *ARGS:
    cargo run -p wcl -- {{ARGS}}

# Run all tests (unit + integration)
test:
    cargo test --workspace

# Run criterion benchmarks
bench:
    cargo bench -p wcl_lang

# Format all code
fmt:
    cargo fmt --all

# Lint with clippy (warnings are errors)
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Full CI gate: fmt-check + lint + test
ci: fmt-check lint test

[private]
fmt-check:
    cargo fmt --all -- --check
