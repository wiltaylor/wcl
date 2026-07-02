# Structured data

_Information captured in a defined format — a schema — so every instance is uniform and the templates can read and project it reliably._

Structured data is information that follows a defined format — a schema — rather than
free-form prose. The schema says what fields a piece of information has and what each one
means, so every instance is captured the same way and a template can read it and render it
reliably.


A wskill is built entirely from structured data. The unit kinds — `concept`, `entity`, `fact`
and `process` — are themselves schemas, and a topic's knowledge is encoded into those formats.
When a topic has a shape that recurs and the four built-in kinds don't fit, you create a new
schema for it (a typed block in `schema/extensions.wcl`); the new format is then captured and
projected exactly like the built-in ones.


## Related

- [Separation of Data and Presentation](../references/concept_datapresentation.md)

- [Decomposing information](../references/concept_decomposing_information.md)

- [Concept](../references/concept_concept.md)

- [Entity](../references/concept_entity.md)

- [Fact](../references/concept_fact.md)

- [Process](../references/concept_process.md)

[← Back to SKILL.md](../SKILL.md)
