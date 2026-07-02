# Function Calls

_Parentheses call a function or constructor with lazily-evaluated arguments._

Parentheses call a function or constructor. Arguments are expressions, evaluated lazily; the call evaluates the body in a fresh context.

```wcl
n      = len(items)
total  = sum(map(items, fn(x: i64) -> i64 x * 2))
shape  = Point { x: 1.0, y: 2.0 }
```

## Related

- [Member Access](../references/concept_member_access.md)

[← Back to SKILL.md](../SKILL.md)
