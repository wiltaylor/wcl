set working-directory := '..'

pkg := '.packages'

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
    uv run --group dev maturin build --release --out "../../{{pkg}}/python"
    echo "Python wheel packaged to {{pkg}}/python/"

# Package WASM npm tarball into .packages/wasm/
wasm:
    #!/bin/bash
    set -euo pipefail
    mkdir -p "{{pkg}}/wasm"
    cd bindings/wasm
    wasm-pack build --target web
    cd pkg
    npm pack --pack-destination "../../../{{pkg}}/wasm"
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
    cp *.vsix "../../{{pkg}}/vscode/"
    echo "VS Code extension packaged to {{pkg}}/vscode/"

# Package everything into .packages/
all: rust python wasm go dotnet vscode
    @echo "All packages created in {{pkg}}/"

# Remove all packaged artifacts
clean:
    rm -rf "{{pkg}}"
    @echo "Cleaned {{pkg}}/"
