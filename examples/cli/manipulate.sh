#!/usr/bin/env bash
set -euo pipefail

# Demonstrate wcl set, add, and remove commands.
# Works on a temporary copy so the original is never modified.

DIR="$(cd "$(dirname "$0")" && pwd)"
CONFIG="$DIR/../config/app.wcl"

TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

cp "$CONFIG" "$TMPDIR/app.wcl"
WORK="$TMPDIR/app.wcl"

echo "=== wcl set: change redis TTL to 600 ==="
wcl set "$WORK" 'cache | .id == "redis" ~> .ttl = 600'
echo "Done."

echo ""
echo "=== wcl add: add a new cache block ==="
wcl add "$WORK" 'cache new_cache { host = "localhost" port = 11211 }'
echo "Done."

echo ""
echo "=== wcl remove: remove memcached block ==="
wcl remove "$WORK" 'cache | .id == "memcached"'
echo "Done."

echo ""
echo "=== Resulting config ==="
cat "$WORK"
