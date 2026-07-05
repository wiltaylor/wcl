# Fix loop and merging

## Fix loop

Triggered by a verification failure report OR a review change-request report for a spec below `MAX_FIX_ATTEMPTS` (counted separately: `<spec>-attempt-*.md` and `<spec>-review-*.md`). A review-triggered fix re-enters at verification afterwards — verify, then re-review.

1. Assemble `.wbuild/prompts/<spec>-fix-<N>.md`: the implementer wrapper from [dispatch_implementation.md](dispatch_implementation.md) with the role line changed to "You are a fix agent. A verification pass found problems with an implementation of the spec below.", then the **failure report in full**, then the **brief in full**. Same hard rules; same silence about status files.
2. Dispatch with role `fixer` into the **same worktree** — the fixer continues on the existing branch; history of the failed attempt is context, not something to rewrite (no force-pushes, no branch resets).
3. On runner exit with new commits → `:implemented` again → re-verify. The verifier numbers the next report attempt N+1.
4. Attempts exhausted → set the row to `:blocked` (`by = "verifier"`, note pointing at the last report) and escalate to the user with: the spec id, all failure reports, and your read on whether the fault is the implementation or the spec itself. If the user amends the spec, follow the correct-course procedure in [orchestrator_loop.md](orchestrator_loop.md#correct-course-mid-run-plan-changes) (edit the .wcl, `just check`, `just specs`, diff the affected briefs), then reset the attempt counter and re-enter the loop.

Count attempts by counting `.wbuild/reports/<spec>-attempt-*.md` — no separate counter to lose.

## Merging

For each merge-ready spec (`:reviewed`, all deps `:merged`), in dependency order:

1. **Sync the branch.** In the worktree: `git merge <TRUNK>` (trunk has moved if sibling specs merged first). Disjoint ownership makes conflicts unlikely; a conflict therefore signals an ownership problem — do NOT resolve it yourself. Abort the merge, write a report describing the conflicting paths, and send it through the fix loop.
2. **Re-verify if the sync changed anything.** If the merge from trunk produced a new commit, re-run the spec's accept commands in the worktree before proceeding (cheap re-verification; a full pass is not needed since ownership can't have regressed).
3. **Merge to trunk.** In the repo root, on trunk: `git merge --no-ff <branch>` with a message like `merge spec/030-cli: CLI layer`.
4. **Post-merge check.** Run `POST_MERGE_CHECK` on trunk; if unset, prefer the project's full test suite (the canonical command is usually in spec_010's accepts or an as-built note) over just the merged spec's accepts — one spec's merge can break a sibling's behaviour that its own accepts never exercise. Fall back to the merged spec's accepts only when no full-suite command is known. Failure here means an integration problem the per-worktree checks couldn't see: **stop, do not mark merged, do not revert on your own** — show the user the failure and agree on a path (usually a fix cycle on the branch after `git merge --abort`-style cleanup, i.e. `git reset --hard` trunk to the pre-merge commit only with the user's go-ahead).
5. **Record and clean.** Set the row to `:merged` (`by = "orchestrator"`), `just check`, then `git worktree remove .tree/<branch>` and (unless KEEP_BRANCHES) `git branch -d <branch>`.

Merging first each loop iteration matters: every merge can unlock the next wave, so specs sitting `:verified` are the highest-value work in the queue.
