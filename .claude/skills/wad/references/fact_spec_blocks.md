# Spec blocks & the status lifecycle

A `spec` is a work package: the implementation detail an AI coding agent needs to build part
of the system. `wcl wad spec --from <rev>` can seed one mechanically (baseline sha + exact
change list + TODO intent fields); the intent — the typed `context`, ordered `instructions`,
and checkable `acceptance` — is authored by the LLM/human decomposing the change. The `body`
is for extras (diagrams, long-form rationale) the typed fields don't carry.


| Block | Fields | Notes |
| --- | --- | --- |
| `spec` | `title`, `status`, `created`, `updated?`, `owner?`, `summary`, `context?`, `instructions[]`, `acceptance[]`, `from_rev?`, `affected[]`, `supersedes?`, `body?` | `affected` holds system/container ids; `from_rev` records the baseline sha when the spec was seeded from a diff; the spec page renders Context, a numbered Instructions list, and an Acceptance checklist from the typed fields |
| `change` | entity key as inline label, `op`, `value?` | mechanical change list rows — `op` is `:added` / `:removed` / `:modified`; `value` is the rendered WCL of an added/removed entity |
| `field_change` | path as inline label, `kind`, `old?`, `new?` | nested in `change`; `old`/`new` carry rendered WCL text |

| Status | Meaning | Who moves it |
| --- | --- | --- |
| :planning | skeleton written, intent being authored/reviewed | author |
| :in_progress | an implementer picked it up | implementer |
| :blocked | an implementer started and cannot proceed — record why in the body | implementer |
| :complete | landed and confirmed by the user | author, after user confirmation |
| :abandoned | won't happen — revert the data or keep it with an ADR explaining | author |

[← Back to SKILL.md](../SKILL.md)
