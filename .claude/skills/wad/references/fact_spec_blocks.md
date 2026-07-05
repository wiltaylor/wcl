# Spec blocks & the status lifecycle

A `spec` is a work package: the implementation detail an AI coding agent needs to build part of the system. `wcl wad spec --from <rev>` can seed one mechanically (baseline sha + exact change list); the intent — summary, rationale, ordered instructions — is authored by the LLM/human decomposing the change.

| Block | Fields | Notes |
| --- | --- | --- |
| `spec` | `title`, `status`, `created`, `updated?`, `owner?`, `summary`, `from_rev?`, `affected[]`, `supersedes?`, `body?` | `affected` holds system/container ids; `from_rev` records the baseline sha when the spec was seeded from a diff |
| `change` | entity key as inline label, `op`, `value?` | mechanical change list rows — `op` is `:added` / `:removed` / `:modified`; `value` is the rendered WCL of an added/removed entity |
| `field_change` | path as inline label, `kind`, `old?`, `new?` | nested in `change`; `old`/`new` carry rendered WCL text |

| Status | Meaning | Who moves it |
| --- | --- | --- |
| :planning | skeleton written, intent being authored/reviewed | author |
| :in_progress | an implementer picked it up | implementer |
| :complete | landed and confirmed by the user | author, after user confirmation |
| :abandoned | won't happen — revert the data or keep it with an ADR explaining | author |

## Related

- [Specs and the change workflow](../references/concept_spec_lifecycle.md)

[← Back to SKILL.md](../SKILL.md)
