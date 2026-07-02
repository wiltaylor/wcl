# Numbers

_Fixed-width signed and unsigned integers plus two float widths, with literal suffixes._

WCL has fixed-width signed and unsigned integers plus two float widths. A bare
integer literal defaults to `i64`; a suffix pins the exact type. Underscores may
group digits for readability and are ignored. See [Numeric promotion](../references/concept_numeric_promotion.md)
for how mixed operands widen to a common type.


Literal syntax — radixes, width suffixes, and digit-grouping underscores — is its own note: [Number literals](../references/fact_number_literals.md).

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

## Related

- [Lists](../references/concept_lists.md)

- [Numeric Promotion](../references/concept_numeric_promotion.md)

[← Back to SKILL.md](../SKILL.md)
