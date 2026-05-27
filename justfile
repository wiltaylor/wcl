set unstable
set shell := ["bash", "-cu"]

[default, private]
main:
    @just --list

# Build the workspace
[group('build')]
workspace-build:
    cargo build --workspace

# Build the VS Code extension (npm install + tsc compile in editors/vscode)
[group('build')]
vscode-build:
    cd editors/vscode && npm install && npm run compile

# Package the VS Code extension as a .vsix (uses @vscode/vsce via npx)
[group('build')]
vscode-package: vscode-build
    cd editors/vscode && npx --yes @vscode/vsce package

# Install the freshly-built .vsix into VS Code (requires `code` on PATH)
[group('build')]
vscode-install: vscode-package
    cd editors/vscode && code --install-extension "$(ls -t wcl-vscode-*.vsix | head -1)" --force

# Install the wcl CLI to ~/.cargo/bin (cargo install --locked)
[group('build')]
cli-install:
    cargo install --path crates/wcl --locked

# Run all tests (unit + integration)
[group('test')]
workspace-test:
    cargo test --workspace

# Run wdoc tests only (unit + integration in crates/wcl_wdoc)
[group('test')]
wdoc-test:
    cargo test -p wcl_wdoc

# Run one cargo-fuzz target (nightly + cargo-fuzz required); pass extra flags after --
[group('test')]
fuzz-run TARGET *ARGS:
    cd crates/wcl_lang && cargo +nightly fuzz run {{TARGET}} {{ARGS}}

# Bounded fuzz sweep across every target (~15s each, ~75s total)
[group('test')]
fuzz-sweep:
    @for t in parse eval format_round_trip json_round_trip set_edit_path; do \
        echo "==> fuzz $t" >&2; \
        (cd crates/wcl_lang && cargo +nightly fuzz run "$t" -- -runs=2000 -max_total_time=15) || exit 1; \
    done

# Format all code
[group('quality')]
workspace-fmt:
    cargo fmt --all

# Lint with clippy (warnings are errors)
[group('quality')]
workspace-lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Full CI gate: fmt-check + workspace-lint + workspace-test
[group('quality')]
ci: fmt-check workspace-lint workspace-test

# Run the CLI: just cli-run -- parse examples/basic.wcl
[group('dev')]
cli-run *ARGS:
    cargo run -p wcl -- {{ARGS}}

# Serve examples/wdoc/main.wcl with `wcl wdoc serve`; pass extra flags after --
# (the example declares three sites — / is the chooser; --site picks one)
[group('dev')]
wdoc-serve *ARGS:
    cargo run -p wcl -- wdoc serve examples/wdoc/main.wcl {{ARGS}}

# Run criterion benchmarks
[group('dev')]
workspace-bench:
    cargo bench -p wcl_lang

# Profile each example with `wcl parse --profile`; JSON profile per file to stderr
[group('dev')]
examples-profile:
    @for f in examples/*.wcl; do \
        echo "==> $f" >&2; \
        cargo run -q -p wcl -- parse --profile "$f" >/dev/null; \
        echo >&2; \
    done

[private]
fmt-check:
    cargo fmt --all -- --check
