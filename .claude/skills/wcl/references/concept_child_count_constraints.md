# Child-count Constraints

_Constraining a block's contents with max_children, required_children and required_fields on @block._

## Child-count constraints on @block

`@block` accepts two named arguments that constrain nested children: `max_children = N` caps the total nested-block count, and `required_children = ["kind", ...]` demands at least one child of each listed kind. Both are enforced by `wcl check`.

```wcl
@block("stage", max_children = 4, required_children = ["step"])
type Stage {
  @inline(0) name: utf8
  @children("step") steps: list<Step>
}
```

## Required fields

`required_fields = ["name", ...]` is the same idea for fields: every listed field must be written in an instance. Declaring a field non-optional describes its \*type\*, not whether an instance must supply it — a schema that means it says so here, and `wcl check` reports a missing one as a schema violation.

```wcl
@block("stage", required_fields = ["owner"])
type Stage {
  @inline(0) name: utf8
  owner: utf8
  notes: utf8?
}

stage build { owner = "platform" }   // omitting `owner` is a violation
```

## Related

- [Block Schema](../references/concept_block_schema.md)

- [Document Schema](../references/concept_document_schema.md)

- [Referential Integrity](../references/concept_ref_integrity.md)

[← Back to SKILL.md](../SKILL.md)
