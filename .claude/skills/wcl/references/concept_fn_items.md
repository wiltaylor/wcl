# fn Items

_The `fn` item form — a reusable named function declaration._

A reusable function is declared once with the `fn` item form — sugar for `let name = fn(...)` with two extras: the name shows up in editor tooling and it can carry a `@doc` decorator. Like a `let`, an `fn` item is a composition helper, not data.

```wcl
@doc("Clamp a value into [lo, hi].")
fn clamp_to(x: f64, lo: f64, hi: f64) -> f64 {
  min(max(x, lo), hi)
}

gain = clamp_to(raw_gain, 0.0, 1.0)
```

## Related

- [Function Literals](../references/concept_function_literals.md)

- [Higher-order Functions](../references/concept_higher_order_functions.md)

- [Function Types](../references/concept_function_types.md)

[← Back to SKILL.md](../SKILL.md)
