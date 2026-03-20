set working-directory := '..'

# Run all Rust tests
rust:
    cargo test --workspace

# Run tests for a specific crate
crate name:
    cargo test -p {{name}}

# Run Python binding tests
python:
    cd bindings/python && uv run --group dev maturin develop && uv run --group dev python -m pytest tests/ -v

# Run WASM binding tests
wasm:
    cd bindings/wasm && wasm-pack test --node

# Run .NET binding tests
dotnet:
    just build dotnet-native
    dotnet test bindings/dotnet/Wcl.sln

# Run Go binding tests (builds FFI first)
go:
    just build go
    cd bindings/go && CGO_ENABLED=1 go test -v ./...

# Run tree-sitter grammar and binding tests
tree-sitter-wcl:
    #!/bin/bash
    set -euo pipefail
    cd extras/tree-sitter-wcl
    tree-sitter generate
    tree-sitter test
    cargo test
    go test ./bindings/go/

# Run all tests
all: rust python wasm dotnet go tree-sitter-wcl
