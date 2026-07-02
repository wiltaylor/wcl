# Literal Units

_Numeric suffixes like MiB / km / s that multiply a literal into its type's base unit._

A numeric literal may carry a **unit suffix** — `5MiB`, `512KiB`, `3km`, `30s` —
written attached, with no space. The value is multiplied by the unit's factor and
stored in the type's base unit, so `size: std.ByteSize = 5MiB` holds `5242880`
(bytes). Units are \*type-scoped\*: a literal resolves against the declared type of
the field or binding it is assigned to, using the `@unit(name, factor)` decorators
on that type. A unit the type doesn't declare — or a unit literal with no declared
type in context — is an error.


The syntax and the built-in unit families are their own note: [Literal unit syntax](../references/fact_literal_unit_syntax.md).

## Built-in unit types

Three unit types are always in scope, no import needed. Each is an ordinary `i64` alias carrying `@unit` decorators:

| Type | Base unit | Units |
| --- | --- | --- |
| `std.ByteSize` | byte | `B`, `KiB`/`MiB`/`GiB`/`TiB`/`PiB` (×1024ⁿ), `kB`/`MB`/`GB`/`TB` (×1000ⁿ) |
| `std.Distance` | millimetre | `mm`, `cm`, `dm`, `m`, `km` |
| `std.Duration` | nanosecond | `ns`, `us`, `ms`, `s`, `min`, `h`, `d` |

> [!NOTE]
> **Define your own**
> The mechanism is not special to `std.*`: hang `@unit(name, factor)` decorators on any numeric type alias and that type gains those units. `@unit("kg", 1000) @unit("g", 1) type Grams = i64` makes `5kg` resolve to `5000`.

## Formatting back

`format_unit(value, type, unit)` renders a stored base-unit value in a chosen unit — the inverse of resolution — looking the factor up from the type by name. `format_unit_value(value, factor, unit)` does the same with an explicit factor.

```wcl
buffer: std.ByteSize = 4MiB                              // 4194304
label  = format_unit(buffer, "std.ByteSize", "MiB")     // "4 MiB"
```

## Related

- [Numbers](../references/concept_numbers.md)

- [Number literals](../references/fact_number_literals.md)

- [Type Aliases](../references/concept_type_aliases.md)

- [Type & field constraints](../references/fact_type_constraints.md)

[← Back to SKILL.md](../SKILL.md)
