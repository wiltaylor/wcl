#!/usr/bin/env bash
set -euo pipefail

# Convert WCL to JSON and YAML.

DIR="$(cd "$(dirname "$0")" && pwd)"
CONFIG="$DIR/../config/app.wcl"

echo "=== wcl convert --to json ==="
wcl convert "$CONFIG" --to json

echo ""
echo "=== wcl convert --to yaml ==="
wcl convert "$CONFIG" --to yaml
