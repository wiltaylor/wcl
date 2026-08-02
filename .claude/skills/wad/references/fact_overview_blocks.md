# Overview blocks

| Block | Fields | Notes |
| --- | --- | --- |
| `wad` | `name`, `summary`, `description?`, `version?`, `created`, `updated?`, `maintainer?`, `tags[]` | exactly one, hand-authored in wad.wcl; `version` is the \*described\* system's |
| `stakeholder` | `name`, `role`, `org?`, `contact?`, `persona?` | the people who fund, build, approve, and operate |
| `raci_entry` | `activity`, `responsible[]`, `accountable[]`, `consulted[]`, `informed[]` | one row per activity; the four lists hold stakeholder ids — renders the matrix directly |
| `adr` | `n`, `title`, `status`, `date`, `deciders[]`, `context?`, `decision?`, `consequences?`, `supersedes?`, `options[]`, `related[]`, `body?` | `n` renders as ADR-001 and is never renumbered; the index page tables every decision plus a timeline |
| `adr_option` | `title`, `summary?`, `rejected_because?` | nested in `adr` — one alternative weighed; an option **without** `rejected_because` marks the winner. The ADR page renders them as an Options-considered table |

Who populates: hand — stakeholders and RACI at framing time, an `adr` at the moment each
decision is made (with its `adr_option` children when alternatives were genuinely weighed; a
convention-recording ADR honestly has none). A `stakeholder` is a **person or organisation**
with skin in the game — never a platform or service (GitHub, a cloud, a CI provider belong in
view 5 as `infra_node`s; a vendor whose API the system calls is the `vendor` of an external
system).


[← Back to SKILL.md](../SKILL.md)
