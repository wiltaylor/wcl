# Function Literals

_Inline `fn(params) -> Return body` function values._

`fn(params) -> Return body`. The body is a single expression, or a `{ ... }` block expression.

```wcl
double = fn(x: i32) -> i32 x * 2
sum_sq = fn(x: i32, y: i32) -> i32 { let s = x + y; s * s }
```

Bind a literal with `let` (or to a field), then call it like any other function:

```wcl
let double = fn(x: i32) -> i32 x * 2
let sum_sq = fn(x: i32, y: i32) -> i32 { let s = x + y; s * s }

four = double(2i32)            // 4
nine = sum_sq(1i32, 2i32)      // (1 + 2)^2 = 9
```

## Related

- [fn Items](../references/concept_fn_items.md)

- [Higher-order Functions](../references/concept_higher_order_functions.md)

- [Function Types](../references/concept_function_types.md)

[← Back to SKILL.md](../SKILL.md)
