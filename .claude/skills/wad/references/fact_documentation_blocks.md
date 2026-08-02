# Documentation blocks

> [!WARNING]
> **Outputs, not systems**
>
> A docs site, a rendered book, a skill folder, a README — anything the project publishes \*to be read\* — is a **documentation artefact**, never a `system` or `container` in view 4. Test: does it execute, or is it read? Read → `doc_item`. Where a published artefact is hosted goes in its `hosted_on` field, not in a system of its own.

| Block | Fields | Notes |
| --- | --- | --- |
| `doc_item` | `name`, `summary`, `kind`, `source?`, `url?`, `built_by?`, `hosted_on?`, `audience[]`, `body?` | `kind` is a DocKind (site / book / skill / reference / readme / guide / changelog); `built_by` names the container that renders it; `audience` holds persona ids |

Who populates: hand — documentation is small enough to enumerate, and the fields force the
useful questions (where's the source? what builds it? where is it hosted? who is it for?).


[← Back to SKILL.md](../SKILL.md)
