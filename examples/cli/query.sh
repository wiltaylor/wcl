#!/usr/bin/env bash
set -euo pipefail

# Demonstrate WCL query expressions.

DIR="$(cd "$(dirname "$0")" && pwd)"
CONFIG="$DIR/../config/app.wcl"

echo "=== Query: server | .workers > 2 ==="
wcl query "$CONFIG" 'server | .workers > 2'

echo ""
echo "=== Query: server | has(.region) ==="
wcl query "$CONFIG" 'server | has(.region)'

echo ""
echo "=== Query: database ==="
wcl query "$CONFIG" 'database'

echo ""
echo "=== Query: cache ==="
wcl query "$CONFIG" 'cache'
