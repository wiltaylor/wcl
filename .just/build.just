set working-directory := '..'

# Build all Rust crates
rust:
    cargo build --workspace

# Build all Rust crates in release mode
rust-release:
    cargo build --workspace --release

# Build FFI static library for the host platform
ffi-native:
    cargo build -p wcl_ffi --release

# Build FFI for Linux x86_64
ffi-linux-amd64:
    #!/usr/bin/env bash
    if [[ "$(uname -s)-$(uname -m)" == "Linux-x86_64" ]]; then
        cargo build -p wcl_ffi --release
    else
        cargo zigbuild -p wcl_ffi --release --target x86_64-unknown-linux-gnu
    fi

# Build FFI for Linux arm64
ffi-linux-arm64:
    #!/usr/bin/env bash
    if [[ "$(uname -s)-$(uname -m)" == "Linux-aarch64" ]]; then
        cargo build -p wcl_ffi --release
    else
        cargo zigbuild -p wcl_ffi --release --target aarch64-unknown-linux-gnu
    fi

# Build FFI for macOS x86_64
ffi-darwin-amd64:
    #!/usr/bin/env bash
    if [[ "$(uname -s)-$(uname -m)" == "Darwin-x86_64" ]]; then
        cargo build -p wcl_ffi --release
    else
        cargo zigbuild -p wcl_ffi --release --target x86_64-apple-darwin
    fi

# Build FFI for macOS arm64
ffi-darwin-arm64:
    #!/usr/bin/env bash
    if [[ "$(uname -s)-$(uname -m)" == "Darwin-arm64" ]]; then
        cargo build -p wcl_ffi --release
    else
        cargo zigbuild -p wcl_ffi --release --target aarch64-apple-darwin
    fi

# Build FFI for Windows x86_64
ffi-windows-amd64:
    cargo zigbuild -p wcl_ffi --release --target x86_64-pc-windows-gnu

# Build FFI for all platforms
ffi-all: ffi-linux-amd64 ffi-linux-arm64 ffi-darwin-amd64 ffi-darwin-arm64 ffi-windows-amd64

# Build WASM module for Go (reuses WASI target)
go-wasm: dotnet-wasm
    mkdir -p bindings/go/wasm
    cp target/wasm32-wasip1/release/wcl_wasm.wasm bindings/go/wasm/

# Build Go bindings
go: go-wasm

# Build WASM module for Python (reuses WASI target)
python-wasm: dotnet-wasm
    mkdir -p bindings/python/python/wcl
    cp target/wasm32-wasip1/release/wcl_wasm.wasm bindings/python/python/wcl/

# Build Python bindings
python: python-wasm
    cd bindings/python && uv build

# Build WASM package
wasm:
    cd bindings/wasm && wasm-pack build --target web

# Build WASM module for .NET (WASI target)
dotnet-wasm:
    cargo build --target wasm32-wasip1 -p wcl_wasm --no-default-features --features wasi --release
    mkdir -p bindings/dotnet/src/Wcl/Wasm
    cp target/wasm32-wasip1/release/wcl_wasm.wasm bindings/dotnet/src/Wcl/Wasm/

# Build .NET library
dotnet: dotnet-wasm
    dotnet build bindings/dotnet/Wcl.sln -c Release

# Build Zig bindings (native platform)
zig: ffi-native
    #!/usr/bin/env bash
    os="$(uname -s | tr '[:upper:]' '[:lower:]')"
    arch="$(uname -m)"
    [[ "$arch" == "x86_64" ]] && arch="amd64"
    [[ "$arch" == "aarch64" ]] && arch="arm64"
    dir="bindings/zig/lib/${os}_${arch}"
    mkdir -p "$dir"
    cp target/release/libwcl_ffi.a "$dir/"
    cp crates/wcl_ffi/wcl.h bindings/zig/

# Build Zig bindings for all platforms
zig-all: ffi-all
    mkdir -p bindings/zig/lib/linux_amd64 bindings/zig/lib/linux_arm64 bindings/zig/lib/darwin_amd64 bindings/zig/lib/darwin_arm64 bindings/zig/lib/windows_amd64
    cp target/release/libwcl_ffi.a bindings/zig/lib/linux_amd64/ 2>/dev/null || true
    cp target/x86_64-unknown-linux-gnu/release/libwcl_ffi.a bindings/zig/lib/linux_amd64/ 2>/dev/null || true
    cp target/aarch64-unknown-linux-gnu/release/libwcl_ffi.a bindings/zig/lib/linux_arm64/
    cp target/x86_64-apple-darwin/release/libwcl_ffi.a bindings/zig/lib/darwin_amd64/
    cp target/aarch64-apple-darwin/release/libwcl_ffi.a bindings/zig/lib/darwin_arm64/
    cp target/x86_64-pc-windows-gnu/release/libwcl_ffi.a bindings/zig/lib/windows_amd64/wcl_ffi.lib
    cp crates/wcl_ffi/wcl.h bindings/zig/

# Regenerate VS Code tmLanguage.json from canonical sublime-syntax
vscode-syntax:
    uv run extras/sublime-syntax/generate-tmlanguage.py > editors/vscode/syntaxes/wcl.tmLanguage.json

# Package VS Code extension
vscode: vscode-syntax
    cd editors/vscode && npm install && npx --yes @vscode/vsce package --allow-missing-repository

# Build WASM module for JVM (reuses WASI target)
jvm-wasm: dotnet-wasm
    mkdir -p bindings/jvm/src/main/resources/io/github/wiltaylor/wcl/wasm
    cp target/wasm32-wasip1/release/wcl_wasm.wasm bindings/jvm/src/main/resources/io/github/wiltaylor/wcl/wasm/

# Build JVM library
jvm: jvm-wasm
    cd bindings/jvm && ./gradlew build -x test

# Build everything (native FFI)
all: rust-release ffi-native go zig python wasm dotnet jvm vscode

# Build everything with all FFI platforms
all-platforms: rust-release ffi-all go zig-all python wasm dotnet jvm vscode

# Publish Rust crates in dependency order
rust-publish:
    cargo publish -p wcl_core
    cargo publish -p wcl_derive
    cargo publish -p wcl_serde
    cargo publish -p wcl_eval
    cargo publish -p wcl_schema
    cargo publish -p wcl
    cargo publish -p wcl_ffi
    cargo publish -p wcl_lsp
    cargo publish -p wcl_cli

# Publish Python package
python-publish: python-wasm
    cd bindings/python && uv publish

# Publish WASM package to npm
wasm-publish:
    cd bindings/wasm && wasm-pack build --target web && npm publish

# Pack .NET NuGet package
dotnet-pack: dotnet-wasm
    dotnet pack bindings/dotnet/Wcl.sln -c Release

# Publish .NET NuGet package
dotnet-publish: dotnet-pack
    dotnet nuget push bindings/dotnet/src/Wcl/bin/Release/*.nupkg --source nuget.org

# Publish JVM package to Maven Central
jvm-publish: jvm
    cd bindings/jvm && ./gradlew publishToMavenCentral

# Publish VS Code extension
vscode-publish:
    cd editors/vscode && npx --yes @vscode/vsce publish
