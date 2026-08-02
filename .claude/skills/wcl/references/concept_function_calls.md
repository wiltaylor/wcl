# Function Calls

_Parentheses call a function or constructor with lazily-evaluated arguments._

Parentheses call a function — a builtin, a `fn` item, or a function-valued binding. Arguments
are expressions, evaluated lazily; the call evaluates the body in a fresh context. Calls nest,
and a function literal may be passed inline.


```wcl
@document
type Doc {
  n:     i64
  total: i64
}

let items = [1, 2, 3]

n     = len(items)
total = sum(map(items, fn(x: i64) -> i64 x * 2))
```

Parentheses also carry the payload of a [union](../references/concept_unions.md) typeref variant —
`Shape::Polygon(7)`. Records have no constructor call: a record value is a bare
`{ field: value }` literal typed by its position.


[← Back to SKILL.md](../SKILL.md)
