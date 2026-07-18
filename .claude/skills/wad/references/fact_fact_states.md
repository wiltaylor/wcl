# States and status.wcl

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
| :reviewed → :merged | orchestrator | merged + post-merge check green (pre-1.3.0 plans: merge from :verified per the review procedure's compatibility note) |
| any → :blocked | verifier/orchestrator | attempts exhausted, crash loop, stuck DAG |

Implementers and fixers write **nothing** here and are never told the ledger exists - the load-bearing invariant from the plan-mode role split; the orchestrator/verifier/reviewer write-split above is build mode's refinement of it.

**Editing the ledger.** Every spec already has a row (the status_covered gate guarantees it). Edit fields in place in plan/status.wcl - plain text, no tooling: `status spec_030_cli { state = :implemented  by = "orchestrator"  note = "runner exited 0, 4 commits" }`. After **every** edit, `just check` in the plan folder must stay green. Query one spec with `just status <spec_id>`. Keep notes one line and factual - the ledger is an audit trail, not a journal; longer narrative belongs in .wbuild/reports/ or lessons.wcl.

**lessons.wcl.** Append a `lesson` block whenever something durable surfaces: a boundary agents repeatedly violated, a brief shape that confused the runner's model, an ownership split that caused a merge conflict, a flaky accept command. Match the block shape already present in the file rather than inventing fields. At run end, review lessons with the user - general ones flow back into the planning template per the lessons loop.

## Related

- [The build roles](../references/fact_fact_roles.md)

- [State vocabularies](../references/fact_fact_state_vocab.md)

- [The verification procedure](../references/process_proc_verify.md)

- [The review procedure](../references/process_proc_review.md)

- [The fix loop and merging](../references/process_proc_fix_and_merge.md)

[← Back to SKILL.md](../SKILL.md)
