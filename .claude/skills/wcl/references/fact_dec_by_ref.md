# @by_ref

`@by_ref` makes a block of this kind reify to a **resolvable reference** instead of inlining its body at the use site. The value is carried on a record and rendered (or resolved) elsewhere, which keeps cross-cutting definitions in one place.

```wcl
@by_ref @block("color") type Color {
  @inline(0) name: identifier
  hex: utf8
}

color brand { hex = "#3366ff" }

@block("box") type Box {
  fill: &Color   // a reference, not an inlined copy
}
```

## Related

- [References](../references/concept_references.md)

- [@block](../references/fact_dec_block.md)

[← Back to SKILL.md](../SKILL.md)
