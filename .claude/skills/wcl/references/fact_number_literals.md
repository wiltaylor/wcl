# Number literals

An integer literal may be written in decimal, hexadecimal (`0x`), octal (`0o`), or binary (`0b`); floats accept scientific notation (`1e6`). A width suffix pins the exact numeric type, and underscores may group digits for readability (they are ignored). A non-numeric suffix is a [literal unit](../references/concept_literal_units.md) (`5MiB`) instead.

```wcl
a = 42          // i64 (default)
b = 200u8       // unsigned 8-bit
c = 9_000i64    // underscores are ignored
d = 3.14f64     // float
f = 0xFFu32     // hex
g = 0b1010_1100u8   // binary
h = 0o755u16    // octal
```

| Form | Meaning | Example |
| --- | --- | --- |
| `0x…` | Hexadecimal integer | `0xFFu32` |
| `0o…` | Octal integer | `0o755u16` |
| `0b…` | Binary integer | `0b1010_1100u8` |
| `…e…` | Scientific notation (float) | `1e6` |
| `…u32` `…i64` | Unsigned/signed integer width suffix | `200u8`, `10i64` |
| `…f32` `…f64` | Float width suffix | `3.14f64` |

## Related

- [Numbers](../references/concept_numbers.md)

- [Numeric Promotion](../references/concept_numeric_promotion.md)

- [Literal Units](../references/concept_literal_units.md)

[← Back to SKILL.md](../SKILL.md)
