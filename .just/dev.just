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

# Build and install the CLI locally, then install the wdoc stdlib so editors
# and the LSP can resolve `import <wdoc.wcl>`.
install:
    cargo install --path crates/wcl
    wcl wdoc install-library --force

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
