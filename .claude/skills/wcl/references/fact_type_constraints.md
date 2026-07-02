# Type & field constraints

Constraint decorators bound a field's value, and `wcl check` enforces them. They also attach to a **type alias** (`type Name = TypeRef`), in which case every field declared with that alias is validated against them — the constraint travels with the name.

| Decorator | Constraint |
| --- | --- |
| `@min(n)` | Numeric value must be at least `n` |
| `@max(n)` | Numeric value must be at most `n` |
| `@non_empty` | The string or list value must not be empty |

```wcl
@min(1) @max(65535)
type Port = u16

@non_empty
type Name = utf8

type Service {
  name: Name        // rejects ""
  port: Port        // rejects 0u16 and 70000-ish values
}
```

## Related

- [Records](../references/concept_records.md)

- [Type Aliases](../references/concept_type_aliases.md)

- [@default](../references/fact_dec_default.md)

[← Back to SKILL.md](../SKILL.md)
