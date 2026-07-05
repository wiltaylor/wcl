#!/usr/bin/env bash
# wbuild runner contract:
#   run-agent.sh <role> <worktree-path> <prompt-file>
#
#   role         implementer | fixer | verifier
#   worktree     absolute path the agent must treat as its whole world
#   prompt-file  absolute path; pass its CONTENTS to the agent
#
# Semantics: block until the agent finishes. Exit 0 = the agent ran to
# completion (NOT that the work is correct — verification decides that).
# Non-zero = the agent itself crashed or timed out.
#
# Adapt the case below to your agent CLI(s). You can route roles to
# different models/backends — e.g. a cheap local model for implementer,
# a stronger one for fixer.
set -euo pipefail

ROLE="$1"; WORKTREE="$2"; PROMPT_FILE="$3"

case "$ROLE" in
  implementer|fixer|verifier)
    echo "run-agent.sh: no agent backend configured yet." >&2
    echo "Edit .wbuild/run-agent.sh — see run-agent.claude.sh in the wbuild" >&2
    echo "skill assets for a headless Claude Code example." >&2
    exit 1
    ;;
  *)
    echo "run-agent.sh: unknown role '$ROLE'" >&2
    exit 2
    ;;
esac
