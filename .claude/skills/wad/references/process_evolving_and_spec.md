# Evolve the WAD and write the specs

## Purpose

Turn a reviewed architecture change into implementation specs sized for the agent that will build it — the WAD-first change workflow.

## Prerequisites

- A recorded reviewed baseline revision

## Flowchart

![diagram](../_wdoc/process_evolving_and_spec-diagram-1.svg)

## Steps

### Step 1: Edit the target state

Change the \*data\* to describe the architecture as it should be — new containers, changed relations, moved deploy targets. Keep `wcl check` green; render to see the future book.

### Step 2: Read the delta

```console
$ wcl wad spec --from <reviewed-rev> wad.wcl
wrote data/specs/spec_from_ab12cd34.wcl
```

Gather what actually changed between the reviewed baseline and the new state: the git diff of the data files, `wcl diff <rev>:wad.wcl wad.wcl` over the evaluated views, and/or a `wcl wad spec` skeleton — which pins the baseline sha and the exact entity/field change list into a schema-valid spec file (spec-entity churn filtered; `--include-specs` keeps it, `--format json` prints the raw list). Add the printed import line to the specs hub.

### Step 3: Decompose into specs

An LLM reads the delta (and the surrounding WAD for context) and writes the **implementation specs**: each one a self-contained work package — scope, ordered instructions, acceptance criteria — sized for the model that will implement it. A strong model may need one broad spec (or none — it can implement from the WAD + diff directly); smaller coding models need the change cut into narrow, explicit pieces. A human shapes the cut — decomposition is hard to get right. The mechanical change list is fact: never edit it by hand, re-derive it if the data moves again.

**The bridge to issue mode.** When the change is more than one strong-model sitting — multiple work packages, parallel agents, verification wanted — don't hand-run the specs: seed an **issue-mode plan** from the delta. The WAD spec's change list and context become recon input and the mini-PRD's requirements; record the WAD spec id in the plan (a `rule` or the goal text) so the two ledgers reference each other; build mode then executes with its full verify/review/merge machinery. The WAD spec stays the architecture-level record; the plan's specs are its implementation cut.

### Step 4: Review

Review the specs (and the changed book) with the comment loop. Not approved → re-cut the specs or keep editing the data, or mark a spec `:abandoned` and revert (keep an `adr` if the abandonment itself is a decision worth recording).

### Step 5: Hand to the implementer

Set status `:in_progress` and hand off. Small change, strong model: give the agent its spec directly — the raw `data/specs/<id>.wcl` is typed and legible, and the WAD itself is the surrounding context. Larger change: the issue-mode plan seeded in the decompose step is the handoff — build mode runs it (dispatch, verification, review, merges) and reports back.

### Step 6: Close the loop

When the work lands and the user confirms (for a plan-executed change: build mode's completion — every spec :merged, scenarios green), set `:complete` (with `updated`), re-run extractors so generated data reflects the new reality, and record the merged revision as the next baseline.

> [!TIP]
> **Verification**
> Each spec is schema-valid, carries authored instructions an implementer can follow without the full context, and its status tracks the work truthfully.

## Related

- [Specs and the change workflow](../references/concept_spec_lifecycle.md)

- [wcl diff (WAD usage)](../references/entity_wcl_diff_wad.md)

- [Review a WAD](../references/process_reviewing_a_wad.md)

[← Back to SKILL.md](../SKILL.md)
