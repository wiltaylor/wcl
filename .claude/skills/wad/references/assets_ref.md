# WAD — bundled files

## Scripts

Runnable scripts shipped under the skill's `scripts/` directory.

### extractor_template.py

The PEP 723 uv single-file skeleton for a new WAD extractor script — copy it, point it at a source of truth, and emit WCL into data/generated/.

**Run:** `cp scripts/extractor_template.py <wad>/scripts/extract_<thing>.py, then edit` — shipped to `scripts/extractor_template.py`.

### new-issue-plan.sh

Issue-mode scaffold: runs `wcl init wplan` into <repo>/plans/<slug>/plan, strips the greenfield bootstrap specs (files, imports, status rows — drift-guarded), and reports whether .tree/ is gitignored and whether the repo carries a living WAD.

**Run:** `scripts/new-issue-plan.sh <repo-root> <slug>` — shipped to `scripts/new-issue-plan.sh`.

### init-wbuild.sh

Build-mode bootstrap: creates .wbuild/ in the target repo with the default config, the annotated runner template as run-agent.sh, and gitignores the transient dirs.

**Run:** `scripts/init-wbuild.sh [repo-root]` — shipped to `scripts/init-wbuild.sh`.

## Data files

Static data shipped under `assets/`.

### run-agent.example.sh

Annotated runner template implementing the run-agent contract (role, worktree, prompt-file) — adapt it to your agent CLI; exits 1 with instructions until edited. — shipped to `assets/run-agent.example.sh`.

### run-agent.claude.sh

Example runner using headless Claude Code (`claude -p`) — verify its CLI flags against the installed Claude Code version before relying on it. — shipped to `assets/run-agent.claude.sh`.

[← Back to SKILL.md](../SKILL.md)
