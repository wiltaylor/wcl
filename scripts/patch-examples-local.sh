#!/usr/bin/env bash
# Deprecated compatibility shim.
#
# Examples are local-first by default now, so normal local and CI workflows
# should not mutate tracked files before running examples.
set -euo pipefail

echo "Examples already use local packages; no patching needed."
