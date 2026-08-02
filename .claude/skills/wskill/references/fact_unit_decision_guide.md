# Which unit kind? — the decision guide

File every atomic note as the kind it \*is\* — not the kind that feels convenient. The most
common mistake is treating `entity` as a catch-all: entities are reserved for concrete
NAMED things (a person, a piece of software, a place, an organisation, a file format).
An idea is never an entity. A table of values is never an entity.


## The decision path

![diagram](../_wdoc/fact_unit_decision_guide-diagram-1.svg)

## By symptom

| The note is… | Kind | Test |
| --- | --- | --- |
| A way of thinking, a pattern, a mental model, an explanation of \*why\* | `concept` | You'd say "the reader must **understand** this" |
| A person, an organisation, a tool, an application, a file format, a place | `entity` | It has a proper name and you could point at it; a kind from `schema/kinds.wcl` fits |
| A default value, a limit, a table of options, a version matrix | `fact` | Nobody argues with it — you'd cite it, not explain it |
| A task someone performs: install, upgrade, review, publish | `process` | It has steps in an order and a way to verify it worked |

## Wrong vs right

| Tempting (wrong) | Correct | Why |
| --- | --- | --- |
| `entity fast_forward` for Git's fast-forward merge | `concept fast_forward` | It's a behaviour to understand, not a named thing you can point at |
| `entity default_ports` holding a port table | `fact default_ports` with a `table` body | Values belong in facts; a reference table is a fact whose body is a table |
| `concept installing` describing install steps | `process installing` with real `step`s | Steps in an order are a process — a concept can't be verified or followed |
| `fact git` describing what Git is | `entity git { kind = :software }` | A named piece of software is exactly what entities are for |
| One giant `concept overview` covering everything | Several atomic units linked via `related` | One idea per unit — split until each note holds exactly one |

## Related

- [Concept](../references/concept_concept.md)

- [Entity](../references/concept_entity.md)

- [Fact](../references/concept_fact.md)

- [Process](../references/concept_process.md)

[← Back to SKILL.md](../SKILL.md)
