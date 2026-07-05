# Verifying and reviewing a spec

## Purpose

The verification agent's protocol: judge an implementation, record the verdict, control the merge.

## Prerequisites

- An implementation agent reports a spec complete
- Access to the plan folder and the repo worktrees

## Flowchart

![diagram](../_wdoc/process_proc_verify_spec-diagram-1.svg)

## Steps

### Step 1: Run the acceptance checks

Enter .tree/<branch> and run every acceptance command from the spec (all must exit 0), then execute every walkthrough step in order against the running application, checking each expect.

### Step 2: Check the diff against ownership

The diff may only touch paths in the spec's owns list (plus paths its allowed items explicitly cover). An out-of-bounds diff is a failure even if tests pass. Check each done item holds.

### Step 3: Record the verdict

```wcl
status spec_020_core { state = :verified  by = "verifier-1"  note = "all checks green" }
```

status.wcl is plain text - edit it directly. On failure: set :in_progress (or :blocked), write the reason in note, and return the spec to an implementation agent with the failing check named. On a PASS, also record an `asbuilt` row in asbuilt.wcl for any deviation from the brief that downstream specs must know (renamed module, changed command, adjusted signature) - no row means no deviations. Then re-render briefs (just specs) so dependents see it.

### Step 4: Strong-model review

After verification passes, a strong model reviews the diff against the brief and project rules: design quality, hidden bugs, error handling, security, whether the tests actually test. Approve -> set :reviewed. Request changes -> a review report through the same fix loop as verification failures. The reviewer judges the code, never the plan - scope complaints go to lessons.wcl, not back to the implementer.

### Step 5: Merge in wave order

Merge a branch only when its spec is :reviewed AND every dependency is :merged. After merging: set :merged and git worktree remove .tree/<branch>.

### Step 5: Run the acceptance scenarios

After the final wave merges, execute every usage scenario from out/specs/index.md end to end on trunk. The project is done when all scenarios pass - not before.

> [!TIP]
> **Verification**
> Every spec is :merged in status.wcl and every usage scenario passes on trunk.

[← All processes](../references/processes_ref.md) · [← Back to SKILL.md](../SKILL.md)
