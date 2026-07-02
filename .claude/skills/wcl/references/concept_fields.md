# Fields

_A field binds a name to a value with `=`; the value is any expression._

A field binds a name to a value with `=`. The value is any expression — a literal, a reference, a call, or arithmetic over other values.

```wcl
name    = "alpha"
count   = 3u32
enabled = true
ratio   = count / 2u32
```

Fields are the leaves of a document: everything else — [blocks](../references/concept_blocks.md), types, schema — exists to group and constrain them.

## Related

- [Blocks](../references/concept_blocks.md)

- [Records](../references/concept_records.md)

- [Document Schema](../references/concept_document_schema.md)

[← Back to SKILL.md](../SKILL.md)
