set working-directory := '..'

# Full CI pipeline: fmt, lint, all tests
all:
    cargo fmt --all -- --check
    cargo clippy --workspace -- -D warnings
    cargo test --workspace
    cd bindings/python && source .venv/bin/activate && maturin develop && pytest tests/ -v
    cd bindings/wasm && wasm-pack test --node
    dotnet test bindings/dotnet/Wcl.sln
    just build go
    cd bindings/go && CGO_ENABLED=1 go test -v ./...

# Quick CI: Rust only (fmt, lint, test)
quick:
    cargo fmt --all -- --check
    cargo clippy --workspace -- -D warnings
    cargo test --workspace
