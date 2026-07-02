# Booleans

_The bool type with exactly two values, true and false._

The `bool` type has exactly two values: `true` and `false`.

```wcl
enabled = true
strict  = false
```

## In expressions

Booleans are produced by every comparison and combined with the logical operators `&&`, `||`, and `!`. See [Operators](../references/fact_operators.md) for the full set.

```wcl
ready    = !pending && enabled
oversize = width > 100u32 || height > 100u32
```

## Related

- [Operators](../references/fact_operators.md)

- [if / else](../references/concept_if_else.md)

[← Back to SKILL.md](../SKILL.md)
