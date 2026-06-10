# Strings

String literals are UTF-8 by default. A prefix selects another encoding; heredocs offer multi-line and verbatim variants; a `$` prefix opts the string into expression interpolation.

## Literals

```wcl
name  = "hello"          // utf8 (default)
label = utf8"hello"      // explicit
tag   = ascii"id-007"
wide  = utf16"hello"
quad  = utf32"hello"
```

## Escapes

Inside a double-quoted string, the usual escape sequences apply: `\\`, `\"`, `\n`, `\r`, `\t`. For backslash-heavy text — regexes, LaTeX, Windows paths — prefer a **raw heredoc** (below).

```wcl
greeting = "Hello,\nworld!"        // newline embedded
quoted   = "She said \"hi\"."
```

## Types

| Type | Use |
| --- | --- |
| `utf8` | Default. Variable-width Unicode in 1–4 byte sequences. |
| `ascii` | 7-bit text; fits one byte per char, no Unicode. |
| `utf16` | Variable-width, two bytes per BMP code unit. |
| `utf32` | Fixed-width, four bytes per code point. |

> [!NOTE]
> **Encoding is metadata**
> The encoding is part of the value's type. A field declared utf8 will reject an ascii literal; widen or convert at the host layer.

## Plain heredocs

`<<TAG ... TAG` introduces a heredoc. The body runs until a line matching the closing tag. Escape sequences (`\n`, `\"`, …) are interpreted. Indentation is stripped to match the closing tag's indentation, so heredocs nest comfortably inside indented blocks.

```wcl
note = <<END
First line.
Second line.
END
```

## Raw heredocs

A **raw heredoc** uses a single-quoted opening tag and takes the body verbatim — no escapes, no interpolation, just literal bytes. Ideal for LaTeX, regexes, code samples with backslashes, or anything where you don't want WCL touching the contents.

```wcl
regex = <<'RAW'
\d{3}-\d{4}
RAW

latex = <<'TEX'
\frac{-b \pm \sqrt{b^2 - 4ac}}{2a}
TEX
```

> [!TIP]
> **Indentation stripping still applies**
> Even in a raw heredoc, the common leading whitespace (matching the closing tag's indent) is stripped from every line. Both forms behave the same way here — only escape handling differs.

## Interpolation

Interpolation is opt-in: prefix the string literal with `$`. Inside a `$"…"` (or `$<<TAG`), `${ … }` slots evaluate any expression and splice the result into the string. Without the prefix, `${…}` is literal text — no hidden interpolation.

```wcl
greeting = $"Hello, ${name}! Count: ${count + 1u32}"

block = $<<MSG
You have ${len(items)} items waiting.
The first is ${head(items)}.
MSG
```

All four encodings accept the `$` prefix (`$"…"` / `$ascii"…"` / `$utf16"…"` / `$utf32"…"`).

## String helpers

The string builtins cover searching (`contains`, `starts_with`, `ends_with`), splitting and joining (`split`, `join`), reshaping (`replace`, `to_upper`, `to_lower`, `trim`, `concat`, `repeat`, `pad_start`, `pad_end`), and slicing (`chars`, `slice`, `len` — all character-indexed, not byte-indexed). See [Builtin Functions](../references/builtins.md) for signatures.

```wcl
letters = chars("abc")            // ["a", "b", "c"]
padded  = pad_start("7", 3, "0")  // "007"
ruler   = repeat("-=", 4)         // "-=-=-=-="
prefix  = slice("hello", 0, 2)    // "he"
```
