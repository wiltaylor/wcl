set working-directory := '..'

# Run all Rust tests
rust:
    cargo test --workspace

# Run tests for a specific crate
crate name:
    cargo test -p {{name}}

# Run Python binding tests
python:
    cd bindings/python && source .venv/bin/activate && maturin develop && pytest tests/ -v

# Run WASM binding tests
wasm:
    cd bindings/wasm && wasm-pack test --node

# Run .NET binding tests
dotnet:
    dotnet test bindings/dotnet/Wcl.sln

# Run Go binding tests (builds FFI first)
go:
    just build go
    cd bindings/go && CGO_ENABLED=1 go test -v ./...

# Run all tests
all: rust python wasm dotnet go
