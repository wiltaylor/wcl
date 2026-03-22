#!/usr/bin/env bash
set -euo pipefail

# Evaluate a WCL config file and output JSON.

DIR="$(cd "$(dirname "$0")" && pwd)"
CONFIG="$DIR/../config/app.wcl"

echo "=== wcl eval (pretty JSON) ==="
wcl eval "$CONFIG"

echo ""
echo "=== wcl eval (compact JSON via jq) ==="
wcl eval "$CONFIG" | jq -c '.'
