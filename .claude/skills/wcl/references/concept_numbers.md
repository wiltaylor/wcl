# Numbers

_Fixed-width signed and unsigned integers plus two float widths, with literal suffixes._

WCL has fixed-width signed and unsigned integers plus two float widths. A bare
integer literal defaults to `i64`; a suffix pins the exact type. Underscores may
group digits for readability and are ignored. See [Expressions](../references/concept_expressions.md)
for numeric promotion across mixed operands.


## Literals

```wcl
a = 42          // i64 (default)
b = 200u8       // unsigned 8-bit
c = 9_000i64    // underscores are ignored
d = 3.14f64     // float
e = -120i8      // signed
f = 0xFFu32     // hex
g = 0b1010_1100u8   // binary
h = 0o755u16    // octal
```

## Types

| Width | Signed | Unsigned | Float |
| --- | --- | --- | --- |
| 8-bit | `i8` | `u8` | — |
| 16-bit | `i16` | `u16` | — |
| 32-bit | `i32` | `u32` | `f32` |
| 64-bit | `i64` | `u64` | `f64` |
| 128-bit | `i128` | `u128` | — |
| platform default size | `isize` | `usize` | — |

The platform-default size (`isize` / `usize`) is the pointer width of the machine WCL runs on — 64-bit on most desktops/servers, 32-bit on smaller targets — so its exact width changes depending on the platform.

## Numeric promotion

Arithmetic and comparison widen mixed numeric operands to a common type, so cross-width and integer/float mixing work without explicit casts.

```wcl
a = 1 + 2.0        // i64 widened to f64 -> 3.0
b = 1u32 == 1i64   // true
c = 3.0 * 2u8      // 6.0
```

## Related

- [Expressions](../references/concept_expressions.md)

- [Lists](../references/concept_lists.md)

[← All concepts](../references/concepts_ref.md)
