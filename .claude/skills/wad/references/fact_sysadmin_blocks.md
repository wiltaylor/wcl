# System-admin blocks

| Block | Fields | Notes |
| --- | --- | --- |
| `sop` | `title`, `purpose`, `kind`, `preconditions[]`, `verification?`, `automation?`, `related[]`, steps + flow | `kind`: operations / incident / change / runbook / release / dev; `automation` names where an automated runbook lives |
| `step` | inline id, `n`, `title?`, `shape`, `body <name> { … }` fragments | `shape` is `:process` (default) / `:decision` / `:terminator`; the sop's `from -> to` statements (with `:yes` / `:no` branch labels) wire the flowchart |

## Related

- [The twelve views](../references/concept_twelve_views.md)

[← Back to SKILL.md](../SKILL.md)
