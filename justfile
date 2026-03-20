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
    cargo install --path crates/wcl_cli

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
    mdbook serve docs/book

# Build the documentation book
docs-build:
    mdbook build docs/book

# Run Python binding tests
test-python:
    cd bindings/python && source .venv/bin/activate && maturin develop && pytest tests/ -v

# Build the WASM package
build-wasm:
    cd bindings/wasm && wasm-pack build --target web

# Run WASM binding tests
test-wasm:
    cd bindings/wasm && wasm-pack test --node

# Run .NET binding tests
test-dotnet:
    dotnet test bindings/dotnet/Wcl.sln

# Build the .NET library
build-dotnet:
    dotnet build bindings/dotnet/Wcl.sln

# Build the FFI static library (native)
build-ffi:
    cargo build -p wcl_ffi --release

# Build FFI static libraries for all platforms (requires cargo-zigbuild + zig)
build-ffi-all: build-ffi
    cargo zigbuild -p wcl_ffi --release --target aarch64-unknown-linux-gnu
    cargo zigbuild -p wcl_ffi --release --target aarch64-apple-darwin
    cargo zigbuild -p wcl_ffi --release --target x86_64-apple-darwin
    cargo zigbuild -p wcl_ffi --release --target x86_64-pc-windows-gnu

# Build the Go bindings (native platform only)
build-go: build-ffi
    mkdir -p bindings/go/lib/linux_amd64
    cp target/release/libwcl_ffi.a bindings/go/lib/linux_amd64/ 2>/dev/null || true
    cp crates/wcl_ffi/wcl.h bindings/go/

# Build Go bindings for all platforms
build-go-all: build-ffi-all
    mkdir -p bindings/go/lib/linux_amd64 bindings/go/lib/linux_arm64 bindings/go/lib/darwin_amd64 bindings/go/lib/darwin_arm64 bindings/go/lib/windows_amd64
    cp target/release/libwcl_ffi.a bindings/go/lib/linux_amd64/
    cp target/aarch64-unknown-linux-gnu/release/libwcl_ffi.a bindings/go/lib/linux_arm64/
    cp target/x86_64-apple-darwin/release/libwcl_ffi.a bindings/go/lib/darwin_amd64/
    cp target/aarch64-apple-darwin/release/libwcl_ffi.a bindings/go/lib/darwin_arm64/
    cp target/x86_64-pc-windows-gnu/release/libwcl_ffi.a bindings/go/lib/windows_amd64/wcl_ffi.lib
    cp crates/wcl_ffi/wcl.h bindings/go/

# Run Go binding tests
test-go: build-go
    cd bindings/go && CGO_ENABLED=1 go test -v ./...

# Start Hugo dev server for the website (with mdBook at /docs/)
dev-web: docs-build
    mkdir -p site/static/docs
    cp -r docs/book/build/* site/static/docs/
    cd site && hugo server --buildDrafts --navigateToChanged

# Build full website (Hugo + mdBook merged)
build-web: docs-build
    cd site && hugo --minify
    mkdir -p site/public/docs
    cp -r docs/book/build/* site/public/docs/

# Clean website build output
clean-web:
    rm -rf site/public site/resources

# Clean build artifacts
clean:
    cargo clean
    dotnet clean bindings/dotnet/Wcl.sln -q 2>/dev/null || true

# Full CI check: fmt, lint, test
ci: fmt-check lint test test-python test-dotnet test-wasm test-go
