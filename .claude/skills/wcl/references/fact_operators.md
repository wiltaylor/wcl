# Operators

WCL's operator set: arithmetic, comparison, logic, none-coalescing, and member access. Precedence runs unary tightest, then `*` `/` `%`, then `+` `-`, then comparison, then `&&`, then `||`, then `??` loosest.

| Operator | Meaning | Example |
| --- | --- | --- |
| `+` `-` `*` `/` `%` | Add, subtract, multiply, divide, remainder | `1 + 2 * 3` |
| `^` | Exponentiation | `2 ^ 10` |
| `==` `!=` | Equality and inequality | `1u32 == 1i64` |
| `<` `<=` `>` `>=` | Ordering comparisons | `age >= 18` |
| `&&` `\|\|` `!` | Logical and, or, not | `a && !b` |
| `??` | None-coalescing — left value unless it is `none` | `box.width ?? 480.0` |
| `.` | Member access — read a field by name | `service.metadata.region` |
| `[]` | Index access into a list or composite | `config.services[0]` |

## Related

- [Optionals](../references/concept_optionals.md)

- [References](../references/concept_references.md)

[← Back to SKILL.md](../SKILL.md)
