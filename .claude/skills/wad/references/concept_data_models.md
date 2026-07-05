# Data models

_Every stored or shared entity as a typed contract - fields, validation, persistence - defined by exactly one spec and copied into every brief that touches it._

The same failure as screens, one layer down: nothing enumerates the entities, so two specs touching tasks invent incompatible representations and weak agents invent schemas freely. A `model` block fixes each entity: fields with types and validation, persistence (where and how stored), and relations to other models.

Exactly one spec `defines_models` each model (models_defined_once gate) and owns its implementation; any other spec that reads or writes it lists it in `uses_models`. Both get the full definition in their brief - the definer marked 'you implement this', users marked 'you use this exactly as defined'. Define models during the PRD phase alongside surfaces; research often settles persistence details, so refine after research like surfaces.

## Related

- [Interface contracts and as-built notes](../references/concept_contracts.md)

- [Surfaces](../references/concept_surfaces.md)

- [The thirteen default gates](../references/fact_fact_default_gates.md)

[← Back to SKILL.md](../SKILL.md)
