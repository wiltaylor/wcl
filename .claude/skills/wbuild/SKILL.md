---
name: wbuild
description: "Implement a wplan plan: orchestrate implementation, verification, fix and merge agents over the plan's spec DAG until every spec is merged into trunk. Use this skill whenever the user wants to execute, implement, build or 'run' a wplan plan, work through spec briefs in out/specs/, dispatch coding agents against a plan folder, verify/QA implemented specs, or merge spec branches — even if they just say 'start building the plan' or 'implement the next wave'."
user-invocable: true
argument-hint: "[plan-folder-path]"
allowed-tools:
  # Bash is deliberately unscoped: the orchestrator runs plan-supplied accept
  # commands, POST_MERGE_CHECK, and user-configured runner scripts — unknowable globs.
  - Bash
  - Read
  - Write
  - Edit
  - Glob
  - Grep
  - Task
---

# wbuild

<overview>
Execute a plan produced by the **wplan** skill. The current session is the **orchestrator**: it never writes application code itself. It schedules work to sub-agents in waves over the spec DAG, has a **verification agent** judge each implemented spec, a **strong-model reviewer** judge each verified spec's code quality, dispatches a **fix agent** when either fails, and merges reviewed branches back into trunk in dependency order.

The contract with wplan: briefs live in `plan/out/specs/*.md` (one standalone brief per spec plus `index.md` with the waves and — since wplan 1.1.0 — the **Final acceptance scenarios**), each spec builds on branch `spec/NNN-name` in a git worktree at `.tree/<branch>`, `status.wcl` is the verification ledger, and spec states move `:todo → :in_progress → :implemented → :verified → :reviewed → :merged` (or `:blocked`; `:reviewed` requires wplan 1.3.0+ schemas — see the review procedure's compatibility note for older plans). A run is complete when every spec is `:merged` **and every scenario passes on trunk** — not at the last merge.
</overview>

<variables>
- `${CLAUDE_SKILL_DIR}`: path to this skill's directory (its `reference/`, `scripts/`, and `assets/` live here).
- `$ARGUMENTS`: path to the plan/ folder (or the repo containing it). Take it from the user's request; default to `./plan` if present. If neither exists, ask.
</variables>

<roles>
| Role | Runs as | Sees | Never sees |
| --- | --- | --- | --- |
| Orchestrator | this session | everything | — |
| Implementer | sub-agent via runner | its own brief + its worktree | status.wcl, plan/, other briefs |
| Verifier | sub-agent (or orchestrator-run procedure) | plan/, worktrees, verification procedure | — |
| Reviewer | strong-model sub-agent via runner (role `reviewer`) | plan/, worktrees, diff, review procedure | — |
| Fixer | sub-agent via runner | its brief + failure report + its worktree | status.wcl, plan/, other briefs |

Status writes are split by role — the invariant is that *implementation-side agents never touch or hear about status.wcl*. The authoritative who-writes-which-transition table is in [States and status.wcl](${CLAUDE_SKILL_DIR}/reference/state_and_status.md).
</roles>

<workflow>
Read in this order for a fresh run:

- [Orchestrator loop](${CLAUDE_SKILL_DIR}/reference/orchestrator_loop.md) — preflight, the resumable wave loop, and completion. **Start here.**

- [Runner contract](${CLAUDE_SKILL_DIR}/reference/runner_contract.md) — how sub-agents are launched: the `.wbuild/` config, the run-agent script contract, and the Task-subagent fallback.

- [Dispatching implementation](${CLAUDE_SKILL_DIR}/reference/dispatch_implementation.md) — branch/worktree setup and the implementer prompt template.

- [Verification procedure](${CLAUDE_SKILL_DIR}/reference/verification_procedure.md) — what the verifier checks, verdict recording, failure reports.

- [Review procedure](${CLAUDE_SKILL_DIR}/reference/review_procedure.md) — the strong-model pre-merge code review: scope discipline, blocker/suggestion tiers, verdict recording.

- [Fix loop and merging](${CLAUDE_SKILL_DIR}/reference/fix_and_merge.md) — bounded fix retries, escalation, dependency-ordered merges, post-merge checks.

- [States and status.wcl](${CLAUDE_SKILL_DIR}/reference/state_and_status.md) — the state machine, who writes what, and the edit syntax.
</workflow>

<boundaries>
<always>
- Re-render briefs (`just specs`) and confirm `just check` is green before dispatching anything; keep it green after every status.wcl or lessons.wcl edit.
- Give an implementer or fixer ONLY its brief, its failure report (fixer), and its worktree — status.wcl, the plan folder, and sibling briefs stay out of their prompts and paths.
- Verify then review before merging, and merge only `:reviewed` specs whose dependencies are `:merged`, in dependency order. The reviewer must be a strong model — do not route review to the same cheap model that implemented.
- Run the final acceptance scenarios from `index.md` on trunk after the last merge — the run is not complete until they all pass.
- Record durable observations in lessons.wcl as the run progresses (what confused agents, boundaries violated, spec shapes that worked).
</always>
<ask>
- Before dispatching the first wave (present the wave plan), before merging when the post-merge check fails, and whenever a spec goes `:blocked`.
- Before deleting a worktree that contains uncommitted work.
</ask>
<never>
- Write application code in the orchestrator session — all code changes go through implementer/fixer agents.
- Let an agent mark its own work done, or merge a spec that skipped verification or review.
- Hand-edit anything under plan/out/, or push to any remote unless the user asks.
</never>
</boundaries>

## Bundled files

- `${CLAUDE_SKILL_DIR}/scripts/init-wbuild.sh` — creates `.wbuild/` with a default config and example runner in the target repo.
- `assets/run-agent.example.sh` — annotated runner template.
- `assets/run-agent.claude.sh` — example runner using headless Claude Code (`claude -p`).
