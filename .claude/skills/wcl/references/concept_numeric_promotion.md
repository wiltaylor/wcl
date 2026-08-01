# Numeric Promotion

_Arithmetic and comparison widen mixed numeric operands to a common type._

## Numeric promotion

Arithmetic and comparison widen mixed numeric operands to a common type, so cross-width and integer/float mixing work without explicit casts.

```wcl
a = 1 + 2.0        // i64 widened to f64 -> 3.0
b = 1u32 == 1i64   // true
c = 3.0 * 2u8      // 6.0
```

## When there is no answer

Same-typed integer operands keep their own type, so an operation whose result will not fit is an evaluation error rather than a wrapped or truncated value. Integer / and % by zero are errors too; floats keep IEEE semantics and yield inf or NaN.

```wcl
a = 4 / 0          // error: operator '/' cannot divide by zero
b = 127i8 + 1i8    // error: operator '+' cannot represent the result in i8 (overflow)
c = 4.0 / 0.0      // inf — floats are unaffected
d = try 4 / 0 catch e { 0 }   // recoverable like any evaluation error
```

## Related

- [Numbers](../references/concept_numbers.md)

[← Back to SKILL.md](../SKILL.md)
