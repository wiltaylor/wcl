# The review procedure

## Purpose

A strong-model code review of each verified spec's diff before merge - verification asked whether it complies with the brief; review asks whether it is good.

## Prerequisites

- A spec in :verified
- A strong model for the reviewer role - never the cheap model that implemented

## Flowchart

![diagram](../_wdoc/process_proc_review-diagram-1.svg)

## Steps

### Step 1: Review the diff against the brief

Inputs: the brief and source .wcl (the contract - review judges against it, never beyond it), the diff (`git diff <TRUNK>...<branch>`) with the worktree for context, the plan's project rules, and any as-built notes of dependencies. Look for: bugs the accepts didn't exercise (edge cases, off-by-ones, races, leaks, unhandled error paths); error handling (swallowed errors, panics on user input, missing validation at trust boundaries); security (injection, path traversal, secrets, unsafe deserialization); test quality (a test that cannot fail is a blocker); convention adherence beyond mechanics; needless complexity.

### Step 2: Scope discipline

The reviewer judges the code, not the plan. It must NOT expand scope, demand refactors of files outside the spec's ownership, relitigate decisions the brief fixed, or fail a spec for faithfully following its brief. "The brief itself is wrong/incomplete" is a real finding - but it goes to the orchestrator as a plan concern (lessons.wcl / user escalation), not into a change request to the implementer. Tier every finding: **Blocker** (bugs, security, error-handling holes, vacuous tests, project-rule violations) or **Suggestion** (style, minor structure). Any blocker means request changes; NEVER block on suggestions alone - note them and approve.

### Step 3: Approve

```wcl
status <spec_id> { state = :reviewed  by = "reviewer"  note = "approved; N suggestions noted" }
```

Edit the spec's row, then `just check` in the plan folder. (Pre-1.3.0 plans lack the `:reviewed` symbol: record approval only in `.wbuild/reports/<spec>-review-approved.md`, treat :verified + approval report as merge-ready, and suggest the user add `reviewed` to the plan schema's state set.)

### Step 3: Request changes

Write `.wbuild/reports/<spec>-review-<N>.md` in the same self-contained, literal style as verification failure reports: What must change (each blocker: file, line, the problem, why it matters, a concrete fix direction), What is fine, Suggestions (non-blocking), and Plan concerns if any. Set the row back to `:in_progress` with a note pointing at the report; the spec re-enters the fix loop, then re-verification, then re-review. Count review attempts by counting `<spec>-review-*.md`; at MAX_FIX_ATTEMPTS, `:blocked` and escalate with your read on whether the fault is code or spec.

> [!TIP]
> **Verification**
> Every merged spec carries either :reviewed or (pre-1.3.0) an approval report; no spec was blocked on suggestions alone.

[← Back to SKILL.md](../SKILL.md)
