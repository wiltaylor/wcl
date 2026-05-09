#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

EXAMPLE_OUT="${WCL_EXAMPLE_OUT:-/tmp/wdoc-example}"
DOCS_OUT="${WCL_DOCS_OUT:-/tmp/wcl-docs-build}"
DOCS_VERSION="${WCL_DOCS_VERSION:-0.1.0}"

cargo run -p wcl -- wdoc build \
  examples/wdoc/site.wcl \
  examples/wdoc/pages.wcl \
  --output "$EXAMPLE_OUT"

cargo run -p wcl -- wdoc build \
  docs/*.wcl \
  --output "$DOCS_OUT" \
  --var "version=$DOCS_VERSION"

echo "WDoc example builds completed:"
echo "  examples: $EXAMPLE_OUT"
echo "  docs:     $DOCS_OUT"
