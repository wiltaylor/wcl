# The orchestrator loop

The loop is **derived, not remembered**: every iteration recomputes what to do from `status.wcl` + git, so a crashed or interrupted run resumes by simply starting the loop again. Never keep the schedule only in your head.

## Preflight (once per run)

1. **Locate the plan.** `$ARGUMENTS` or `./plan`. Confirm `plan.wcl`, `justfile`, `specs/`, `status.wcl` exist. The repo root is the plan folder's parent unless the user says otherwise.
2. **Green-check and render.** In the plan folder: `just check-full` must pass (wplan 1.2.0+ — this includes the phase signoffs; on older plans without the recipe, `just check`), then `just specs` to re-render `out/specs/` so briefs match the current plan. Never dispatch from stale briefs. If the check fails, stop and show the user the failing gate output — never edit plan files to make it pass (plan edits are wplan's job).
3. **Read the DAG.** Parse `out/specs/index.md` for the spec list, waves, and each spec's `depends_on`, `branch`, and `owns`. Note the **Final acceptance scenarios** section (wplan 1.1.0+) — it is the completion criterion, not documentation. Cross-check state per spec with `just status <spec_id>`.
4. **Git preflight.** In the repo root, on trunk (default `main`; see config): `git status --porcelain` must print nothing — if it prints anything, show the user the output and ask before proceeding. Confirm `.tree/` is gitignored (spec_000 does this — if missing, stop and ask), and note any existing `spec/*` branches or `.tree/*` worktrees from a previous run — these are resume state, not garbage.
5. **Load config.** Read `.wbuild/config` if present; otherwise offer to run `${CLAUDE_SKILL_DIR}/scripts/init-wbuild.sh <repo-root>` or proceed with defaults (see [runner contract](runner_contract.md)).
6. **Present the plan.** Show the user the waves, current states, and what the first iteration will do. Get a go-ahead before the first dispatch.

## The loop

Repeat until every spec is `:merged` (or something is `:blocked` and escalated). Each iteration, classify every spec and act in this priority order — merging first keeps the DAG unblocking:

### 1. Merge-ready — state `:reviewed` and all `depends_on` are `:merged`

Follow [fix_and_merge.md](fix_and_merge.md#merging). Merge in dependency order (a topological order of the ready set; wave order from index.md is a valid tiebreak). Mark `:merged` after each successful merge + post-merge check. Then run any scenario whose `ready_after` names the just-merged spec (index.md marks these "Runnable after X merges") — same discipline as the final scenarios, and a failure here is far cheaper to attribute than at the end.

### 2. Review-ready — state `:verified`

Run the [review procedure](review_procedure.md) for each — a strong-model code review of the diff. Reviewer records `:reviewed`, or reverts to `:in_progress` with a review report → the spec becomes fix-ready.

### 3. Verify-ready — state `:implemented`

Run the [verification procedure](verification_procedure.md) for each. Verifier records `:verified`, or reverts to `:in_progress` with a failure report → the spec becomes fix-ready.

### 4. Fix-ready — state `:in_progress` with a failure or review report in `.wbuild/reports/` and no agent currently running

Follow the [fix loop](fix_and_merge.md#fix-loop). If attempts have hit `MAX_FIX_ATTEMPTS`, set `:blocked` and escalate to the user instead.

### 5. Dispatch-ready — state `:todo` and all `depends_on` are `:merged`

These form the current **wave**. Re-render briefs first (`just specs`) so any as-built notes recorded since the last render reach the briefs — dependent briefs must describe the world as built, not as planned. Then dispatch up to `MAX_PARALLEL` at once per [dispatch_implementation.md](dispatch_implementation.md); if the plan records complexity scores for specs, weight the wave by them — fewer high-complexity specs in flight at once, using ownership breadth as the proxy when no scores exist. Mark each `:in_progress` when its agent launches, `:implemented` when its runner exits and the branch has commits.

### 6. Nothing ready

If agents are still running, wait for them. If nothing is running and nothing is ready but specs remain unmerged, the DAG is stuck — some dependency is `:blocked`. Summarise the stuck subgraph and escalate.

**Resume note:** a spec found `:in_progress` at preflight with *no* running agent and *no* failure report was interrupted mid-implementation. Inspect its worktree: committed work → treat as `:implemented` and verify; dirty or empty → ask the user whether to re-dispatch (the implementer prompt tells agents to commit, so uncommitted work is suspect).

## Correct course (mid-run plan changes)

When implementation reveals the *plan* is wrong — a dependency assumption that doesn't hold, an obsolete requirement, a spec whose scope no longer makes sense — do not improvise around it. The procedure:

1. **Pause dispatch.** Let running agents finish; dispatch nothing new.
2. **Amend the plan at the source.** Plan edits go through wplan/wissue: edit the `.wcl` files (never `out/`), with the user's approval for anything that changes scope or requirements.
3. **Re-gate and re-render.** In the plan folder: `just check` must be green, then `just specs`. If the check fails, the amendment is incomplete — stop and show the user.
4. **Diff the affected briefs.** `git diff` on `out/specs/` (or compare against the prompts already dispatched) to see which `:todo` specs changed. Specs already `:merged` are history; specs `:in_progress` with a changed brief need a decision from the user (let them finish against the old brief and fix after, or abandon the worktree and re-dispatch).
5. **Resume the loop.** The wave computation picks up the amended DAG automatically — it is derived from status.wcl + git, not from memory.

Small course corrections that only affect one spec's future work are usually better recorded as an `asbuilt` row (the verifier's mechanism) than as a plan amendment — reserve this procedure for changes to requirements, dependencies, or scope.

## Completion

When all specs are `:merged`:

1. Run the post-merge check one final time on trunk.
2. **Final acceptance scenarios.** Execute every scenario from `index.md` end to end on trunk, in order, checking each step's expectation against the running application (same discipline as a verification walkthrough). Scenarios are cross-spec by nature, so a failure here does NOT automatically reopen a spec: write a scenario-failure report (`.wbuild/reports/scenario-<id>.md` — the step that failed, the surface involved, expected vs observed) and escalate to the user with your read on which spec's work is implicated. The user may reopen a spec (fix loop), amend the plan (through wplan), or raise it as a fresh issue (wissue). **The run is not complete until every scenario passes.**
3. **Fold knowledge back.** (a) If the plan folder has a sibling `capability-deltas.md` (wissue plans), apply its ADDED/MODIFIED/REMOVED lines to `plans/capabilities/<capability>.md` and stamp the deltas file `Merged into capabilities on <date>.` — the wissue skill's capabilities reference documents the format. (b) Create or update `plans/project-context.md` with what the run established: the canonical build/test commands actually used, conventions agents were held to, landmines hit (update entries in place; keep it concise and factual).
4. Remove remaining `.tree/*` worktrees (`git worktree prune` after removal) and, unless `KEEP_BRANCHES=true`, delete the merged `spec/*` branches.
5. Summarise the run for the user: waves executed, fix cycles per spec, scenario results, anything notable.
6. Review `lessons.wcl` with the user — per wplan's lessons loop, durable lessons should flow back into the wplan template and skills.
