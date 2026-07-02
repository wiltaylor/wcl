# Entity

_A concrete NAMED thing in the topic's world — a person, software, a place, an organisation. Reserved: never a catch-all._

An entity is a unit of knowledge that is a noun and an actual, named instance of something:
a person, a piece of software, a tool, an organisation, a file format, a place. If you can
point at it and it has a proper name, it can be an entity.


Entities are **reserved** for such concrete things — they are not a home for whatever
doesn't obviously fit elsewhere. An idea or behaviour is a [concept](../references/concept_concept.md); a
value or lookup table is a [fact](../references/concept_fact.md); a task with steps is a
[process](../references/concept_process.md). When you are tempted to file something as an entity, run it
through the [decision guide](../references/fact_unit_decision_guide.md) first.


Every entity carries a `kind` from the closed vocabulary in `schema/kinds.wcl`
(`:person`, `:software`, `:tool`, `:organization`, `:file_format`, …). The vocabulary is
topic-extensible — append a new kind there when your topic genuinely needs one — but if no
kind fits and none is worth adding, the note is not an entity.


An entity's body typically holds a short description plus an attribute table (name, role,
version, where to get it). Detail that is really about \*understanding\* belongs in a linked
concept, not inside the entity.


## Related

- [Concept](../references/concept_concept.md)

- [Fact](../references/concept_fact.md)

- [Process](../references/concept_process.md)

- [Which unit kind? — the decision guide](../references/fact_unit_decision_guide.md)

[← Back to SKILL.md](../SKILL.md)
