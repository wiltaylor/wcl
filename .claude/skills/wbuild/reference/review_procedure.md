# Review procedure

Runs once per `:verified` spec, before merge — a strong-model code review of
the diff. Verification asked "does it comply with the brief?"; review asks
"is it good?": design quality, hidden bugs, error handling, security,
convention adherence beyond mechanics, and whether the tests actually test
anything. Dispatch through the runner with role `reviewer` or as the
`wbuild-reviewer` subagent (model: opus); the orchestrator may only run it
in-session if the session itself is a strong model.

## Inputs

- The spec's brief and source .wcl (the contract — review judges against it,
  never beyond it).
- The diff: `git diff <TRUNK>...<branch>` and the worktree for context.
- The plan's project rules (in the brief) and any as-built notes of
  dependencies.

## Scope discipline

The reviewer judges the code, not the plan. It must NOT: expand scope, demand
refactors of files outside the spec's ownership, relitigate decisions the
brief fixed, or fail a spec for following its brief. "The brief itself is
wrong/incomplete" is a real finding — but it goes to the orchestrator as a
plan concern (→ lessons.wcl / user escalation), not into a change request to
the implementer.

## Verdict

Findings are tiered:

- **Blockers** — bugs, security issues, error-handling holes, tests that
  assert nothing, violations of project rules. Any blocker → request changes.
- **Suggestions** — style, minor structure, nice-to-haves. Never block on
  suggestions; record them in the report as non-blocking notes (and durable
  ones in lessons.wcl).

**Approve** — edit the spec's row in `plan/status.wcl`:

```wcl
status <spec_id> { state = :reviewed  by = "reviewer"  note = "approved; N suggestions noted" }
```

Then `just check` in the plan folder.

**Request changes** — write `.wbuild/reports/<spec>-review-<N>.md` in the same
self-contained, literal style as verification failure reports (What must
change / What is fine / each blocker: file, line, problem, why it matters,
concrete fix direction). Set the row back to `:in_progress` with a note
pointing at the report; the spec re-enters the fix loop, then re-verification,
then re-review. Count review attempts by counting `<spec>-review-*.md`; at
`MAX_FIX_ATTEMPTS`, `:blocked` and escalate with your read on whether the
fault is code or spec.

## Compatibility

Pre-1.3.0 plans lack the `:reviewed` symbol (`wcl check` will reject it).
There, record approval only in the report
(`.wbuild/reports/<spec>-review-approved.md`), treat `:verified` + approval
report as merge-ready, and suggest the user add `reviewed` to the plan
schema's PlanSpecState.
