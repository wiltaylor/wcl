# Strings

_UTF-8 by default, with encoding prefixes, heredocs, and opt-in interpolation._

String literals are UTF-8 by default. A prefix selects another encoding; heredocs
offer multi-line and verbatim variants; a `$` prefix opts the string into
expression interpolation.


## Literals

```wcl
name  = "hello"          // utf8 (default)
label = utf8"hello"      // explicit
tag   = ascii"id-007"
wide  = utf16"hello"
quad  = utf32"hello"
```

## Escapes

Inside a double-quoted string the usual escape sequences apply: backslash,
escaped quote, \`
`, ``, `	\`. For backslash-heavy text — regexes, LaTeX,
Windows paths — prefer a **raw heredoc** (below).


```wcl
greeting = "Hello,\nworld!"        // newline embedded
quoted   = "She said \"hi\"."
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

## Plain heredocs

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

## Interpolation

Interpolation is opt-in: prefix the string literal with `$`. Inside a `$"..."`
(or `$<<TAG`), `${ ... }` slots evaluate any expression and splice the result
into the string. Without the prefix `${...}` is literal text. All four encodings
accept the `$` prefix.


```wcl
greeting = $"Hello, ${name}! Count: ${count + 1u32}"

block = $<<MSG
You have ${len(items)} items waiting.
The first is ${head(items)}.
MSG
```

## String helpers

The string builtins cover searching (`contains`, `starts_with`, `ends_with`),
splitting and joining (`split`, `join`), reshaping (`replace`, `to_upper`,
`to_lower`, `trim`, `concat`, `repeat`, `pad_start`, `pad_end`), and slicing
(`chars`, `slice`, `len` — all character-indexed). See [Expressions](../references/concept_expressions.md).


```wcl
letters = chars("abc")            // ["a", "b", "c"]
padded  = pad_start("7", 3, "0")  // "007"
ruler   = repeat("-=", 4)         // "-=-=-=-="
prefix  = slice("hello", 0, 2)    // "he"
```

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

- [Expressions](../references/concept_expressions.md)

- [Symbols](../references/concept_symbols.md)

[← All concepts](../references/concepts_ref.md)
