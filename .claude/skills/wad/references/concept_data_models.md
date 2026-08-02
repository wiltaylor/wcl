# Data models

_Every stored or shared entity as a typed contract - fields, validation, persistence - defined by exactly one spec and copied into every brief that touches it._

The same failure as screens, one layer down: nothing enumerates the entities, so two specs
touching tasks invent incompatible representations and weak agents invent schemas freely. A
`model` block fixes each entity: fields with types and validation, persistence (where and how
stored), and relations to other models.


Exactly one spec `defines_models` each model (models_defined_once gate) and owns its
implementation; any other spec that reads or writes it lists it in `uses_models`. Both get the
full definition in their brief - the definer marked 'you implement this', users marked 'you
use this exactly as defined'. Define models during the PRD phase alongside surfaces; research
often settles persistence details, so refine after research like surfaces.


## Related

- [Surfaces](../references/concept_surfaces.md) — Surfaces supports Data models: Every screen, command and endpoint is a typed contract - elements, required states, interactions - defined during the PRD and delivered by exactly one spec.

[← Back to SKILL.md](../SKILL.md)
