# Numeric Promotion

_Arithmetic and comparison widen mixed numeric operands to a common type._

## Numeric promotion

Arithmetic and comparison widen mixed numeric operands to a common type, so cross-width and integer/float mixing work without explicit casts.

```wcl
a = 1 + 2.0        // i64 widened to f64 -> 3.0
b = 1u32 == 1i64   // true
c = 3.0 * 2u8      // 6.0
```

## Related

- [Numbers](../references/concept_numbers.md)

[← Back to SKILL.md](../SKILL.md)
