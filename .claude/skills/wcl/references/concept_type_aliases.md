# Type Aliases

_type Name = TypeRef declares a transparent alias; constraint decorators travel to every field using it._

## Type aliases

`type Name = TypeRef` declares a transparent alias — a readable name for any type, resolved wherever the name is used (transitively). Constraint decorators on the alias travel with it: every field declared with the alias is validated by `wcl check`.

```wcl
@min(1) @max(65535)
type Port = u16

@non_empty
type Name = utf8

type Service {
  name: Name        # rejects ""
  port: Port        # rejects 0u16 and 70000-ish values
}
```

## Related

- [Records](../references/concept_records.md)

- [Type & field constraints](../references/fact_type_constraints.md)

[← Back to SKILL.md](../SKILL.md)
