# Function Types

_The `fn(T1, T2, ...) -> R` type for fields that hold callables._

The type `fn(T1, T2, ...) -> R` describes a callable. Use it for fields that hold callbacks.

```wcl
type Step {
  apply: fn(i32) -> i32
}
```

> [!NOTE]
> **Lazy and cycle-checked**
> Function values participate in WCL's lazy field evaluation. Each call evaluates its body in a fresh context, and circular references between fields are detected and reported.

## Related

- [Function Literals](../references/concept_function_literals.md)

- [fn Items](../references/concept_fn_items.md)

- [Higher-order Functions](../references/concept_higher_order_functions.md)

[← Back to SKILL.md](../SKILL.md)
