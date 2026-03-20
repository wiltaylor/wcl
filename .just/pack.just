set working-directory := '..'

pkg := absolute_path('.packages')

# Package all Rust crates into .packages/rust/
rust:
    #!/bin/bash
    set -euo pipefail
    mkdir -p "{{pkg}}/rust"
    cargo package --workspace --allow-dirty --no-verify \
        --exclude wcl_python --exclude wcl_wasm
    cp target/package/*.crate "{{pkg}}/rust/"
    echo "Rust crates packaged to {{pkg}}/rust/"

# Package Python wheel into .packages/python/
python:
    #!/bin/bash
    set -euo pipefail
    mkdir -p "{{pkg}}/python"
    cd bindings/python
    uv run --group dev maturin build --release --out "{{pkg}}/python"
    echo "Python wheel packaged to {{pkg}}/python/"

# Package WASM npm tarball into .packages/wasm/
wasm:
    #!/bin/bash
    set -euo pipefail
    mkdir -p "{{pkg}}/wasm"
    cd bindings/wasm
    wasm-pack build --target web
    cd pkg
    npm pack --pack-destination "{{pkg}}/wasm"
    echo "WASM package packaged to {{pkg}}/wasm/"

# Package Go module source into .packages/go/
go:
    #!/bin/bash
    set -euo pipefail
    just build go
    mkdir -p "{{pkg}}/go"
    tar czf "{{pkg}}/go/wcl-go.tar.gz" \
        -C bindings/go \
        --exclude='*.tar.gz' \
        .
    echo "Go module packaged to {{pkg}}/go/"

# Package .NET NuGet package into .packages/dotnet/
dotnet:
    #!/bin/bash
    set -euo pipefail
    just build dotnet-native
    mkdir -p "{{pkg}}/dotnet"
    dotnet pack bindings/dotnet/src/Wcl/Wcl.csproj -c Release
    cp bindings/dotnet/src/Wcl/bin/Release/*.nupkg "{{pkg}}/dotnet/"
    echo ".NET package packaged to {{pkg}}/dotnet/"

# Package VS Code extension into .packages/vscode/
vscode:
    #!/bin/bash
    set -euo pipefail
    mkdir -p "{{pkg}}/vscode"
    cd editors/vscode
    npm install
    npx --yes @vscode/vsce package --allow-missing-repository
    cp *.vsix "{{pkg}}/vscode/"
    echo "VS Code extension packaged to {{pkg}}/vscode/"

# Package tree-sitter-wcl npm tarball into .packages/tree-sitter-wcl/
tree-sitter-wcl:
    #!/bin/bash
    set -euo pipefail
    mkdir -p "{{pkg}}/tree-sitter-wcl"
    cd extras/tree-sitter-wcl
    tree-sitter generate
    npm pack --pack-destination "{{pkg}}/tree-sitter-wcl"
    echo "tree-sitter-wcl packaged to {{pkg}}/tree-sitter-wcl/"

# Package everything. Pass a version to stamp all packages (default: 0.0.0-local)
all ver="0.0.0-local": (set-version ver) rust python wasm go dotnet vscode tree-sitter-wcl
    @echo "All packages created in {{pkg}}/"

# Remove all packaged artifacts
clean:
    rm -rf "{{pkg}}"
    @echo "Cleaned {{pkg}}/"

[private]
set-version ver:
    #!/bin/bash
    set -euo pipefail
    V="{{ver}}"
    echo "Setting version to: $V"

    # Rust: workspace version + workspace dependency versions
    sed -i "0,/^version = .*/s/^version = .*/version = \"$V\"/" Cargo.toml
    for crate in wcl wcl_core wcl_eval wcl_schema wcl_serde wcl_derive wcl_lsp; do
        sed -i "s|$crate = { path = \"crates/$crate\", version = \"[^\"]*\" }|$crate = { path = \"crates/$crate\", version = \"$V\" }|" Cargo.toml
    done

    # Python: maturin reads version from Cargo.toml (must be semver that maturin
    # can convert to PEP 440). Map our prerelease tags to maturin-compatible semver.
    # 0.0.0-local → 0.0.0-dev.0, 0.0.0-ci+sha → 0.0.0-dev.0, 1.0.0-rc1 → 1.0.0-rc.1
    PY_SEMVER="$V"
    if [[ "$V" =~ ^([0-9]+\.[0-9]+\.[0-9]+)-(.+)$ ]]; then
        BASE="${BASH_REMATCH[1]}"
        PRE="${BASH_REMATCH[2]}"
        case "$PRE" in
            local|ci+*) PY_SEMVER="${BASE}-dev.0" ;;
            rc*)        NUM="${PRE//[^0-9]/}"; PY_SEMVER="${BASE}-rc.${NUM:-1}" ;;
            alpha*)     NUM="${PRE//[^0-9]/}"; PY_SEMVER="${BASE}-alpha.${NUM:-1}" ;;
            beta*)      NUM="${PRE//[^0-9]/}"; PY_SEMVER="${BASE}-beta.${NUM:-1}" ;;
            *)          PY_SEMVER="${BASE}-dev.0" ;;
        esac
    fi
    sed -i "s/^version = .*/version = \"$PY_SEMVER\"/" bindings/python/Cargo.toml
    sed -i "s/^version = .*/version = \"$PY_SEMVER\"/" bindings/python/pyproject.toml

    # npm (WASM + VS Code + tree-sitter-wcl): semver with - prerelease
    sed -i "s/\"version\": \"[^\"]*\"/\"version\": \"$V\"/" bindings/wasm/package.json
    sed -i "s/\"version\": \"[^\"]*\"/\"version\": \"$V\"/" editors/vscode/package.json
    sed -i "s/\"version\": \"[^\"]*\"/\"version\": \"$V\"/" extras/tree-sitter-wcl/package.json

    # .NET: supports semver with - prerelease
    sed -i "s|<Version>[^<]*</Version>|<Version>$V</Version>|" bindings/dotnet/src/Wcl/Wcl.csproj

    # tree-sitter-wcl: Cargo.toml (Rust crate), pyproject.toml (Python), tree-sitter.json
    sed -i "0,/^version = .*/s/^version = .*/version = \"$V\"/" extras/tree-sitter-wcl/Cargo.toml
    sed -i "s/^version = .*/version = \"$V\"/" extras/tree-sitter-wcl/pyproject.toml
    sed -i "s/\"version\": \"[^\"]*\"/\"version\": \"$V\"/" extras/tree-sitter-wcl/tree-sitter.json

    echo "All packages set to $V"
