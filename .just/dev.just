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

# Build and install the CLI locally
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

# Install the VS Code extension via symlink
install-vscode:
    cd editors/vscode && npm install
    ln -sfn "$(pwd)/editors/vscode" "${HOME}/.vscode/extensions/wil.wcl-0.1.0"
    @echo "VS Code extension installed. Restart VS Code to activate."

# Uninstall the VS Code extension
uninstall-vscode:
    rm -f "${HOME}/.vscode/extensions/wil.wcl-0.1.0"
    @echo "VS Code extension removed. Restart VS Code."

# Clean build artifacts
clean:
    cargo clean
    dotnet clean bindings/dotnet/Wcl.sln -q 2>/dev/null || true
