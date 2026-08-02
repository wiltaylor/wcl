# Implementation vs verification

_Implementation agents build; a separate verification agent judges, records status, and controls merges - so agents never mark their own work done._

status.wcl is edited by the verification agent only. Implementation briefs never mention it -
they end with an instruction to stop and NOT update any status files. Do not leak the status
mechanism into any implementation brief.


The verification agent gets the plan folder, the repository with its worktrees, and the
verification procedure. It runs every acceptance command, diffs against the spec's ownership,
checks the done list, records the verdict, and gates merging on `:verified` plus merged
dependencies.


## Examples

### Recording a verification verdict

status.wcl is plain text - the verification agent edits it directly, no tooling needed.

```wcl
status spec_020_core { state = :verified  by = "verifier-1"  note = "all checks green" }
```

**Expected:** just status spec_020_core prints :verified.

## Related

- [Self-contained briefs](../references/concept_briefs.md) — Self-contained briefs supports Implementation vs verification: Every exported spec .md must stand alone: rules, context and findings are copied in, never referenced.

[← Back to SKILL.md](../SKILL.md)
