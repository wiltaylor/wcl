#!/usr/bin/env bash
set -euo pipefail

# Validate a WCL config against its inline schemas.

DIR="$(cd "$(dirname "$0")" && pwd)"
CONFIG="$DIR/../config/app.wcl"

echo "=== wcl validate ==="
wcl validate "$CONFIG"
