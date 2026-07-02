# Symbol Sets

_A symbol_set names a closed vocabulary of symbols — an enum-like type._

## Symbol sets

A `symbol_set` names a **closed vocabulary** of symbols. A field typed by the symbol set is restricted to exactly those members, giving an enum-like type without the weight of a union.

```wcl
symbol_set Color {
  red
  green
  blue
}

type Paint {
  shade: Color     // only :red, :green, or :blue
}

cream = Paint { shade: :red }
```

Use a symbol set wherever you would reach for an enum in another language: severity levels, edge kinds, named layout modes, palette hues.

## Related

- [Symbols](../references/concept_symbols.md)

- [Unions](../references/concept_unions.md)

[← Back to SKILL.md](../SKILL.md)
