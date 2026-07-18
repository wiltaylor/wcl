# The orchestrator loop

## Purpose

Preflight, the resumable wave loop, mid-run course correction, and completion - derived from status.wcl + git every iteration, never remembered.

## Prerequisites

- A rendered plan (out/specs/ exists) produced by plan or issue mode
- The orchestrator session never writes application code

## Flowchart

![diagram](../_wdoc/process_proc_orchestrator_loop-diagram-1.svg)

## Steps

### Step 1: Preflight (once per run)

1. **Locate the plan** ($ARGUMENTS or ./plan; confirm plan.wcl, justfile, specs/, status.wcl exist; repo root is the plan folder's parent unless the user says otherwise). 2. **Green-check and render**: `just check-full` must pass, then `just specs` so briefs match the current plan - never dispatch from stale briefs; if the check fails, stop and show the user the failing gate output, never edit plan files to make it pass (plan edits are plan/issue mode's job). 3. **Read the DAG**: parse out/specs/index.md for the spec list, waves, each spec's depends_on/branch/owns, and the **Final acceptance scenarios** section - it is the completion criterion, not documentation; cross-check state per spec with `just status <spec_id>`. 4. **Git preflight**: on trunk, `git status --porcelain` must print nothing (else show the user and ask); confirm `.tree/` is gitignored; note existing `spec/*` branches or `.tree/*` worktrees from a previous run - resume state, not garbage. 5. **Load config**: read `.wbuild/config` if present, else offer the bundled `init-wbuild.sh` or proceed with defaults (see the runner-contract fact). 6. **Present the plan**: waves, current states, what the first iteration will do; get a go-ahead before the first dispatch.

**Resume note:** a spec found `:in_progress` at preflight with no running agent and no failure report was interrupted mid-implementation. Inspect its worktree: committed work means treat as `:implemented` and verify; dirty or empty means ask the user whether to re-dispatch.

### Step 2: Classify every spec, act in priority order

Repeat until every spec is `:merged` (or something is `:blocked` and escalated). Each iteration, recompute from status.wcl + git and act in this order - merging first keeps the DAG unblocking: **1. Merge-ready** (`:reviewed`, all deps `:merged`) - merge per the fix-and-merge runbook, then run any scenario whose ready_after names the just-merged spec. **2. Review-ready** (`:verified`) - run the review runbook. **3. Verify-ready** (`:implemented`) - run the verification runbook. **4. Fix-ready** (`:in_progress` with a failure or review report in .wbuild/reports/ and no agent running) - the fix loop; at MAX_FIX_ATTEMPTS set `:blocked` and escalate. **5. Dispatch-ready** (`:todo`, all deps `:merged`) - the current wave: re-render briefs first (`just specs`) so as-built notes reach dependent briefs, then dispatch up to MAX_PARALLEL per the dispatch runbook (fewer high-complexity specs in flight at once, ownership breadth as the proxy). **6. Nothing ready** - agents running: wait; nothing running and specs remain: the DAG is stuck on something `:blocked` - summarise the stuck subgraph and escalate.

### Step 3: Keep the plan's WAD tracking the build

If the plan carries a WAD (`wad/` beside plan.wcl - plan mode scaffolds one), run `just wad-extract` in the plan folder after status.wcl moves - at minimum once per loop iteration that changed any state. The WAD's spec statuses track the plan's verbatim via plan_state tags, so the architecture book of the future system always shows the build's real progress. This is cheap (one extractor run) and keeping it current is what makes the graduation step at completion trustworthy.

### Step 4: Correct course (mid-run plan changes)

When implementation reveals the \*plan\* is wrong - a dependency assumption that doesn't hold, an obsolete requirement, a spec whose scope no longer makes sense - do not improvise around it: 1. Pause dispatch (let running agents finish). 2. Amend the plan at the source - the .wcl files, through plan/issue mode, with the user's approval for anything that changes scope or requirements; never out/. 3. Re-gate and re-render (`just check`, then `just specs`); a failing check means the amendment is incomplete. 4. Diff the affected briefs (git diff on out/specs/): `:merged` specs are history; `:in_progress` specs with a changed brief need a user decision (finish against the old brief and fix after, or abandon the worktree and re-dispatch). 5. Resume the loop - the wave computation picks up the amended DAG automatically. Small corrections affecting only one spec's future work are usually better recorded as an `asbuilt` row than a plan amendment.

### Step 5: Completion

When all specs are `:merged`: 1. Run the post-merge check one final time on trunk. 2. **Final acceptance scenarios**: execute every scenario from index.md end to end on trunk, in order. Scenarios are cross-spec, so a failure does NOT automatically reopen a spec: write a scenario-failure report (`.wbuild/reports/scenario-<id>.md` - failed step, surface involved, expected vs observed) and escalate with your read on which spec is implicated; the user may reopen a spec, amend the plan, or raise a fresh issue. **The run is not complete until every scenario passes.**

3. **Fold knowledge back.** (a) If the plan folder has a sibling capability-deltas.md (issue plans), apply its ADDED/MODIFIED/REMOVED lines to `plans/capabilities/<capability>.md` and stamp the deltas file `Merged into capabilities on <date>.` (b) Create or update `plans/project-context.md` with what the run established: canonical build/test commands actually used, conventions agents were held to, landmines hit. (c) **Update the architecture record.** Greenfield plan carrying a WAD: run the graduation runbook - the plan's WAD becomes the repo's living WAD. Repo that already has a living `.wad/`: run the keeping-current sweep (re-extract, diff against the reviewed baseline, capture ADRs for decisions made during the build); when the change was architectural, `wcl wad spec --from <pre-build-rev>` derives the record of what actually changed.

4. Remove remaining `.tree/*` worktrees (`git worktree prune` after removal) and, unless KEEP_BRANCHES=true, delete the merged `spec/*` branches. 5. Summarise the run: waves executed, fix cycles per spec, scenario results, anything notable. 6. Review lessons.wcl with the user - durable lessons flow back into the planning template and this skill.

> [!TIP]
> **Verification**
> Every spec :merged, every scenario green on trunk, knowledge folded back (capabilities, project-context, the WAD), worktrees pruned.

[← Back to SKILL.md](../SKILL.md)
