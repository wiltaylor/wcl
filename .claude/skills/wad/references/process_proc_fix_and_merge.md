# The fix loop and merging

## Purpose

Bounded fix retries in the same worktree, escalation on exhaustion, and dependency-ordered merges with a post-merge check.

## Prerequisites

- A failure or review report in .wbuild/reports/, or a merge-ready (:reviewed) spec

## Flowchart

![diagram](../_wdoc/process_proc_fix_and_merge-diagram-1.svg)

## Steps

### Step 1: The fix loop

Triggered by a verification failure report OR a review change-request for a spec below MAX_FIX_ATTEMPTS (counted separately: `<spec>-attempt-*.md` and `<spec>-review-*.md` - count files, no separate counter to lose). 1. Assemble `.wbuild/prompts/<spec>-fix-<N>.md`: the implementer wrapper with the role line changed to "You are a fix agent. A verification pass found problems with an implementation of the spec below.", then the **failure report in full**, then the **brief in full**; same hard rules, same silence about status files. 2. Dispatch with role fixer into the **same worktree** - the fixer continues on the existing branch; the failed attempt's history is context, not something to rewrite (no force-pushes, no branch resets). 3. On runner exit with new commits: `:implemented` again - re-verify (a review-triggered fix re-enters at verification: verify, then re-review). 4. Attempts exhausted: set `:blocked` and escalate with the spec id, all reports, and your read on whether the fault is the implementation or the spec; if the user amends the spec, follow the correct-course procedure, reset the attempt counter, and re-enter.

### Step 2: Merging

For each merge-ready spec (:reviewed, all deps :merged), in dependency order: 1. **Sync the branch**: in the worktree, `git merge <TRUNK>` (trunk moved if siblings merged first). Disjoint ownership makes conflicts unlikely - a conflict therefore signals an ownership problem: do NOT resolve it yourself; abort, write a report describing the conflicting paths, and send it through the fix loop. 2. **Re-verify if the sync changed anything**: if the merge produced a new commit, re-run the spec's accept commands in the worktree. 3. **Merge to trunk**: in the repo root, `git merge --no-ff <branch>` with a message like `merge spec/030-cli: CLI layer`. 4. **Post-merge check**: run POST_MERGE_CHECK on trunk; if unset, prefer the project's full test suite over just the merged spec's accepts - one spec's merge can break a sibling's behaviour its own accepts never exercise. Failure means an integration problem the per-worktree checks couldn't see: stop, do not mark merged, do not revert on your own - show the user and agree a path. 5. **Record and clean**: set `:merged` (by = "orchestrator"), `just check`, `git worktree remove .tree/<branch>`, and (unless KEEP_BRANCHES) `git branch -d <branch>`. Merging first each iteration matters: every merge can unlock the next wave, so specs sitting :reviewed are the highest-value work in the queue.

> [!TIP]
> **Verification**
>
> Merges land in dependency order with the post-merge check green; conflicts route through the fix loop, never hand-resolved.

[← Back to SKILL.md](../SKILL.md)
