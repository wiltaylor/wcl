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

# Uninstall the CLI
uninstall:
    cargo uninstall wcl_cli

# Run the CLI
run *args:
    cargo run --bin wcl -- {{args}}

# Validate a WCL file
validate file:
    cargo run --bin wcl -- validate {{file}}

# Convert a WCL file to JSON
to-json file:
    cargo run --bin wcl -- convert {{file}} --to json

# Install the VS Code extension via symlink (requires wcl to be installed)
install-vscode:
    cd editors/vscode && npm install
    ln -sfn "$(pwd)/editors/vscode" "${HOME}/.vscode/extensions/wil.wcl-0.1.0"
    @echo "VS Code extension installed. Restart VS Code to activate."

# Uninstall the VS Code extension
uninstall-vscode:
    rm -f "${HOME}/.vscode/extensions/wil.wcl-0.1.0"
    @echo "VS Code extension removed. Restart VS Code."

# Serve the documentation book locally
docs-serve:
    mdbook serve docs

# Build the documentation book
docs-build:
    mdbook build docs

# Run Python binding tests
test-python:
    cd wcl_python && source .venv/bin/activate && maturin develop && pytest tests/ -v

# Build the WASM package
build-wasm:
    cd wcl_wasm && wasm-pack build --target bundler

# Run WASM binding tests
test-wasm:
    cd wcl_wasm && wasm-pack test --node

# Run .NET binding tests
test-dotnet:
    dotnet test wcl_dotnet/Wcl.sln

# Build the .NET library
build-dotnet:
    dotnet build wcl_dotnet/Wcl.sln

# Clean build artifacts
clean:
    cargo clean
    dotnet clean wcl_dotnet/Wcl.sln -q 2>/dev/null || true

# Full CI check: fmt, lint, test
ci: fmt-check lint test test-python test-dotnet test-wasm
