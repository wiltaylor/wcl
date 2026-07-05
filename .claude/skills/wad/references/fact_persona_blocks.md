# Persona blocks

> [!WARNING]
> **Users, not builders**
> A persona is a kind of **user of the system** — who it exists for. The people and agents who \*build, maintain, or operate\* it (the maintainer, contributors, an AI implementation agent, the on-call engineer) are **stakeholders** (view 1) with RACI rows, never personas. Test: would this party still exist if the system were bought instead of built? Yes → persona; no → stakeholder.

> [!WARNING]
> **Access grants are security controls**
> An `access_grant` records a **security control**: SSO, an approved signup, an admin invite — something that \*gates\* access, with an approver where one exists. Installing freely-available software is NOT access control; a system with no access control honestly has **no** access grants, not a grant that says “installs it”.

| Block | Fields | Notes |
| --- | --- | --- |
| `persona` | `name`, `summary`, `kind`, `goals[]`, `body?` | `kind` is `:human` / `:ai_agent` / `:service` — an agent or service persona is one that \*uses\* the system, not one that develops it |
| `access_grant` | `system`, `method`, `approver?` | nested in `persona` — the security control gating this user kind's access; omit entirely when there is none |
| `use_case` | `title`, `persona`, `summary?`, `preconditions[]`, `outcome?`, steps + flow, `body?` | a typical flow for one persona, rendered as a flowchart (reuses the SOP `step` / `from -> to` machinery); record a good selection per persona — they double as user-acceptance tests |

Personas are diagram nodes: relations from a persona to a container put them on the context diagram, and screens list the personas that use them (`screen.personas`). Use cases render under their persona's chapter, one page each with the flowchart above the step detail.

## Related

- [The twelve views](../references/concept_twelve_views.md)

[← Back to SKILL.md](../SKILL.md)
