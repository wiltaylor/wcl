# The plan's WAD

_A browsable architecture book of the future system, derived from the plan - the steering surface during planning, the living WAD after the build._

Every scaffolded plan can carry a WAD (Wil's Architecture Document) at `wad/`: `just wad-init <id> "<Name>"` scaffolds it with `wcl init wad`, and `just wad-extract` derives `wad/data/generated/plan.wcl` from the plan itself - surfaces become screens (wireframes carried over) and API endpoints, contracts become module-API code items, models become domain objects with an ER diagram, scenarios become use cases with flowcharts, PRD requirements and rules become standards, and specs become WAD specs whose status tracks the build. The result is a twelve-chapter book of the system \*as it will exist when the plan completes\* - `just wad-book` renders it, `just wad-serve` reviews it with click-to-comment.

**Two layers, one model.** The extractor owns exactly one file (`wad/data/generated/plan.wcl`, regenerated wholesale - never hand-edit it); everything else in `wad/data/` is hand-authored and layers on top: the system/container decomposition, real personas, context relations, planned infrastructure, ADRs for decisions made while planning. The attribution tables at the top of `scripts/extract_plan.py` (SURFACE_CONTAINER / SPEC_CONTAINER / SCENARIO_PERSONA) tie the derived blocks to the hand-authored ones; anything unmapped lands under the synthetic `plan_unassigned` container and `plan_user` persona, whose presence in the book is the visible prompt to steer. Hand-authored data must never reuse plan ids or the `plan_` prefix - the extractor fails loudly on collisions.

**Graduation.** After the build completes, the WAD outlives the plan: move `wad/` to the built repo's WAD home (e.g. `.wad/`), delete `scripts/extract_plan.py` and the generated import, convert the derived blocks worth keeping into hand-authored data files (the plan stops being the source of truth once code exists), install the normal code extractors, and run the codebase-scan checklist to backfill what the plan never knew (infrastructure, build pipelines, externals). The plan's WAD becomes the system's living WAD. Build mode's completion step owns the trigger; the receiving side is the document-existing-system runbook's adopt step, which treats the scan as backfill rather than a cold start.

## Related

- [Surfaces](../references/concept_surfaces.md)

- [Data models](../references/concept_data_models.md)

- [Interface contracts and as-built notes](../references/concept_contracts.md)

- [Usage scenarios](../references/concept_scenarios.md)

- [The lessons loop](../references/concept_lessons_loop.md)

- [Document an existing system (the scan)](../references/process_documenting_existing_system.md)

- [The orchestrator loop](../references/process_proc_orchestrator_loop.md)

[← Back to SKILL.md](../SKILL.md)
