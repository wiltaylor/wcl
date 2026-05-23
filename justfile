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

# Run a cargo-fuzz target (requires nightly + cargo-fuzz installed).
# Examples:
#   just fuzz parse
#   just fuzz eval -- -runs=10000
fuzz TARGET *ARGS:
    cd crates/wcl_lang && cargo +nightly fuzz run {{TARGET}} {{ARGS}}

# Short fuzz sweep across every target (~15s each, ~75s total). Bounded
# so it always terminates; exits nonzero on the first crash.
test-fuzz:
    @for t in parse eval format_round_trip json_round_trip set_edit_path; do \
        echo "==> fuzz $t" >&2; \
        (cd crates/wcl_lang && cargo +nightly fuzz run "$t" -- -runs=2000 -max_total_time=15) || exit 1; \
    done

# Profile each example with `wcl parse --profile`; JSON profile per file to stderr.
examples-profile:
    @for f in examples/*.wcl; do \
        echo "==> $f" >&2; \
        cargo run -q -p wcl -- parse --profile "$f" >/dev/null; \
        echo >&2; \
    done

[private]
fmt-check:
    cargo fmt --all -- --check
