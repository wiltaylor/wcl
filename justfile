# WCL - Wil's Configuration Language

set unstable

mod build '.just/build.just'
mod test '.just/test.just'
mod pack '.just/pack.just'
mod dev '.just/dev.just'
mod ci '.just/ci.just'
mod docs '.just/docs.just'

# Default version for local development
version := "0.0.0-local"

[private]
default:
    just --list --list-submodules

# Set version across all packages
set-version ver=version:
    #!/bin/bash
    set -euo pipefail
    V="{{ver}}"
    echo "Setting version to: $V"

    # Rust: workspace version + workspace dependency versions
    sed -i "0,/^version = .*/s/^version = .*/version = \"$V\"/" Cargo.toml
    for crate in wcl wcl_core wcl_eval wcl_schema wcl_serde wcl_derive wcl_lsp; do
        sed -i "s|$crate = { path = \"crates/$crate\", version = \"[^\"]*\" }|$crate = { path = \"crates/$crate\", version = \"$V\" }|" Cargo.toml
    done

    # Python: PEP 440 version (replace - with . for pre-release)
    PY_V=$(echo "$V" | sed 's/-\(.*\)/.\1/')
    sed -i "s/^version = .*/version = \"$PY_V\"/" bindings/python/pyproject.toml

    # npm (WASM + VS Code): semver with - prerelease
    sed -i "s/\"version\": \"[^\"]*\"/\"version\": \"$V\"/" bindings/wasm/package.json
    sed -i "s/\"version\": \"[^\"]*\"/\"version\": \"$V\"/" editors/vscode/package.json

    # .NET: supports semver with - prerelease
    sed -i "s|<Version>[^<]*</Version>|<Version>$V</Version>|" bindings/dotnet/src/Wcl/Wcl.csproj

    echo "All packages set to $V"
