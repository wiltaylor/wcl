# Expressions and operators

Every field value is an expression. This chapter is the operator set, the precedence table, and
the three postfix forms — member access, calls and variant construction. Control-flow
expressions (`if`, `match`, `try`) are in `lang_control_flow.md`.

## The operator set

| Operator | Meaning | Example |
| --- | --- | --- |
| `+` `-` `*` `/` `%` | add, subtract, multiply, divide, remainder | `1 + 2 * 3` |
| `-` (unary) | negation | `-5` |
| `!` (unary) | logical not | `!enabled` |
| `==` `!=` | equality | `1u32 == 1i64` |
| `<` `<=` `>` `>=` | ordering | `age >= 18` |
| `&&` `\|\|` | logical and, or | `a && !b` |
| `??` | none-coalescing | `box.width ?? 480.0` |
| `.` | member access | `service.metadata.region` |
| `::` | qualified name — a namespaced type, kind or union variant | `Shape::Circle` |

## Precedence

Loosest first. Every binary operator is **left-associative**.

| Tier | Operators |
| --- | --- |
| 1 (loosest) | `??` |
| 2 | `\|\|` |
| 3 | `&&` |
| 4 | `==` `!=` |
| 5 | `<` `<=` `>` `>=` |
| 6 | `+` `-` |
| 7 | `*` `/` `%` |
| 8 | unary `-` `!` |
| 9 | call `f(…)` |
| 10 (tightest) | `.` and `::` |

Two consequences worth remembering:

```wcl
a = trim(raw) ?? "untitled"        // (trim(raw)) ?? "untitled" — the call binds tighter
b = x ?? y ?? z                    // (x ?? y) ?? z — left-associative
c = a && b || c && d               // (a && b) || (c && d)
d = -x.field                       // -(x.field) — member access binds tighter than unary
```

`&&`, `||` and `??` **short-circuit**. The right side is not evaluated when the left already
decides the answer, which is what makes `count > 0 && total / count > 5` safe and
`cached ?? expensive()` cheap.

## What is *not* an operator

Three shapes other languages spell with an operator are builtin calls here. Writing the
operator is a parse error, not a subtle bug.

| Instead of | Write | Why |
| --- | --- | --- |
| `2.0 ^ 10.0` | `pow(2.0, 10.0)` | `^` is not a token |
| `items[0]` | `at(items, 0)` | `[` only opens a list literal, a tensor shape, or a table row |
| `"a" + "b"` | `concat("a", "b")` or `$"${a}${b}"` | `+` is numeric only — it has no string case |

**String concatenation has no operator.** Use interpolation for anything with more than two
parts (`$"${host}:${port}"`), `concat(a, b)` for exactly two, `format("{}:{}", host, port)` for
a template, or `join(parts, "/")` for a list.

Three more symbols read like operators but are syntax:

| Symbol | Is |
| --- | --- |
| `&T` | a reference type — `lang_types.md` |
| `->` | a function's return type, or a connection statement — `lang_functions.md`, `lang_connections.md` |
| `=>` | the separator between a `match` arm and its body — `lang_control_flow.md` |

## Numeric promotion at the operator

Mixed-width and integer/float operands widen to a common type before the operation runs. The
ladder is: if either side is a float, both become `f64`; otherwise both become `i128`.

```wcl
a = 1 + 2.0        // 3.0
b = 1u32 == 1i64   // true
c = 3.0 * 2u8      // 6.0
```

Two operands of the **same** numeric type skip promotion and keep that type, which is what makes
overflow detectable:

```wcl
a = 127i8 + 1i8    // error: the result does not fit i8
b = 127i8 + 1      // 128i128 — the untyped literal promotes first, so nothing overflows
```

If you want a narrow type enforced, annotate **both** operands. Integer `/` and `%` by zero are
evaluation errors; floats follow IEEE and give `inf` or `NaN`. Any of these is recoverable with
`try` / `catch`.

Comparison uses the same ladder, so cross-width comparison needs no cast. Ordering on strings is
lexicographic. `==` on non-numeric values is structural equality.

## Member access

`.` reads a member. It behaves differently depending on what is on its left, and the difference
is the single most common source of `unresolved reference` in WCL.

### Off a document name — a path through the tree

A path starting at a field, a block, `self` or `parent` walks the **document tree**: blocks,
nested blocks, and gathered lists addressed by label.

```wcl
service "web" {
  region = "us-east-1"
  metadata {
    inherited = parent.region            // up one block
  }
  zone = self.metadata.tags.environment  // down through nested blocks
}
```

An **integer** segment is legal and still means a **label match**, not a positional index:
`steps.1` is the block labelled `1`. A float segment is an error.

**The path stops at a field.** It cannot continue into that field's *value*:

```wcl
meta = { region: "us-east-1" }
r    = meta.region              // error: unresolved reference 'meta.region'
```

The same applies to a variant payload (`data.path` where `data` is a union-typed field) and to
`wcl get` on the command line.

### Off a local or a call — a value

A name bound by a `let` item, a `let … ;` binding or a function parameter holds a **value**, and
`.` on it chains freely through nested records and variant payloads.

```wcl
let origin = { x: 0.0, inner: { zone: "a" } }

zone   = origin.inner.zone                          // "a"
region = { let m = meta; m.region }                 // bind the field, then read
path   = match data { Src::File { path, .. } => path, _ => "" }
```

So the fix for the failing cases above is always the same: **bind it, then read it.**

### Not off a call result

`f(x).field` is an error — `expected a reference, got call`. Bind the result first:

```wcl
top = { let t = max_by(rows, fn(r: Row) -> i64 r.hits); t.name }
```

`self` and `parent` are the two navigators into the enclosing scope — see `lang_documents.md`.

## Calls

Parentheses call a builtin, an `fn` item, or any function-valued binding. Arguments are
expressions. The body evaluates in a fresh context, so a call never sees the caller's local
names.

```wcl
n     = len(items)
total = sum(map(items, fn(x: i64) -> i64 x * 2))
```

There are **no named arguments and no default arguments**. Every parameter is positional and
every one must be supplied. See `lang_functions.md` for what that means when you design an API.

Parentheses in one other place carry a union variant's positional payload:

```wcl
b = Shape::Polygon(7)
```

Records have no constructor call at all. A record value is a bare `{ field: value }` literal
typed by the position it lands in — see `lang_collections.md`.

## `??` — none-coalescing

`a ?? b` is `a` unless `a` is `none`, in which case it is `b`. It chains left to right, binds
looser than everything else, and evaluates the right side only when needed.

```wcl
width = box.width ?? 480.0
theme = page.theme ?? site.theme ?? :nord
```

Because it is the loosest operator, an expression on its left is grouped whole:

```wcl
label = trim(raw_label) ?? "untitled"     // (trim(raw_label)) ?? "untitled"
n     = a + b ?? 0                        // (a + b) ?? 0
```

`??` handles absence. It does **not** handle failure: an expression that *errors* is not `none`,
and `??` will not catch it. Use `try` / `catch` for that.

## Worked example

```wcl
@document
type Panel {
  width:    f64?
  height:   f64?
  ratio:    f64
  scale:    f64
  headline: utf8
  tier:     utf8
}

let items = [3, 8, 12, 5]

width  = none
height = 270.0

ratio    = (width ?? 480.0) / (height ?? 270.0)
scale    = pow(2.0, 3.0) / 4.0
headline = format("{} items, largest {}", len(items), at(sort(items), len(items) - 1))
tier     = if len(items) > 3 && ratio > 1.0 { "wide" } else { "narrow" }
```

```console
$ wcl get panel.wcl ratio
1.7777777777777777
$ wcl get panel.wcl headline
"4 items, largest 12"
$ wcl get panel.wcl tier
"wide"
```

Note what each line depends on: `ratio` needs `??` because `width` is optional; `headline` needs
`format` because `+` would not join a number to a string; `at` appears because there is no index
operator.
