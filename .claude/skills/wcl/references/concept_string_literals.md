# String Literals

_UTF-8 by default; ascii/utf16/utf32 prefixes select another encoding._

## Literals

```wcl
name  = "hello"          // utf8 (default)
label = utf8"hello"      // explicit
tag   = ascii"id-007"
wide  = utf16"hello"
quad  = utf32"hello"
```

## Types

| Type | Use |
| --- | --- |
| `utf8` | Default. Variable-width Unicode in 1-4 byte sequences. |
| `ascii` | 7-bit text; one byte per char, no Unicode. |
| `utf16` | Variable-width, two bytes per BMP code unit. |
| `utf32` | Fixed-width, four bytes per code point. |

> [!NOTE]
> **Encoding is metadata**
> The encoding is part of the value's type: a utf8 field rejects an ascii literal; widen or convert at the host layer.

## Escapes

Inside a double-quoted string the usual escape sequences apply: backslash,
escaped quote, \`
`, ``, `	\`. For backslash-heavy text — regexes, LaTeX,
Windows paths — prefer a **raw heredoc** (below).


```wcl
greeting = "Hello,\nworld!"        // newline embedded
quoted   = "She said \"hi\"."
```

## Heredocs

`<<TAG ... TAG` introduces a heredoc whose body runs until a line matching the
closing tag. Escape sequences are interpreted, and indentation is stripped to
match the closing tag's indentation, so heredocs nest comfortably inside blocks.


```wcl
note = <<END
First line.
Second line.
END
```

## Raw heredocs

A **raw heredoc** uses a single-quoted opening tag and takes the body verbatim —
no escapes, no interpolation, just literal bytes. Ideal for LaTeX, regexes, code
samples with backslashes, or anything where you do not want WCL touching the
contents. Common leading whitespace is still stripped.


```wcl
regex = <<'RAW'
\d{3}-\d{4}
RAW

latex = <<'TEX'
\frac{-b \pm \sqrt{b^2 - 4ac}}{2a}
TEX
```

Opt a string into expression interpolation with a `$` prefix — see [String Interpolation](../references/concept_string_interpolation.md).

## Related

- [Strings](../references/concept_strings.md)

- [String Interpolation](../references/concept_string_interpolation.md)

- [Symbols](../references/concept_symbols.md)

[← Back to SKILL.md](../SKILL.md)
