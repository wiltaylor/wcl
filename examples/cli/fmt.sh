#!/usr/bin/env bash
set -euo pipefail

# Check WCL formatting.

DIR="$(cd "$(dirname "$0")" && pwd)"
CONFIG="$DIR/../config/app.wcl"

echo "=== wcl fmt --check ==="
if wcl fmt --check "$CONFIG" 2>&1; then
    echo "File is already formatted."
else
    echo "File is not formatted (this is expected for the example)."
fi
