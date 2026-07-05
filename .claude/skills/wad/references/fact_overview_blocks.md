# Overview blocks

| Block | Fields | Notes |
| --- | --- | --- |
| `wad` | `name`, `summary`, `description?`, `version?`, `created`, `updated?`, `maintainer?`, `tags[]` | exactly one, hand-authored in wad.wcl; `version` is the \*described\* system's |
| `stakeholder` | `name`, `role`, `org?`, `contact?`, `persona?` | the people who fund, build, approve, and operate |
| `raci_entry` | `activity`, `responsible[]`, `accountable[]`, `consulted[]`, `informed[]` | one row per activity; the four lists hold stakeholder ids — renders the matrix directly |
| `adr` | `n`, `title`, `status`, `date`, `deciders[]`, `context?`, `decision?`, `consequences?`, `supersedes?`, `related[]`, `body?` | `n` renders as ADR-001 and is never renumbered; the index page tables every decision plus a timeline |

Who populates: hand — stakeholders and RACI at framing time, an `adr` at the moment each decision is made.

## Related

- [The twelve views](../references/concept_twelve_views.md)

[← Back to SKILL.md](../SKILL.md)
