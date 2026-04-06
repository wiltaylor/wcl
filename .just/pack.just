set working-directory := '..'

pkg := absolute_path('.packages')

# Package all Rust crates into .packages/rust/
rust:
    #!/bin/bash
    set -euo pipefail
    mkdir -p "{{pkg}}/rust"
    cargo package --workspace --allow-dirty --no-verify \
        --exclude wcl_wasm
    cp target/package/*.crate "{{pkg}}/rust/"
    echo "Rust crates packaged to {{pkg}}/rust/"

# Package Python wheel into .packages/python/
python:
    #!/bin/bash
    set -euo pipefail
    just build python-wasm
    mkdir -p "{{pkg}}/python"
    cd bindings/python
    uv build --out-dir "{{pkg}}/python"
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
    just build dotnet-wasm
    mkdir -p "{{pkg}}/dotnet"
    dotnet pack bindings/dotnet/src/Wcl/Wcl.csproj -c Release
    cp bindings/dotnet/src/Wcl/bin/Release/*.nupkg "{{pkg}}/dotnet/"
    echo ".NET package packaged to {{pkg}}/dotnet/"

# Package VS Code extension for a single platform (skips if binary not found)
vscode-platform target vsce_target:
    #!/bin/bash
    set -euo pipefail
    bin_src=""
    if [[ "{{vsce_target}}" == win32-* ]]; then
        # Try both MSVC and GNU Windows targets
        for t in "{{target}}" "$(echo '{{target}}' | sed -E 's/-gnu$/-msvc/; t; s/-msvc$/-gnu/')"; do
            if [[ -f "target/${t}/release/wcl.exe" ]]; then
                bin_src="target/${t}/release/wcl.exe"
                break
            fi
        done
    elif [[ -f "target/{{target}}/release/wcl" ]]; then
        bin_src="target/{{target}}/release/wcl"
    elif [[ -f "target/release/wcl" ]]; then
        bin_src="target/release/wcl"
    fi
    if [[ -z "$bin_src" || ! -f "$bin_src" ]]; then
        echo "Skipping VS Code extension ({{vsce_target}}): no CLI binary at target/{{target}}/release/"
        exit 0
    fi
    mkdir -p "{{pkg}}/vscode"
    mkdir -p editors/vscode/bin
    cp "$bin_src" editors/vscode/bin/
    if [[ "{{vsce_target}}" != win32-* ]]; then
        chmod +x editors/vscode/bin/wcl
    fi
    cd editors/vscode
    npm install
    npx --yes @vscode/vsce package --target "{{vsce_target}}" --allow-missing-repository
    cp *.vsix "{{pkg}}/vscode/"
    rm -rf bin *.vsix
    echo "VS Code extension ({{vsce_target}}) packaged to {{pkg}}/vscode/"

# Package VS Code extension for all platforms (requires cli-all to be run first)
vscode: (vscode-platform "x86_64-unknown-linux-gnu" "linux-x64") (vscode-platform "aarch64-unknown-linux-gnu" "linux-arm64") (vscode-platform "x86_64-apple-darwin" "darwin-x64") (vscode-platform "aarch64-apple-darwin" "darwin-arm64") (vscode-platform "x86_64-pc-windows-msvc" "win32-x64")

# Package tree-sitter-wcl into .packages/tree-sitter-wcl/ (npm + Rust crate)
tree-sitter-wcl:
    #!/bin/bash
    set -euo pipefail
    mkdir -p "{{pkg}}/tree-sitter-wcl"
    cd extras/tree-sitter-wcl
    tree-sitter generate
    npm pack --pack-destination "{{pkg}}/tree-sitter-wcl"
    cargo package --allow-dirty --no-verify
    cp target/package/*.crate "{{pkg}}/tree-sitter-wcl/"
    echo "tree-sitter-wcl packaged to {{pkg}}/tree-sitter-wcl/"

# Package tree-sitter highlight queries into .packages/highlight-queries/
highlight-queries:
    #!/bin/bash
    set -euo pipefail
    mkdir -p "{{pkg}}/highlight-queries"
    tar czf "{{pkg}}/highlight-queries/wcl-highlight-queries.tar.gz" \
        -C extras/tree-sitter-wcl/queries highlights.scm \
        -C ../../highlight-queries injections.scm locals.scm textobjects.scm
    echo "Highlight queries packaged to {{pkg}}/highlight-queries/"

# Package Sublime Text syntax into .packages/sublime-syntax/
sublime-syntax:
    #!/bin/bash
    set -euo pipefail
    mkdir -p "{{pkg}}/sublime-syntax"
    tar czf "{{pkg}}/sublime-syntax/wcl-sublime-syntax.tar.gz" \
        -C extras/sublime-syntax WCL.sublime-syntax
    echo "Sublime syntax packaged to {{pkg}}/sublime-syntax/"

# Package highlight.js grammar into .packages/highlightjs/
highlightjs:
    #!/bin/bash
    set -euo pipefail
    mkdir -p "{{pkg}}/highlightjs"
    tar czf "{{pkg}}/highlightjs/wcl-highlightjs.tar.gz" \
        -C extras/highlightjs wcl.js
    echo "highlight.js grammar packaged to {{pkg}}/highlightjs/"

# Package Chroma XML lexer into .packages/chroma/
chroma:
    #!/bin/bash
    set -euo pipefail
    mkdir -p "{{pkg}}/chroma"
    tar czf "{{pkg}}/chroma/wcl-chroma.tar.gz" \
        -C extras/chroma wcl.xml
    echo "Chroma lexer packaged to {{pkg}}/chroma/"

# Package Pygments lexer into .packages/pygments/
pygments:
    #!/bin/bash
    set -euo pipefail
    mkdir -p "{{pkg}}/pygments"
    tar czf "{{pkg}}/pygments/wcl-pygments.tar.gz" \
        -C extras/pygments wcl_lexer.py
    echo "Pygments lexer packaged to {{pkg}}/pygments/"

# Package JVM JAR into .packages/jvm/
jvm:
    #!/bin/bash
    set -euo pipefail
    just build jvm-wasm
    mkdir -p "{{pkg}}/jvm"
    bindings/jvm/gradlew -p bindings/jvm jar
    cp bindings/jvm/build/libs/*.jar "{{pkg}}/jvm/"
    echo "JVM JAR packaged to {{pkg}}/jvm/"

# Package C/C++ archive (header + libs + CMake) into .packages/ffi/
ffi:
    #!/bin/bash
    set -euo pipefail
    just build ffi-native
    stage="{{pkg}}/ffi/wcl-ffi"
    rm -rf "$stage"
    mkdir -p "$stage/include" "$stage/lib"

    # Header
    cp crates/wcl_ffi/wcl.h "$stage/include/"

    # CMakeLists.txt
    cp crates/wcl_ffi/CMakeLists.txt "$stage/"

    # Copy prebuilt libraries per platform (skip missing cross targets)
    copy_lib() {
        local platform="$1" src="$2" name="$3"
        if [[ -f "$src" ]]; then
            mkdir -p "$stage/lib/$platform"
            cp "$src" "$stage/lib/$platform/$name"
            echo "  $platform"
        fi
    }

    echo "Included platforms:"
    # Native build always available
    copy_lib linux_amd64   target/release/libwcl_ffi.a                              libwcl_ffi.a
    # Cross-compiled targets (from ffi-all)
    copy_lib linux_amd64   target/x86_64-unknown-linux-gnu/release/libwcl_ffi.a     libwcl_ffi.a
    copy_lib linux_arm64   target/aarch64-unknown-linux-gnu/release/libwcl_ffi.a    libwcl_ffi.a
    copy_lib darwin_amd64  target/x86_64-apple-darwin/release/libwcl_ffi.a          libwcl_ffi.a
    copy_lib darwin_arm64  target/aarch64-apple-darwin/release/libwcl_ffi.a         libwcl_ffi.a
    copy_lib windows_amd64 target/x86_64-pc-windows-gnu/release/libwcl_ffi.a       wcl_ffi.lib

    # Also check Zig binding layout (may have been built there)
    for plat in linux_amd64 linux_arm64 darwin_amd64 darwin_arm64; do
        src="bindings/zig/lib/$plat/libwcl_ffi.a"
        if [[ -f "$src" && ! -d "$stage/lib/$plat" ]]; then
            copy_lib "$plat" "$src" libwcl_ffi.a
        fi
    done
    if [[ -f "bindings/zig/lib/windows_amd64/wcl_ffi.lib" && ! -d "$stage/lib/windows_amd64" ]]; then
        copy_lib windows_amd64 "bindings/zig/lib/windows_amd64/wcl_ffi.lib" wcl_ffi.lib
    fi

    tar czf "{{pkg}}/ffi/wcl-ffi.tar.gz" -C "{{pkg}}/ffi" wcl-ffi
    rm -rf "$stage"
    echo "C/C++ archive packaged to {{pkg}}/ffi/wcl-ffi.tar.gz"

# Package Ruby gem into .packages/ruby/
ruby:
    #!/bin/bash
    set -euo pipefail
    just build ruby-wasm
    mkdir -p "{{pkg}}/ruby"
    cd bindings/ruby && gem build wcl.gemspec -o "{{pkg}}/ruby/wcl.gem"
    echo "Ruby gem packaged to {{pkg}}/ruby/"

# Package JetBrains plugin for a single platform (skips if binary not found)
jetbrains-platform target platform_name:
    #!/bin/bash
    set -euo pipefail
    bin_src=""
    if [[ "{{platform_name}}" == windows-* ]]; then
        # Try both MSVC and GNU Windows targets
        for t in "{{target}}" "$(echo '{{target}}' | sed -E 's/-gnu$/-msvc/; t; s/-msvc$/-gnu/')"; do
            if [[ -f "target/${t}/release/wcl.exe" ]]; then
                bin_src="target/${t}/release/wcl.exe"
                break
            fi
        done
    elif [[ -f "target/{{target}}/release/wcl" ]]; then
        bin_src="target/{{target}}/release/wcl"
    elif [[ -f "target/release/wcl" ]]; then
        bin_src="target/release/wcl"
    fi
    if [[ -z "$bin_src" || ! -f "$bin_src" ]]; then
        echo "Skipping JetBrains plugin ({{platform_name}}): no CLI binary at target/{{target}}/release/"
        exit 0
    fi
    mkdir -p "{{pkg}}/jetbrains"
    mkdir -p editors/jetbrains/bin
    cp "$bin_src" editors/jetbrains/bin/
    if [[ "{{platform_name}}" != windows-* ]]; then
        chmod +x editors/jetbrains/bin/wcl
    fi
    cd editors/jetbrains
    ./gradlew buildPlugin
    for f in build/distributions/*.zip; do
        base="$(basename "$f" .zip)"
        cp "$f" "{{pkg}}/jetbrains/${base}-{{platform_name}}.zip"
    done
    rm -rf bin build/distributions/*.zip
    echo "JetBrains plugin ({{platform_name}}) packaged to {{pkg}}/jetbrains/"

# Package JetBrains plugin for all platforms (requires cli-all to be run first)
jetbrains: (jetbrains-platform "x86_64-unknown-linux-gnu" "linux-x64") (jetbrains-platform "aarch64-unknown-linux-gnu" "linux-arm64") (jetbrains-platform "x86_64-apple-darwin" "darwin-x64") (jetbrains-platform "aarch64-apple-darwin" "darwin-arm64") (jetbrains-platform "x86_64-pc-windows-msvc" "windows-x64")

# Package everything. Pass a version to stamp all packages (default: 0.0.0-local)
all ver="0.0.0-local": (set-version ver) rust python wasm go dotnet jvm ruby ffi vscode jetbrains tree-sitter-wcl highlight-queries sublime-syntax highlightjs chroma pygments
    @echo "All packages created in {{pkg}}/"

# Remove all packaged artifacts
clean:
    rm -rf "{{pkg}}"
    @echo "Cleaned {{pkg}}/"

# Stamp version across all packages
set-version ver:
    #!/bin/bash
    set -euo pipefail
    V="{{ver}}"
    echo "Setting version to: $V"

    # Rust: workspace version + workspace dependency versions
    sed -i "0,/^version = .*/s/^version = .*/version = \"$V\"/" Cargo.toml
    for crate in wcl wcl_lang wcl_lsp wcl_wdoc; do
        sed -i "s|$crate = { path = \"crates/$crate\", version = \"[^\"]*\" }|$crate = { path = \"crates/$crate\", version = \"$V\" }|" Cargo.toml
    done

    # Python: map prerelease tags to PEP 440-compatible versions.
    # 0.0.0-local → 0.0.0.dev0, 0.0.0-ci+sha → 0.0.0.dev0, 1.0.0-rc1 → 1.0.0rc1
    PY_VER="$V"
    if [[ "$V" =~ ^([0-9]+\.[0-9]+\.[0-9]+)-(.+)$ ]]; then
        BASE="${BASH_REMATCH[1]}"
        PRE="${BASH_REMATCH[2]}"
        case "$PRE" in
            local|ci+*) PY_VER="${BASE}.dev0" ;;
            rc*)        NUM="${PRE//[^0-9]/}"; PY_VER="${BASE}rc${NUM:-1}" ;;
            alpha*)     NUM="${PRE//[^0-9]/}"; PY_VER="${BASE}a${NUM:-1}" ;;
            beta*)      NUM="${PRE//[^0-9]/}"; PY_VER="${BASE}b${NUM:-1}" ;;
            *)          PY_VER="${BASE}.dev0" ;;
        esac
    fi
    sed -i "s/^version = .*/version = \"$PY_VER\"/" bindings/python/pyproject.toml

    # npm (WASM + VS Code + tree-sitter-wcl): semver with - prerelease
    sed -i "s/\"version\": \"[^\"]*\"/\"version\": \"$V\"/" bindings/wasm/package.json
    sed -i "s/\"version\": \"[^\"]*\"/\"version\": \"$V\"/" editors/vscode/package.json
    sed -i "s/\"version\": \"[^\"]*\"/\"version\": \"$V\"/" extras/tree-sitter-wcl/package.json

    # .NET: supports semver with - prerelease
    sed -i "s|<Version>[^<]*</Version>|<Version>$V</Version>|" bindings/dotnet/src/Wcl/Wcl.csproj

    # JVM: Gradle version
    sed -i "s/^version = .*/version = '$V'/" bindings/jvm/build.gradle

    # Ruby: map prerelease tags to RubyGems-compatible versions.
    # RubyGems doesn't allow hyphens or + in versions.
    # 0.0.0-local → 0.0.0.dev0, 0.0.0-ci+sha → 0.0.0.dev0, 1.0.0-rc1 → 1.0.0.rc1
    RB_VER="$V"
    if [[ "$V" =~ ^([0-9]+\.[0-9]+\.[0-9]+)-(.+)$ ]]; then
        BASE="${BASH_REMATCH[1]}"
        PRE="${BASH_REMATCH[2]}"
        case "$PRE" in
            local|ci+*) RB_VER="${BASE}.dev0" ;;
            rc*)        NUM="${PRE//[^0-9]/}"; RB_VER="${BASE}.rc${NUM:-1}" ;;
            alpha*)     NUM="${PRE//[^0-9]/}"; RB_VER="${BASE}.alpha${NUM:-1}" ;;
            beta*)      NUM="${PRE//[^0-9]/}"; RB_VER="${BASE}.beta${NUM:-1}" ;;
            *)          RB_VER="${BASE}.dev0" ;;
        esac
    fi
    sed -i "s/VERSION = \"[^\"]*\"/VERSION = \"$RB_VER\"/" bindings/ruby/lib/wcl/version.rb

    # C/C++ CMake: project version (CMake only allows digits and dots)
    CMAKE_VER=$(echo "$V" | sed 's/-.*//')
    sed -i "s/project(wcl VERSION [^ ]*/project(wcl VERSION $CMAKE_VER/" crates/wcl_ffi/CMakeLists.txt

    # JetBrains plugin: gradle.properties
    sed -i "s/pluginVersion=.*/pluginVersion=$V/" editors/jetbrains/gradle.properties

    # tree-sitter-wcl: Cargo.toml (Rust crate), pyproject.toml (Python), tree-sitter.json
    sed -i "0,/^version = .*/s/^version = .*/version = \"$V\"/" extras/tree-sitter-wcl/Cargo.toml
    sed -i "s/^version = .*/version = \"$V\"/" extras/tree-sitter-wcl/pyproject.toml
    sed -i "s/\"version\": \"[^\"]*\"/\"version\": \"$V\"/" extras/tree-sitter-wcl/tree-sitter.json

    echo "All packages set to $V"
