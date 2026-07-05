#!/usr/bin/env bash
# Extract the verified wplan template into a destination folder.
# Usage: scripts/new-plan.sh <destination-dir>   (creates <destination-dir>/plan)
set -euo pipefail
dest="${1:?usage: new-plan.sh <destination-dir>}"
here="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "$dest"
tar -xzf "$here/assets/plan-template.tar.gz" -C "$dest"
mv "$dest/plan-template" "$dest/plan"
echo "wplan template extracted to $dest/plan"
echo "Next: cd $dest/plan && just check"
