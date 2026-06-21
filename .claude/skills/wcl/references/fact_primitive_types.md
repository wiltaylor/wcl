# WCL primitive types

WCL's primitive (scalar) types: variable-width and fixed-width integers, two float widths, the two text-like scalars, and the boolean. A bare integer literal defaults to `i64`; a suffix pins the exact width.

| Type | Description | Example literal |
| --- | --- | --- |
| `utf8` | Variable-width Unicode text (the default string type) | `"hello"` |
| `bool` | Boolean truth value | `true` |
| `i8` `i16` `i32` `i64` | Signed integers of 8/16/32/64 bits (`i64` is the default for a bare literal) | `-120i8` |
| `u8` `u16` `u32` `u64` | Unsigned integers of 8/16/32/64 bits | `200u8` |
| `f32` `f64` | 32-bit and 64-bit floating point | `3.14f64` |
| `identifier` | A bare name used as a block label or id | `web` |
| `symbol` | An identifier-like tag value written with a leading colon | `:amber` |

> [!NOTE]
> **Wider integers exist too**
> WCL also has `i128`/`u128` and the pointer-width `isize`/`usize`, but `i8`-`i64` and `u8`-`u64` cover almost all schema work.

## Related

- [Numbers](../references/concept_numbers.md)

- [Strings](../references/concept_strings.md)

- [Booleans](../references/concept_booleans.md)

- [Symbols](../references/concept_symbols.md)

- [Identifiers](../references/concept_identifiers.md)

[← All facts](../references/facts_ref.md)
