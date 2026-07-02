# if / else

_An if is an expression; branches must agree on a type, else if chains for multi-way branches._

An `if` is an expression. The branches must agree on a type. `else if` chains for multi-way branches.

```wcl
sign = if x < 0 { :neg } else if x > 0 { :pos } else { :zero }
```

## Related

- [match](../references/concept_match_expr.md)

- [if let](../references/concept_if_let.md)

[← Back to SKILL.md](../SKILL.md)
