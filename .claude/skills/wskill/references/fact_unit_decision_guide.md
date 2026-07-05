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
| A dated finding from an investigation — versions, API specifics, a verified gotcha | `research` | It answers a question you investigated and could go stale when the subject moves |
| A person, an organisation, a tool, an application, a file format, a place | `entity` | It has a proper name and you could point at it; a kind from `schema/kinds.wcl` fits |
| A default value, a limit, a table of options, a version matrix | `fact` | Nobody argues with it — you'd cite it, not explain it |
| A task someone performs: install, upgrade, review, publish | `process` | It has steps in an order and a way to verify it worked |
| A curated grouping that arranges other units | `index` | It holds no knowledge of its own — only `related` links |
| One word or phrase and its meaning | `term` | One sentence covers it; it needs no page of its own |
| Code or commands that illustrate another unit | `example` | It makes no sense without the unit it belongs to (`unit = <id>`) |

## Wrong vs right

| Tempting (wrong) | Correct | Why |
| --- | --- | --- |
| `entity fast_forward` for Git's fast-forward merge | `concept fast_forward` | It's a behaviour to understand, not a named thing you can point at |
| `entity default_ports` holding a port table | `fact default_ports` with a `table` body | Values belong in facts; a reference table is a fact whose body is a table |
| `concept installing` describing install steps | `process installing` with real `step`s | Steps in an order are a process — a concept can't be verified or followed |
| `fact git` describing what Git is | `entity git { kind = :software }` | A named piece of software is exactly what entities are for |
| One giant `concept overview` covering everything | Several atomic units linked via `related` | One idea per unit — split until each note holds exactly one |

Two follow-up rules. First: when a note seems to be two kinds at once, it is two notes —
split it and link them with `related`. Second: `kind` on an entity comes from the closed
vocabulary in `schema/kinds.wcl`; if no kind fits and you cannot justify adding one, that
is the format telling you the note is not an entity.


## Examples

### Misfiled as an entity — and the fix

The classic mistake: an idea filed as an entity. The kind field has no honest answer, which is the tell.

```wcl
// WRONG — a behaviour isn't a named thing; no EntityKind fits:
entity fast_forward {
  name = "Fast-forward merge"
  kind = :software            // a lie — and the closest kind available
}

// RIGHT — it's an idea to understand:
concept fast_forward {
  name    = "Fast-forward merge"
  summary = "Advancing a branch pointer when history hasn't diverged."
  body { p "When the target already contains the source's history, Git just moves the pointer." }
}
```

**Expected:** The wrong form either fails wcl check (no plausible kind) or reads as a lie; the concept form checks clean and files where readers look for ideas.

## Related

- [Decomposing information](../references/concept_decomposing_information.md)

- [Concept](../references/concept_concept.md)

- [Entity](../references/concept_entity.md)

- [Fact](../references/concept_fact.md)

- [Process](../references/concept_process.md)

- [Atomic Note](../references/concept_atomic_note.md)

- [Capturing research into a wskill](../references/process_capturing_research.md)

[← Back to SKILL.md](../SKILL.md)
