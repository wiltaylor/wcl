# Math builtins

Numeric builtins cover sign and rounding, roots and powers, logarithms, and trigonometry. Most return an `f64`.

| Function | Use |
| --- | --- |
| `abs(x)` | Absolute value |
| `min(a, b)` / `max(a, b)` | Smaller / larger of two numbers |
| `floor(x)` / `ceil(x)` | Round down / round up to a whole number |
| `round(x)` | Round to the nearest whole number |
| `trunc(x)` | Drop the fractional part |
| `sqrt(x)` / `cbrt(x)` | Square root / cube root |
| `pow(b, e)` | `b` raised to the power `e` |
| `exp(x)` | `e` raised to the power `x` |
| `ln(x)` / `log2(x)` / `log10(x)` | Natural, base-2, and base-10 logarithm |
| `sin(x)` / `cos(x)` / `tan(x)` | Trigonometric functions (radians) |
| `clamp(x, lo, hi)` | Constrain `x` to the range `[lo, hi]` |
| `sign(x)` | -1, 0, or 1 by sign of `x` |
| `hypot(a, b)` | The hypotenuse — `sqrt(a*a + b*b)` |

## Related

- [Numbers](../references/concept_numbers.md)

- [Expressions](../references/concept_expressions.md)

[← All facts](../references/facts_ref.md)
