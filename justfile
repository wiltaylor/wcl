# WCL - Wil's Configuration Language

[private]
default:
    just --list

# Run all tests
test:
    cargo test --workspace

# Run tests for a specific crate
test-crate crate:
    cargo test -p {{crate}}

# Build all crates
build:
    cargo build --workspace

# Build in release mode
build-release:
    cargo build --workspace --release

# Check compilation without building
check:
    cargo check --workspace

# Run clippy lints
lint:
    cargo clippy --workspace -- -D warnings

# Format code
fmt:
    cargo fmt --all

# Check formatting without modifying
fmt-check:
    cargo fmt --all -- --check

# Run all benchmarks
bench:
    cargo bench --workspace

# Run benchmarks for a specific crate
bench-crate crate:
    cargo bench -p {{crate}}

# Build and install the CLI locally
install:
    cargo install --path wcl_cli

# Run the CLI
run *args:
    cargo run --bin wcl -- {{args}}

# Validate a WCL file
validate file:
    cargo run --bin wcl -- validate {{file}}

# Convert a WCL file to JSON
to-json file:
    cargo run --bin wcl -- convert {{file}} --to json

# Clean build artifacts
clean:
    cargo clean

# Full CI check: fmt, lint, test
ci: fmt-check lint test
