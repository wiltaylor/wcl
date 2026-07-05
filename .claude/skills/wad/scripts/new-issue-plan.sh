#!/usr/bin/env bash
# Scaffold a brownfield wplan plan for a bug/feature on an existing repo.
#
# Usage: new-issue-plan.sh <repo-root> <slug>
#
# Scaffolds the verified wplan template with `wcl init wplan` into
# <repo-root>/plans/<slug>/plan and strips the greenfield bootstrap specs
# (spec_000_repo, spec_010_build): their files, their plan.wcl imports, and
# their status.wcl rows.
# Aborts if the template doesn't match what this surgery expects (drift guard).
set -euo pipefail

usage() { echo "usage: new-issue-plan.sh <repo-root> <slug>" >&2; exit 2; }

REPO="${1:-}"; SLUG="${2:-}"
[[ -n "$REPO" && -n "$SLUG" ]] || usage
[[ $# -le 2 ]] || usage

[[ "$SLUG" =~ ^[a-z0-9][a-z0-9-]*$ ]] || { echo "error: slug must be lowercase kebab-case" >&2; exit 2; }
[[ -d "$REPO/.git" ]] || { echo "error: $REPO is not a git repository root" >&2; exit 1; }
command -v wcl >/dev/null || {
  echo "error: wcl not found on PATH — the plan template is the wplan built-in of \`wcl init\`." >&2
  exit 1
}

DEST="$REPO/plans/$SLUG"
[[ -e "$DEST/plan" ]] && { echo "error: $DEST/plan already exists — not overwriting" >&2; exit 1; }

wcl init wplan "$DEST/plan" --defaults
PLAN="$DEST/plan"

# ---- brownfield surgery, with drift guards ---------------------------------
# Every line we intend to remove must exist first; otherwise the built-in
# template has drifted from what this surgery was verified against.

require_line() { # file, exact line
  grep -qxF "$2" "$1" || {
    echo "DRIFT GUARD: expected line not found in $1:" >&2
    echo "  $2" >&2
    echo "The wcl built-in wplan template has changed since this surgery was" >&2
    echo "written — do the brownfield surgery by hand and update this skill." >&2
    exit 1
  }
}

remove_line() { # file, exact line
  require_line "$1" "$2"
  grep -vxF "$2" "$1" > "$1.tmp" && mv "$1.tmp" "$1"
}

for f in specs/spec_000_repo.wcl specs/spec_010_build.wcl; do
  [[ -f "$PLAN/$f" ]] || { echo "DRIFT GUARD: $f missing from template" >&2; exit 1; }
done

remove_line "$PLAN/plan.wcl" 'import "./specs/spec_000_repo.wcl"'
remove_line "$PLAN/plan.wcl" 'import "./specs/spec_010_build.wcl"'

# Status rows: template ships them as single lines 'status spec_000_repo { state = :todo }'
STATUS_000="$(grep -E '^status spec_000_repo ' "$PLAN/status.wcl" || true)"
STATUS_010="$(grep -E '^status spec_010_build ' "$PLAN/status.wcl" || true)"
[[ -n "$STATUS_000" && -n "$STATUS_010" ]] || {
  echo "DRIFT GUARD: expected status rows for spec_000_repo / spec_010_build not found" >&2
  exit 1
}
remove_line "$PLAN/status.wcl" "$STATUS_000"
remove_line "$PLAN/status.wcl" "$STATUS_010"

rm "$PLAN/specs/spec_000_repo.wcl" "$PLAN/specs/spec_010_build.wcl"

# ---- repo facts the planner needs ------------------------------------------
echo
echo "brownfield plan scaffolded at $PLAN"
if git -C "$REPO" check-ignore -q .tree/x 2>/dev/null; then
  echo "  .tree/ is gitignored: YES — no spec_000_prep needed for this."
else
  echo "  .tree/ is gitignored: NO  — include spec_000_prep (see the spec-shapes concept)."
fi
if [[ -d "$REPO/.wad" ]]; then
  echo "  living WAD found at .wad/ — recon starts by reading it."
else
  echo "  no .wad/ in the repo — recon works from raw code (consider doc mode later)."
fi
echo "Next: cd $PLAN && just check   (must be green before adding content)"
