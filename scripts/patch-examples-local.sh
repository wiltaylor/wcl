#!/usr/bin/env bash
# Patch example projects to use locally-built packages instead of published versions.
# Used in CI to test examples against the current codebase.
# This is a destructive operation — only run in ephemeral CI environments.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "Patching examples to use local packages..."

# ── Rust ──
# Replace crates.io version with path dependency
sed -i 's|^wcl = ".*"|wcl = { path = "../../crates/wcl" }|' \
  "$REPO_ROOT/examples/rust/Cargo.toml"
echo "  ✓ Rust: patched to path dependency"

# ── Python ──
# Point inline script dependency to local source build.
# uv will build the package from source when given a path.
sed -i "s|^# dependencies = \[\"pywcl>=.*\"\]|# dependencies = [\"pywcl @ file:///$REPO_ROOT/bindings/python\"]|" \
  "$REPO_ROOT/examples/python/example.py"
echo "  ✓ Python: patched to local source path"

# ── Ruby ──
# Build and install the local gem, then patch the Gemfile to accept any version.
# This avoids Bundler path-source issues with the gemspec requiring the WASM file.
(cd "$REPO_ROOT/bindings/ruby" && gem build wcl.gemspec -o /tmp/wcl-local.gem 2>/dev/null)
gem install /tmp/wcl-local.gem --no-document 2>/dev/null || true
sed -i 's|^gem "wcl", ".*"|gem "wcl"|' \
  "$REPO_ROOT/examples/ruby/Gemfile"
rm -f "$REPO_ROOT/examples/ruby/Gemfile.lock"
rm -rf "$REPO_ROOT/examples/ruby/vendor"
echo "  ✓ Ruby: installed local gem and patched Gemfile"

# ── .NET ──
# Replace NuGet version with local project reference
DOTNET_PROJ="$REPO_ROOT/examples/dotnet/Example.csproj"
if grep -q 'PackageReference Include="WclLang"' "$DOTNET_PROJ"; then
  sed -i 's|<PackageReference Include="WclLang" Version=".*" />|<ProjectReference Include="../../bindings/dotnet/src/Wcl/Wcl.csproj" />|' \
    "$DOTNET_PROJ"
  echo "  ✓ .NET: patched to local project reference"
fi

echo "Done. Examples now use locally-built packages."
