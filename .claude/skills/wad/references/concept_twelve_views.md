# The twelve views

_The fixed chapter set of every WAD book, and which block families feed each._

Every WAD renders the same twelve chapters, so a reader always knows where a fact lives. When filing data, pick the view first:

| # | View | Answers | Block families |
| --- | --- | --- | --- |
| 1 | Overview | What is this, who cares, what was decided? | `wad`, `stakeholder`, `raci_entry`, `adr` |
| 2 | System context | How does the estate sit in the world? | `relation` (the diagram derives) |
| 3 | External systems | What third parties does it \*integrate with\* to do its job? (Platforms it merely runs/ships on are view 5.) | `external_system` (+ contacts, endpoints, api_refs, security notes) |
| 4 | Systems | What are we made of? (C4 drill-down) | `system`, `container`, `component`, `cli_command`, `code_item`, `screen` |
| 5 | Infrastructure | Where does it run, per environment? | `environment`, `infra_node`, `deploy_target` |
| 6 | Build & deploy | How does a change become a running system? | `repository`, `pipeline` (+ stages), `release` |
| 7 | Documentation | What does the project publish to be read, and who serves it? | `doc_item` |
| 8 | User personas | Who \*uses\* it, how are they gated, what do they typically do? | `persona` (+ access grants), `use_case` |
| 9 | System admin | How is it operated? | `sop` (+ steps and flow) |
| 10 | Standards & rules | What is the code/business held to? | `standard` (+ rules) |
| 11 | Domain | What is the model made of? | `domain_object` (+ fields, relations), `term` |
| 12 | Specs | What changes are in flight? | `spec` (+ changes) |

Freeform topics that fit no family become `article` blocks, filed under a view with `section = :overview` (etc.).

**Documentation is an output, not a system.** A docs site, book, or skill folder is never a `system`/`container` in view 4 — it files as a `doc_item` in the documentation view, recording where it's authored, what builds it, where it's hosted, and which personas it serves.

## Related

- [What a WAD is](../references/concept_what_is_a_wad.md)

- [WAD folder layout](../references/fact_wad_layout.md)

[← Back to SKILL.md](../SKILL.md)
