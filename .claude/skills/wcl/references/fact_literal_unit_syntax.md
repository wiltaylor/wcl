# Literal unit syntax

A unit suffix is written **attached** to the magnitude (`5MiB`, not `5 MiB`) — the same lexical slot as a width suffix like `200u8`. Any suffix that isn't a numeric width is taken as a unit name and resolved against the field's declared type at evaluation time. The magnitude defaults to `i64` (integer) or `f64` (with a decimal point); the resolved value's type is the alias's underlying type.

```wcl
@document
type Config {
  buffer:  std.ByteSize     // base unit: byte
  radius:  std.Distance     // base unit: millimetre
  timeout: std.Duration     // base unit: nanosecond
  sizes:   list<std.ByteSize>
}

buffer  = 4MiB              // 4 * 1024 * 1024  = 4194304
radius  = 3km              // 3 * 1_000_000     = 3000000
timeout = 30s             // 30 * 1e9 (ns)     = 30000000000
sizes   = [256KiB, 1MiB]   // each element resolves
```

## Declaring units

Units live on a numeric type alias as repeated `@unit(name, factor)` decorators — the same alias-decorator mechanism as `@min` / `@max`. `factor` is the number of base units in one of that unit, and is an ordinary expression.

```wcl
@unit("B", 1)
@unit("KiB", 1024)
@unit("MiB", 1024 * 1024)
@unit("kB", 1000)            // SI: powers of 1000
@unit("MB", 1000 * 1000)
type ByteSize = i64
```

## Rules

| Form | Meaning | Example |
| --- | --- | --- |
| `<n><unit>` | Magnitude × the type's `@unit` factor | `5MiB` → `5242880` |
| float magnitude | Allowed if the product is whole for an integer type | `1.5MiB` → `1572864` |
| unknown unit | Error: the unit isn't declared on the type | `5km` on a `ByteSize` field |
| no type context | Error: nothing to resolve against | `let x = 5MiB` |

> [!NOTE]
> **Scientific notation needs a decimal point**
> `2e3` is \*not\* a float here — `e3` reads as a unit suffix. Write `2.0e3` for scientific notation (see [Number literals](../references/fact_number_literals.md)).

## Related

- [Literal Units](../references/concept_literal_units.md)

- [Numbers](../references/concept_numbers.md)

- [Number literals](../references/fact_number_literals.md)

- [Type Aliases](../references/concept_type_aliases.md)

[← Back to SKILL.md](../SKILL.md)
