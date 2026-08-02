# Fields

_A field binds a name to a value with `=`; the value is any expression._

A field binds a name to a value with `=`. The value is any expression — a literal, a
reference, a call, or arithmetic over other values.


```wcl
name    = "alpha"
count   = 3u32
enabled = true
ratio   = count / 2u32
```

Fields are the leaves of a document: everything else — [blocks](../references/concept_blocks.md), types, schema
— exists to group and constrain them.


## Related

- [Blocks](../references/concept_blocks.md) — Blocks supports Fields: A named group of fields that can also nest other blocks; the schema sets its labels and fields.

[← Back to SKILL.md](../SKILL.md)
