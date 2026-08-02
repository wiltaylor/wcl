# Process

_A unit (authored as a procedure) that captures the reliable sequence for doing a task — ordered steps, for someone who already knows the topic._

A process — authored as a `procedure` block — is the unit for doing a specific task
reliably. It is written for someone who already knows the topic and needs the
sequence, not a newcomer learning it (that is a tutorial). Unlike concept, entity and
fact it keeps a structured shape rather than a free body: a `purpose`,
`preconditions`, `step` children, and a `verification`. Each step has an `id`, and the
author wires the steps together with `from -> to` flow statements (\`scaffold ->
set_topic`, or `valid -> fix :no\` to label a branch) — so a process can branch, not
just run straight down, and a step can be a `process` box, a `decision` diamond or a
`terminator`. Each process renders to its own `process_<id>` page as a flow chart of
its steps (built from those flow statements) above the step detail, and cross-links to
the concepts, entities and facts it touches.


## Related

- [Concept](../references/concept_concept.md) — Concept supports Process: A unit that captures an idea or mental model of something.

- [Entity](../references/concept_entity.md) — Entity supports Process: A concrete NAMED thing in the topic's world — a person, software, a place, an organisation. Reserved: never a catch-all.

- [Fact](../references/concept_fact.md) — Fact supports Process: A unit that holds factual data — a value, a constant, a value table.

[← Back to SKILL.md](../SKILL.md)
