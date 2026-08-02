# System-admin blocks

| Block | Fields | Notes |
| --- | --- | --- |
| `sop` | `title`, `purpose`, `kind`, `preconditions[]`, `verification?`, `automation?`, `related[]`, steps + flow | `kind`: operations / incident / change / runbook / release / dev; `automation` names where an automated runbook lives |
| `step` | inline id, `n`, `title?`, `shape`, `body <name> { … }` fragments | `shape` is `:process` (default) / `:decision` / `:terminator`; the sop's `from -> to` statements (with `:yes` / `:no` branch labels) wire the flowchart |

**SOPs are for operating the running system** — incidents, restores, rotations, migrations.
How the software is \*built, tested, and released\* belongs in view 6: model the canonical
build/test/release flow as `pipeline` blocks with staged gates (a local from-source build is a
one-stage pipeline; the test suite is a stage or a gate), and keep view-10 `sop`s of kind
`:release`/`:dev` only for the genuinely human choreography around them (sign-offs,
announcement steps). If you find the sysadmin page filling with build instructions, they're
filed one view too far right. A dedicated testing view (suites, coverage expectations,
how-to-run per suite) is a tracked schema-0.5 candidate — until then, test suites are pipeline
stages and per-suite commands live in the pipeline's stage summaries or the repository body.


[← Back to SKILL.md](../SKILL.md)
