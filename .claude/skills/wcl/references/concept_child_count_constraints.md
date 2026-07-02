# Child-count Constraints

_Constraining nested children with max_children and required_children on @block._

## Child-count constraints on @block

`@block` accepts two named arguments that constrain nested children: `max_children = N` caps the total nested-block count, and `required_children = ["kind", ...]` demands at least one child of each listed kind. Both are enforced by `wcl check`.

```wcl
@block("stage", max_children = 4, required_children = ["step"])
type Stage {
  @inline(0) name: utf8
  @children("step") steps: list<Step>
}
```

## Related

- [Block Schema](../references/concept_block_schema.md)

- [Document Schema](../references/concept_document_schema.md)

- [Referential Integrity](../references/concept_ref_integrity.md)

[← Back to SKILL.md](../SKILL.md)
