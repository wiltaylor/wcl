#!/usr/bin/env bash
# Example wbuild runner: headless Claude Code.
#
# CAVEAT: verify flags against your installed Claude Code version
# (`claude --help`) before trusting this — CLI flags change between
# releases and the ones below may be stale. The shape is what matters:
# cd into the worktree, feed the prompt on stdin in print mode, relax
# permissions enough for autonomous file edits + commands, block, and
# propagate the exit code.
set -euo pipefail

ROLE="$1"; WORKTREE="$2"; PROMPT_FILE="$3"

cd "$WORKTREE"

# Optionally pick a model per role, e.g.:
# MODEL_FLAG=""
# [[ "$ROLE" == "fixer" ]] && MODEL_FLAG="--model claude-opus-4-8"

claude -p "$(cat "$PROMPT_FILE")" \
  --dangerously-skip-permissions
