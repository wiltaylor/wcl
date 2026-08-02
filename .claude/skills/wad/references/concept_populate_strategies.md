# Interview vs scan

_The two population strategies — interview the user (new design) or scan a codebase (existing system) — and when to use each._

Two processes converge on the same block model:

- **Greenfield** (nothing built yet) → run the \*design interview\*: ask the user one view at a time, write blocks as answers land. The question bank is data — see the interview checklist.
- **Existing system** → run the \*codebase scan\* first: cheap, high-signal sources (repo layout, CI config, entry points, config files) fill most views mechanically; then a targeted mini-interview covers what code can't say (contacts, approvals, off-repo infrastructure).

Hybrid is normal: scan what exists, interview the rest. Either way, never invent — a gap is
recorded as an open question, not filled with a guess.


[← Back to SKILL.md](../SKILL.md)
