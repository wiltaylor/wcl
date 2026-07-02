# Symbols

_Identifier-like :name values for tags and enum-like choices, plus symbol_set vocabularies._

A **symbol** is an identifier-like value written `:name` — no space between the
colon and the name. Symbols are used for tags, enum-like choices, and any time you
want a typed name rather than a string.


```wcl
shade  = :amber
accent = :cyan
edge   = :uses
```

## The symbol type

A free-form field accepting any symbol has type `symbol`. There is no validation on which symbols may appear — anything that lexes as `:name` is valid.

```wcl
type Tag {
  name: utf8
  kind: symbol
}

t = Tag { name: "release", kind: :stable }
```

## Related

- [Unions](../references/concept_unions.md)

- [Identifiers](../references/concept_identifiers.md)

- [Symbol Sets](../references/concept_symbol_set.md)

[← Back to SKILL.md](../SKILL.md)
