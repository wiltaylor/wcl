# if let

_Binds and branches in a single step when you only care about a single variant or pattern._

`if let pattern = value { ... } else { ... }` binds and branches in a single step. Use it when you only care about a single variant or pattern.

```wcl
label = if let Shape::Circle { radius, .. } = s {
  format("circle r={}", radius)
} else {
  "other"
}
```

## Related

- [match](../references/concept_match_expr.md)

- [Patterns](../references/fact_patterns.md)

- [Optionals](../references/concept_optionals.md)

[← Back to SKILL.md](../SKILL.md)
