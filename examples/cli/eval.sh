#!/usr/bin/env bash
set -euo pipefail

# Evaluate a WCL config file and output JSON.

DIR="$(cd "$(dirname "$0")" && pwd)"
CONFIG="$DIR/../config/app.wcl"

echo "=== wcl eval (default WCL output) ==="
wcl eval "$CONFIG"

echo ""
echo "=== wcl eval --format json ==="
wcl eval "$CONFIG" --format json

echo ""
echo "=== wcl eval --format json | jq -c ==="
wcl eval "$CONFIG" --format json | jq -c '.'
