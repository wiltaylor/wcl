# @only / @except

`@only` and `@except` restrict which kinds (or fields) a slot accepts. `@only` is an include-list — nothing outside it is allowed — while `@except` is an exclude-list that admits everything else.

```wcl
@block("layout") type Layout {
  @only("header", "footer")
  @children("section") fixed: list<Section>

  @except("draft")
  @children("section") published: list<Section>
}
```

## Related

- [@children](../references/fact_dec_children.md)

- [Block Schema](../references/concept_block_schema.md)

[← Back to SKILL.md](../SKILL.md)
