# Patterns

Patterns destructure and test values in `match`, `if let`, and guard arms.

| Pattern | Matches |
| --- | --- |
| `_` | Anything (wildcard) |
| `name` | Anything, binding the value to `name` |
| `name @ inner` | Match `inner`, also bind whole as `name` |
| Literal (`42`, `"hi"`, `:red`) | Equality with a literal |
| `Union::Variant {...}` | A specific variant; `..` ignores remaining fields |
| `Union::Variant(x)` | Typeref variant, binding payload to `x` |
| `Union::Variant` | Unit variant |
| `pat1 \| pat2` | Either pattern matches |

## Related

- [match](../references/concept_match_expr.md)

- [Unions](../references/concept_unions.md)

- [Optionals](../references/concept_optionals.md)

[← Back to SKILL.md](../SKILL.md)
