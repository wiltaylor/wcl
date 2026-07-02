# Strings

_UTF-8 by default, with encoding prefixes, heredocs, and opt-in interpolation._

String literals are UTF-8 by default. A prefix selects another encoding; heredocs
offer multi-line and verbatim variants; a `$` prefix opts the string into
expression interpolation. String builtins (split, join, replace, contains, slice,
and so on) are covered in the String functions reference.


- [String Literals](../references/concept_string_literals.md) — UTF-8 by default plus `ascii`/`utf16`/`utf32` prefixes, backslash escapes, plain and raw heredocs.

- [String Interpolation](../references/concept_string_interpolation.md) — opt-in `$` prefix with `${ ... }` slots.

## Examples

### Raw heredoc for backslash-heavy text

A single-quoted tag takes the body verbatim — no escapes, no interpolation.

```wcl
regex = <<'RAW'
\d{3}-\d{4}
RAW

latex = <<'TEX'
\frac{-b \pm \sqrt{b^2 - 4ac}}{2a}
TEX
```

**Expected:** `regex` and `latex` hold the literal bytes, backslashes intact.

## Related

- [String Literals](../references/concept_string_literals.md)

- [String Interpolation](../references/concept_string_interpolation.md)

- [Symbols](../references/concept_symbols.md)

[← Back to SKILL.md](../SKILL.md)
