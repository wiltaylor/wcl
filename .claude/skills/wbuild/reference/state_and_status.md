# States and status.wcl

## The state machine (wplan's spec vocabulary)

```text
:todo → :in_progress → :implemented → :verified → :reviewed → :merged
              ↑              │              │
              ├─ verify fail ┘              │
              └─ review fail (changes requested)
                                      any state → :blocked (escalate)
```

| Transition | Written by | When |
| --- | --- | --- |
| :todo → :in_progress | orchestrator | agent launched |
| :in_progress → :implemented | orchestrator | runner exited 0 with commits |
| :implemented → :verified | verifier | all checks green |
| :implemented → :in_progress | verifier | verification failed (report written) |
| :verified → :reviewed | reviewer | review approved |
| :verified → :in_progress | reviewer | changes requested (review report written) |
| :reviewed → :merged | orchestrator | merged + post-merge check green (pre-1.3.0 plans have no `:reviewed`; merge from `:verified` per the review procedure's compatibility note) |
| any → :blocked | verifier/orchestrator | attempts exhausted, crash loop, stuck DAG |

Implementers and fixers write **nothing** here and are never told the ledger exists. This is the load-bearing invariant from wplan's role split; the orchestrator/verifier/reviewer write-split above is wbuild's refinement of it (wplan's docs say verifier-only — if you want that stricter reading, have the verifier record the scheduling transitions on the orchestrator's report instead; the ledger syntax is identical).

## Editing the ledger

Every spec already has a row (wplan's `status_covered` gate guarantees it). Edit fields in place in `plan/status.wcl` — plain text, no tooling:

```wcl
status spec_030_cli { state = :implemented  by = "orchestrator"  note = "runner exited 0, 4 commits" }
```

After **every** edit: `just check` in the plan folder must stay green. Query one spec with `just status <spec_id>`.

Keep notes one line and factual — the ledger is an audit trail, not a journal. Longer narrative belongs in `.wbuild/reports/` or lessons.wcl.

## lessons.wcl

Append a `lesson` block whenever something durable surfaces: a boundary agents repeatedly violated, a brief shape that confused the runner's model, an ownership split that caused a merge conflict, an accept command that was flaky. Match the block shape already present in the file (or the plan schema) rather than inventing fields. At run end, review lessons with the user — general ones flow back into the wplan template per its lessons loop.
