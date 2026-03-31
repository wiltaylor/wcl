#!/usr/bin/env bash
# Update published package versions in example projects.
# Run this after publishing a new release to keep examples current.
#
# Usage: ./scripts/update-example-versions.sh <version>
# Example: ./scripts/update-example-versions.sh 0.3.0
#          ./scripts/update-example-versions.sh 0.3.0-alpha
set -euo pipefail

if [ $# -lt 1 ]; then
  echo "Usage: $0 <version>"
  echo "  version: the new package version (e.g. 0.3.0, 0.3.0-alpha)"
  exit 1
fi

VERSION="$1"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Derive format-specific version strings
# Rust/crates.io: 0.3.0-alpha
RUST_VERSION="$VERSION"

# Python/PyPI: 0.3.0a1 (alpha -> a1, beta -> b1, rc -> rc1, release -> as-is)
PYTHON_VERSION=$(echo "$VERSION" | sed -E 's/-alpha/.a1/; s/-beta/.b1/; s/-rc/rc/')

# Ruby/RubyGems: 0.3.0.alpha1 (alpha -> .alpha1)
RUBY_VERSION=$(echo "$VERSION" | sed -E 's/-alpha/.alpha1/; s/-beta/.beta1/; s/-rc/.rc1/')

# .NET/NuGet: 0.3.0-alpha (same as Rust)
DOTNET_VERSION="$VERSION"

echo "Updating example versions to $VERSION..."

# ── Rust ──
sed -i "s|^wcl = \".*\"|wcl = \"$RUST_VERSION\"|" \
  "$REPO_ROOT/examples/rust/Cargo.toml"
echo "  ✓ Rust: wcl = \"$RUST_VERSION\""

# ── Python ──
sed -i "s|^# dependencies = \[\"pywcl>=.*\"\]|# dependencies = [\"pywcl>=$PYTHON_VERSION\"]|" \
  "$REPO_ROOT/examples/python/example.py"
echo "  ✓ Python: pywcl>=$PYTHON_VERSION"

# ── Ruby ──
sed -i "s|^gem \"wcl\", \".*\"|gem \"wcl\", \"$RUBY_VERSION\"|" \
  "$REPO_ROOT/examples/ruby/Gemfile"
echo "  ✓ Ruby: gem \"wcl\", \"$RUBY_VERSION\""

# ── .NET ──
sed -i "s|<PackageReference Include=\"WclLang\" Version=\".*\" />|<PackageReference Include=\"WclLang\" Version=\"$DOTNET_VERSION\" />|" \
  "$REPO_ROOT/examples/dotnet/Example.csproj"
echo "  ✓ .NET: WclLang $DOTNET_VERSION"

echo "Done. Commit and push to update examples."
