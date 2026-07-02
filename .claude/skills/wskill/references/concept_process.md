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

- [Concept](../references/concept_concept.md)

- [Entity](../references/concept_entity.md)

- [Fact](../references/concept_fact.md)

[← Back to SKILL.md](../SKILL.md)
