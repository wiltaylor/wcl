# Operators

WCL's complete operator set. Precedence runs unary tightest, then `*` `/` `%`, then `+` `-`,
then comparison, then `&&`, then `||`, then `??` loosest.


| Operator | Meaning | Example |
| --- | --- | --- |
| `+` `-` `*` `/` `%` | Add, subtract, multiply, divide, remainder | `1 + 2 * 3` |
| `-` (unary) | Negation | `-5` |
| `==` `!=` | Equality and inequality | `1u32 == 1u32` |
| `<` `<=` `>` `>=` | Ordering comparisons | `age >= 18` |
| `&&` `\|\|` `!` | Logical and, or, not | `a && !b` |
| `??` | None-coalescing — the left value unless it is `none` | `box.width ?? 480.0` |
| `.` | Member access — read a field by name | `service.metadata.region` |
| `::` | Qualified name — a namespaced type, kind, or union variant | `Shape::Circle` |

## There is no exponent or index operator

Two shapes that other languages spell with an operator are builtin calls in WCL, and writing
the operator is a parse error:


| Instead of | Write | Note |
| --- | --- | --- |
| `2.0 ^ 10.0` | `pow(2.0, 10.0)` | `^` is not a token |
| `items[0]` | `at(items, 0)` | `[` only opens a list literal, a tensor shape, or a table row |

## Not operators

Three symbols read like operators but are syntax, not expressions: `&T` is a
[reference type](../references/concept_references.md), `->` writes a [connection statement](../references/concept_connections.md)
or a function's return type, and `=>` separates a `match` arm from its body.


[← Back to SKILL.md](../SKILL.md)
