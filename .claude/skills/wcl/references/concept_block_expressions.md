# Block Expressions

_A { ... } expression holds let bindings followed by a tail expression, which is the block's value._

A `{ ... }` expression holds zero or more `let ... ;` bindings followed by a tail expression, which is the block's value.

```wcl
result = {
  let a = to_upper("x");
  let b = to_upper("y");
  len(a) + len(b)
}
```

## Related

- [let bindings](../references/concept_let_bindings.md)

[← Back to SKILL.md](../SKILL.md)
