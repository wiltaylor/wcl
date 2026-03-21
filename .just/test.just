set working-directory := '..'

# Run all Rust tests
rust:
    cargo test --workspace

# Run tests for a specific crate
crate name:
    cargo test -p {{name}}

# Run Python binding tests
python:
    cd bindings/python && uv run pytest tests/ -v

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

# Run Zig binding tests (builds FFI first)
zig:
    just build zig
    cd bindings/zig && zig build test

# Run tree-sitter grammar and binding tests
tree-sitter-wcl:
    #!/bin/bash
    set -euo pipefail
    cd extras/tree-sitter-wcl
    tree-sitter generate
    tree-sitter test
    cargo test
    go test ./bindings/go/
    cd bindings/zig && zig build test

# Run JVM binding tests
jvm:
    just build jvm-wasm
    cd bindings/jvm && ./gradlew test

# Run all tests
all: rust python wasm dotnet go zig jvm tree-sitter-wcl
