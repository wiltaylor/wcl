#!/usr/bin/env bash
# Scaffold .wbuild/ in a target repo. Usage: init-wbuild.sh [repo-root]
set -euo pipefail

ROOT="${1:-.}"
SKILL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WB="$ROOT/.wbuild"

if [[ -e "$WB/config" ]]; then
  echo "error: $WB/config already exists — not overwriting" >&2
  exit 1
fi

mkdir -p "$WB/prompts" "$WB/reports" "$WB/logs"

cat > "$WB/config" <<'EOF'
# wbuild orchestration config (shell-sourceable KEY=VALUE)
TRUNK=main
RUNNER=.wbuild/run-agent.sh
MAX_PARALLEL=4
MAX_FIX_ATTEMPTS=3
# Command run on trunk after each merge; empty = re-run the merged spec's accept commands
POST_MERGE_CHECK=
KEEP_BRANCHES=false
EOF

# Ship the annotated runner template as the starting runner (not executable
# until the user adapts it — it exits 1 with instructions by default).
cp "$SKILL_DIR/assets/run-agent.example.sh" "$WB/run-agent.sh"
chmod +x "$WB/run-agent.sh"

# Gitignore the transient dirs
GI="$ROOT/.gitignore"
for entry in ".wbuild/prompts/" ".wbuild/reports/" ".wbuild/logs/"; do
  grep -qxF "$entry" "$GI" 2>/dev/null || echo "$entry" >> "$GI"
done

echo "wbuild initialised at $WB"
echo "  1. Edit $WB/run-agent.sh (see also assets/run-agent.claude.sh in the skill),"
echo "     or delete it to use the orchestrator's Task-subagent fallback."
echo "  2. Review $WB/config."
