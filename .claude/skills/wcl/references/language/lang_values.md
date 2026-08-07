# Values and primitives

The scalar values you write on the right of an `=`. Lists, tensors and records are in
`lang_collections.md`; the types that *constrain* these values are in `lang_types.md`.

## Numbers

WCL has fixed-width signed and unsigned integers plus two float widths.

| Width | Signed | Unsigned | Float |
| --- | --- | --- | --- |
| 8-bit | `i8` | `u8` | — |
| 16-bit | `i16` | `u16` | — |
| 32-bit | `i32` | `u32` | `f32` |
| 64-bit | `i64` | `u64` | `f64` |
| 128-bit | `i128` | `u128` | — |
| pointer width | `isize` | `usize` | — |

`isize` / `usize` are the pointer width of the machine WCL runs on. Their exact width is
therefore platform-dependent — 64-bit on most desktops and servers.

A bare integer literal defaults to **`i64`**. A bare literal with a decimal point defaults to
**`f64`**. A suffix pins the exact type.

### Literal forms

```wcl
a = 42              // i64 (default)
b = 200u8           // unsigned 8-bit
c = 9_000i64        // underscores group digits and are ignored
d = 3.14f64         // float with an explicit width
e = 2.5             // f64 (default)
f = 0xFFu32         // hexadecimal
g = 0b1010_1100u8   // binary
h = 0o755u16        // octal
i = 1.0e6           // scientific notation
j = -5              // unary minus
```

| Form | Meaning |
| --- | --- |
| `0x…` | Hexadecimal integer |
| `0o…` | Octal integer |
| `0b…` | Binary integer |
| `…e…` | Scientific notation — **needs a decimal point in the mantissa** |
| `…u8` `…u16` `…u32` `…u64` `…u128` `…usize` | Unsigned width suffix |
| `…i8` `…i16` `…i32` `…i64` `…i128` `…isize` | Signed width suffix |
| `…f32` `…f64` | Float width suffix |
| any other suffix | A **literal unit** — see below |

> **`2e3` is not scientific notation.** With no decimal point, `e3` lexes as a unit suffix and
> resolves against the field's type, which almost always fails. Write `2.0e3`.

### Numeric promotion

Arithmetic and comparison widen mixed operands to a common type, so cross-width and
integer/float mixing need no cast.

```wcl
a = 1 + 2.0        // i64 widened to f64 → 3.0
b = 1u32 == 1i64   // true
c = 3.0 * 2u8      // 6.0
```

Promotion runs **before** the operation, so WCL judges overflow against the type the operands
end up sharing:

```wcl
a = 127i8 + 1i8    // error: the result does not fit i8
b = 127i8 + 1      // 128i128 — promoted first, so nothing overflows
```

Annotate **both** operands when you want the narrow type enforced. Integer `/` and `%` by zero
are evaluation errors. Floats keep IEEE semantics and give `inf` or `NaN` instead.

```wcl
a = 4 / 0                       // error: cannot divide by zero
b = 4.0 / 0.0                   // inf
c = try 4 / 0 catch e { 0 }     // recoverable like any evaluation error
```

Operator-level detail is in `lang_expressions.md`.

## Literal units

Any suffix that is not a numeric width is a **unit name**. The magnitude is multiplied by the
unit's factor and stored in the type's base unit. The suffix attaches directly to the number:
`5MiB`, never `5 MiB`.

Units are **type-scoped**. A literal resolves against the *declared type of the field it lands
in*, using the `@unit(name, factor)` decorators on that type. A unit the type does not declare
— or a unit literal with no declared type in context — is an error.

Three unit types are always in scope:

| Type | Base unit | Units |
| --- | --- | --- |
| `std.ByteSize` | byte | `B`, `KiB` `MiB` `GiB` `TiB` `PiB` (×1024ⁿ), `kB` `MB` `GB` `TB` (×1000ⁿ) |
| `std.Distance` | millimetre | `mm`, `cm`, `dm`, `m`, `km` |
| `std.Duration` | nanosecond | `ns`, `us`, `ms`, `s`, `min`, `h`, `d` |

```wcl
@document
type Config {
  buffer:  std.ByteSize     // base unit: byte
  radius:  std.Distance     // base unit: millimetre
  timeout: std.Duration     // base unit: nanosecond
  sizes:   list<std.ByteSize>
}

buffer  = 4MiB              // 4194304
radius  = 3km               // 3000000
timeout = 30s               // 30000000000
sizes   = [256KiB, 1MiB]    // each element resolves on its own
```

To declare your own, put `@unit` decorators on a numeric type alias. `factor` is the number of
base units in one of that unit. It is an ordinary expression:

```wcl
@unit("g", 1)
@unit("kg", 1000)
type Grams = i64          // a Grams field written `5kg` holds 5000
```

Rules worth knowing:

| Case | Result |
| --- | --- |
| `5MiB` on a `std.ByteSize` field | `5242880` |
| A float magnitude whose product is whole (`1.5MiB`) | `1572864` |
| A unit the type does not declare (`5km` on a `ByteSize`) | Error |
| A unit literal with no type in context (`let x = 5MiB`) | Error |

`format_unit(value, type_name, unit)` renders a stored base-unit value back in a chosen unit —
the inverse of resolution. `format_unit_value(value, factor, unit)` does the same with an
explicit factor.

```wcl
label = format_unit(buffer, "std.ByteSize", "MiB")   // "4 MiB"
```

## Booleans

`bool` has exactly two values, `true` and `false`. Every comparison produces one, and `&&`,
`||` and `!` combine them.

```wcl
enabled  = true
ready    = !pending && enabled
oversize = width > 100u32 || height > 100u32
```

## `none`

`none` is the absence value. A field typed `T?` accepts either a `T` or `none`; a field without
the `?` rejects it. Omitting an optional field and writing `none` into it mean the same thing.

```wcl
headline = none
```

`??` supplies a default when the left side is `none`:

```wcl
theme = page.theme ?? site.theme ?? :nord
```

Optionals in full are in `lang_types.md`; `??` is in `lang_expressions.md`.

## Symbols

A **symbol** is an identifier-like value written `:name`, with no space after the colon. Use a
symbol for a tag or an enum-like choice where a string would be untyped.

```wcl
shade  = :amber
accent = :cyan
edge   = :uses
```

The type `symbol` accepts any symbol at all. To close the vocabulary, declare a `symbol_set` —
see `lang_types.md`.

## Identifiers

Identifiers name fields, types, block kinds, variants, symbols, `let` bindings and imported
items. One rule everywhere, with one exception for block labels.

An identifier starts with an ASCII letter or `_`, and continues with letters, digits or `_`.
No Unicode, no dashes, no spaces.

| Name | Legal | Why |
| --- | --- | --- |
| `name`, `my_field`, `_internal` | yes | letters and underscores |
| `v2`, `HTTPStatus` | yes | digits and capitals are fine after the first character |
| `2nd_attempt` | no | must not start with a digit |
| `kebab-case` | no | dashes are not identifier characters — except in a block label |

**Block labels** are the exception: a bare label may contain `-` and `/` connectors, so
kebab-case names and path-like names need no quotes (`class dgm-box`, `page api/v1/users`).
See `lang_documents.md`.

### Reserved words

The lexer reserves six words. You cannot use them as identifiers anywhere:

`true`, `false`, `none`, `if`, `else`, `match`

Fourteen more words look like keywords: `type`, `interface`, `union`, `symbol_set`, `let`,
`import`, `namespace`, `use`, `connection`, `fn`, `extends`, `as`, `try` and `catch`. The
parser recognises each one only in a declaration position, so each may also be an ordinary
identifier. Keep them for their declaration use — it reads better.

### Conventions

The standard library uses `snake_case` for fields, `let` bindings, block kinds and symbols,
and `PascalCase` for types, interfaces, unions, variants and symbol sets. The language does
not enforce either.

## Strings

String literals are UTF-8 by default. A prefix selects another encoding.

```wcl
name  = "hello"          // utf8 (default)
label = utf8"hello"      // explicit
tag   = ascii"id-007"
wide  = utf16"hello"
quad  = utf32"hello"
```

| Type | Use |
| --- | --- |
| `utf8` | Default. Variable-width Unicode. |
| `ascii` | 7-bit text, one byte per character. |
| `utf16` | Variable-width, two bytes per BMP code unit. |
| `utf32` | Fixed-width, four bytes per code point. |

The encoding is part of the value's type, so a `utf8` field rejects an `ascii` literal.

### Escapes

The escape set is small and closed. An unrecognised escape is a lex error, not a literal
backslash.

| Escape | Character |
| --- | --- |
| `\"` | `"` |
| `\\` | `\` |
| `\n` | newline |
| `\t` | tab |
| `\r` | carriage return |
| `\$` | a literal `$` — only inside an interpolated (`$"…"`) string |

```wcl
greeting = "Hello,\nworld!"
quoted   = "She said \"hi\"."
```

For backslash-heavy text — regexes, LaTeX, Windows paths — reach for a raw heredoc instead of
escaping every character.

### Interpolation

Interpolation is **opt-in**: prefix the literal with `$`. Inside a `$"…"` string, a `${ … }`
slot evaluates any expression and splices the result in. Without the `$` prefix, `${…}` is
literal text. All four encodings accept the prefix.

```wcl
greeting = $"Hello, ${name}! Count: ${count + 1u32}"
literal  = "Costs ${amount}"          // no prefix — the braces are text
```

### Heredocs

`<<TAG` opens a heredoc whose body runs to a line matching the closing tag. Escapes are
interpreted, and the common indentation of the body is stripped to match the closing tag, so a
heredoc nests comfortably inside a block.

```wcl
note = <<END
First line.
Second line.
END
```

`$<<TAG` opts the heredoc into interpolation, exactly like `$"…"`:

```wcl
block = $<<MSG
You have ${len(items)} items waiting.
MSG
```

A **raw heredoc** uses a single-quoted opening tag and takes the body verbatim — no escapes,
no interpolation. Common leading whitespace is still stripped.

```wcl
regex = <<'RAW'
\d{3}-\d{4}
RAW

latex = <<'TEX'
\frac{-b \pm \sqrt{b^2 - 4ac}}{2a}
TEX
```

> Use `<<'TAG'` whenever the body contains backslashes or `${`. It is the one form that never
> reinterprets its contents.

## Worked example

```wcl
@unit("g", 1)
@unit("kg", 1000)
type Grams = i64

symbol_set Grade { first  second }

@document
type Parcel {
  weight:  Grams
  volume:  std.ByteSize
  grade:   Grade
  express: bool
  note:    utf8?
  label:   utf8
}

weight  = 2kg                                   // 2000
volume  = 512KiB                                // 524288
grade   = :first
express = weight < 5000 && grade == :first
note    = none
label   = $"parcel ${format_unit(weight, "Grams", "kg")} (${note ?? "no note"})"
```

```console
$ wcl get parcel.wcl label
"parcel 2 kg (no note)"
```
