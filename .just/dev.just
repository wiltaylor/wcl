set working-directory := '..'

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

# Run WCL parse/eval microbenchmarks
bench-wcl:
    cargo bench -p wcl --bench e2e

# Run WDoc project/render benchmarks
bench-wdoc: bench-wdoc-load bench-wdoc-codec

# Run WDoc project loading benchmark
bench-wdoc-load:
    cargo bench -p wcl --bench wdoc "load example project"

# Run WDoc codec registry benchmark
bench-wdoc-codec:
    cargo bench -p wcl --bench wdoc "codec registry"

# Run the long WDoc render benchmark
bench-wdoc-render:
    cargo bench -p wcl --bench wdoc "render example project"

# Run all WDoc benchmarks, including long render/full-build measurements
bench-wdoc-all:
    cargo bench -p wcl --bench wdoc

# Profile the WDoc example build with phase timings
perf-wdoc:
    cargo build --quiet -p wcl
    WCL_PROFILE=1 ./target/debug/wcl wdoc build examples/wdoc/site.wcl --output /tmp/wcl-perf-wdoc

# Run the focused WCL performance checks
perf: perf-wdoc bench-wdoc

# Build and install the CLI locally.
install:
    cargo install --path crates/wcl

# Uninstall the CLI
uninstall:
    cargo uninstall wcl

# Run the CLI
run *args:
    cargo run --bin wcl -- {{args}}

# Validate a WCL file
validate file:
    cargo run --bin wcl -- validate {{file}}

# Convert a WCL file to JSON
to-json file:
    cargo run --bin wcl -- convert {{file}} --to json

# Install the VS Code extension via symlink.
# The target directory name must match `<publisher>.<name>-<version>` from
# package.json — otherwise VS Code silently ignores the extension.
install-vscode:
    #!/usr/bin/env bash
    set -euo pipefail
    cd editors/vscode && npm install
    cd ../..
    version=$(node -p "require('./editors/vscode/package.json').version")
    dir="${HOME}/.vscode/extensions/wil.wcl-${version}"
    ln -sfn "$(pwd)/editors/vscode" "${dir}"
    echo "VS Code extension installed at ${dir}. Restart VS Code to activate."

# Uninstall the VS Code extension
uninstall-vscode:
    #!/usr/bin/env bash
    set -euo pipefail
    version=$(node -p "require('./editors/vscode/package.json').version")
    rm -f "${HOME}/.vscode/extensions/wil.wcl-${version}"
    echo "VS Code extension removed. Restart VS Code."

# Clean build artifacts
clean:
    cargo clean
    dotnet clean bindings/dotnet/Wcl.sln -q 2>/dev/null || true
