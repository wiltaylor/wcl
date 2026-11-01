# Expressions and operators

Every field value is an expression. This chapter is the operator set, the precedence table,
bitwise operators and shifts, and the three postfix forms — member access, calls and variant construction. Control-flow
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
| `&` `\|` `^` | bitwise and, or, exclusive or (integers only) | `flags & 0x0Fu8` |
| `<<` `>>` | shift left, shift right (integers only) | `1u32 << 3u32` |
| `~` (unary) | bitwise complement (integers only) | `~0u8` |
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
| 6 | `\|` |
| 7 | `^` |
| 8 | `&` |
| 9 | `<<` `>>` |
| 10 | `+` `-` |
| 11 | `*` `/` `%` |
| 12 | unary `-` `!` `~` |
| 13 | call `f(…)` |
| 14 (tightest) | `.` and `::` |

The bitwise tiers sit between comparison and arithmetic, as in Rust and unlike C.

Two consequences:

```wcl
a = trim(raw) ?? "untitled"        // (trim(raw)) ?? "untitled" — the call binds tighter
b = x ?? y ?? z                    // (x ?? y) ?? z — left-associative
c = a && b || c && d               // (a && b) || (c && d)
d = -x.field                       // -(x.field) — member access binds tighter than unary
e = x & 1 == 0                     // (x & 1) == 0 — bitwise binds tighter than comparison
f = 1 << n + 1                     // 1 << (n + 1) — shifts bind looser than arithmetic
```

**An expression is at most 256 levels deep.** Each binary operator, call, member access and
bracket is a level, and a left-associative chain nests one level per link: `1 + 1 + … + 1` with
more than 256 terms, or a 300-link `a.b.c…`, fails to parse with `expression too deep (more than
256 levels of operators, calls or member accesses)` (exit 1 from `wcl check`). Generated files hit
this. Split the chain with `let` bindings, or build a list and use `sum` / `join`. `else if`
chains and `${…}` slots nested inside one another count against the separate 128-level nesting
cap (`nesting too deep (more than 128 levels)`). Either parse cap can fire sooner on a thread
short of stack (the LSP's 2 MiB workers, an unoptimised build) with the same message, instead
of a crash. This is a parse-time limit on one written expression, separate from the run-time
cap of 200 nested fields, `let`s and `fn` calls (`lang_evaluation.md`).

`&&`, `||` and `??` **short-circuit**. They skip the right side when the left already decides
the answer. That is what makes `count > 0 && total / count > 5` safe and `cached ??
expensive()` cheap.

## What is *not* an operator

Three shapes other languages spell with an operator are builtin calls here.

| Instead of | Write | Fails as |
| --- | --- | --- |
| `2.0 ^ 10.0` | `pow(2.0, 10.0)` | **evaluation** error — `^` is exclusive or: `operator '^' is not defined for f64 and f64` |
| `items[0]` | `at(items, 0)` | parse error — `[` only opens a list literal, a tensor shape, or a table row |
| `"a" + "b"` | `concat("a", "b")` | **evaluation** error — `operator '+' is not defined for utf8 and utf8` |

Only the second is a parse error. **The other two are not.** `wcl check` answers `OK` on `a =
"x" + "y"` and on `a = 2 ^ 10`, and the mistake surfaces only when something reads the field.
`2 ^ 10` is worse: between integers it is a valid exclusive or and answers `8`, not `1024`.

**String concatenation has no operator.** Use interpolation for anything with more than two
parts (`$"${host}:${port}"`). Use `concat(a, b)` for exactly two, `format("{}:{}", host, port)`
for a template, and `join(parts, "/")` for a list.

Three more symbols read like operators but are syntax:

| Symbol | Is |
| --- | --- |
| `&T` | a reference type in a type position — `lang_types.md`. In an expression `&` is bitwise and |
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

### Comparing numbers

Comparison does **not** use the ladder. `==`, `!=`, `<`, `<=`, `>` and `>=` compare two numbers
of any types exactly, by value, and order them the way `sort` does. No cast is needed, and no
precision is lost on the way:

```wcl
a = 1u32 == 1.0                                          // true
b = 340282366920938463463374607431768211455u128 > -1     // true — u128::MAX, no i128 detour
c = 9007199254740993 == 9007199254740992.0               // false — both round to one f64
```

**NaN is unordered.** `<`, `<=`, `>` and `>=` are all `false` when either side is NaN, `==` is
`false` even against itself, and `!=` is `true` (IEEE 754):

```wcl
d = (0.0 / 0.0) <= 1.0              // false
e = (0.0 / 0.0) >= 1.0              // false
f = (0.0 / 0.0) != (0.0 / 0.0)      // true
```

Ordering on strings is lexicographic between two strings of the same type. `==` on non-numeric
values is structural equality. Any other ordered pair is an error naming the operator you wrote:
`operator '<' is not defined for i64 and utf8`. A prefix `-` / `!` names its one operand:
`operator '-' is not defined for utf8`.

## Bitwise operators and shifts

`&`, `|`, `^`, `<<`, `>>` and prefix `~` take **integers only**, of any width, `isize`, `usize`
and the 128-bit types included. A float or `bool` operand is a type mismatch:
`operator '&' is not defined for f64 and i64`, `operator '|' is not defined for bool and bool`,
`operator '~' is not defined for f64`. There is no logical `&` / `|` on booleans; use `&&` / `||`.

```wcl
a = 0xF0u32 & 0x3Cu32     // 48u32
b = 0xF0u32 | 0x0Fu32     // 255u32
c = 0xFFu8 ^ 0x0Fu8       // 240u8
d = ~0u8                  // 255u8 — every bit of the operand's own type flips
e = ~0                    // -1
f = 0xF0u8 & 0x3Cu16      // 48i128 — mixed widths promote like arithmetic
```

**`&`, `|` and `^` promote exactly like arithmetic.** Same type in, same type out. A mixed pair
becomes `i128`, and a `u128` above `i128::MAX` beside any other type is a type mismatch. Annotate
both operands to keep a narrow type.

**Shifts never promote.** The result has the **left** operand's type, and the amount may be any
integer type:

```wcl
a = 1u8 << 7u64              // 128u8
b = 0x80000000u32 >> 31i8    // 1u32
c = 128u8 << 1               // 0u8 — bits shifted out are discarded, not an overflow
d = -16i8 >> 2               // -4i8 — `>>` sign-extends a signed left operand
e = 0x80u8 >> 7              // 1u8 — and fills an unsigned one with zeros
```

**The amount must be `0` to `bits - 1` of the left type.** Anything else, negative included, is
an evaluation error (`wcl::eval::arithmetic`), recoverable with `try` / `catch`:
`operator '<<' cannot shift u32 by 32 (the amount must be 0 to 31)`,
`operator '>>' cannot shift u32 by -1 (the amount must be 0 to 31)`. An unsuffixed left operand
is `i64`, so `1 << 40` is fine and `1 << 64` is the error.

### Gotchas

- **`a <<b` (space before, none after) opens a heredoc.** A name, number or string followed by
  a space and `<<TAG` is how a heredoc block label is written (`p <<'TXT'`), so the lexer reads
  it that way: `unterminated heredoc starting with '<<b'`. `a << b` and `a<<b` are shifts, and
  so is `f(a) <<b` (no heredoc follows `)` or `]`). **Write spaces on both sides.**
- **`ascii<<n` is a heredoc**, whatever `ascii` is bound to. `ascii`, `utf8`, `utf16` and
  `utf32` touching `<<` are always a typed heredoc prefix. Write `ascii << n`.
- **`a > > b` is not a shift.** A shift is two `<` or two `>` touching. `>>=` and `<<=` do not
  exist; there is no assignment operator at all.
- **`|` ends a table cell.** In a `| … |` row, a bare `|` is the delimiter, so bitwise or in a
  cell needs brackets: `| "rw" | (READ | WRITE) |`. Inside any bracket, call or list it is an
  operator again. `&`, `^` and the shifts need no brackets. The formatter brackets a cell whose
  tree holds a bare `|`, so a tree built through the edit API still prints back as one cell.
- **Match alternatives are unchanged.** `1 | 2 => …` is two patterns. A guard is an
  expression, so `1 | 2 if a | b == 3 => …` reads the guard as `(a | b) == 3`.
- **Slice a field with `bits(v, hi, lo)`**, not a hand-built mask. Both ends are inclusive, the
  answer keeps `v`'s type and is zero-extended, and `hi` past the type's width is an error. See
  [`lang_builtins.md`](lang_builtins.md#bits).

## Member access

`.` reads a member. A path walks the document tree first, follows a `&T` reference to the block
it names, then reads fields of whatever record value it lands on. The receiver may be any
expression: a call, a bracketed expression, a block, an `if`, a `match`, a literal.

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

**At a field, the path continues into the field's value** when that value is a record or a
variant with a record body. The same holds for a `let` item, a `let … ;` binding and a function
parameter, and for `wcl get` on the command line. It chains through nested records:

```wcl
let origin = { x: 0.0, inner: { zone: "a" } }

meta = { region: "us-east-1" }               // meta: Meta
data = Src::File { path: "/etc/app.conf" }   // data: Src

r    = meta.region                           // "us-east-1"
p    = data.path                             // "/etc/app.conf"
zone = origin.inner.zone                     // "a"
```

- **The tree wins.** A segment that names a block, a field of a block, or a declaration resolves
  exactly as it would without the record. The value's fields are read only when the document
  path goes no further.
- **A missing field names the record.** `meta.zone` fails with `unresolved reference 'meta.zone':
  record of type 'Meta' has no field 'zone'` and the help `its fields are: region` (exit 3). A
  variant says `variant 'Src::File'`. `wcl get m.wcl meta.zone` prints `no such path: meta.zone`.
- **A record literal is named by the slot it came out of.** It carries no type of its own, so
  the error uses the declared one: `at(rows, 0).nope` with `rows: list<Row>` says `record of type
  'Row'`. The same holds through `head`, `find`, `max_by`, `min_by` (and `filter`, `sort_by`,
  `reverse`, `take`, `drop`, `tail`, `slice`, `unique` in between), a field of a typed record
  (`cfg.db.nope` with `db: Db` says `Db`), a parameter or return type, a variant payload bound by
  `match` / `if let`, and a `let` bound to any of these. Only the message changes: the value
  stays an untagged literal (prints without a type name, equals the same literal anywhere). A
  literal nothing declared, or declared only as `any` / `record`, says just `record`. Call and
  field type errors name it the same way (`got record of type 'Row'`, `value is record of type
  'Row'`).
- **Nothing else has fields.** `rows.name` on a list fails with `unresolved reference
  'rows.name': value of type 'list' has no field 'name'`; a scalar names its own type the same
  way (`value of type 'i64'`). There is no index operator: write `at(rows, 0).name`.
- **A field that fails, fails the path.** `host.name` where `host` errors reports `host`'s error.

### Through a `&T` reference

A `&T` field that points at a **block** is followed: the next segment reads the block as if it
had been named directly. Fields, nested blocks and labels all resolve, references chain, and
`wcl get` follows the same way.

```wcl
@block("service") type Service {
  @inline(0) id: identifier
  host: utf8
  upstream: &Service?
}
@document type D { @children("service") services: list<Service>  h: utf8 }

service db  { host = "db.local" }
service api { host = "api.local"  upstream = services.db }
service web { host = "web.local"  upstream = services.api }

h = services.web.upstream.upstream.host     // "db.local"
```

```console
$ wcl get s.wcl services.web.upstream.host
"api.local"
$ wcl get s.wcl services.web.upstream
&services.api<block>
```

- **Only a block is followed.** A `&T` naming a declaration (`target = Row`) stays a handle to
  that declaration: `probes.p.target.score` is `unresolved reference`, as it always was. Pass
  the handle to a reflection builtin instead.
- **A dangling reference reports itself.** `upstream = services.ghost` makes
  `services.web.upstream.host` fail with `unresolved reference 'services.ghost'`, pointing at the
  `upstream = …` line.
- **A reference that loops fails as a cycle.** `upstream = self.upstream.upstream` gives
  `cycle while evaluating 'upstream'`; it does not hang.
- **A reference carried as a value is followed too**: through a local
  (`{ let u = services.web.upstream; u.host }`) or out of a reified block
  (`at([services.web], 0).upstream.host`).

### Off any other expression — a value

Anything that is not a path is evaluated, and `.` reads a field of the value it produces. The
same rules apply: a record or a variant with a record body has fields, nothing else does.

```wcl
first = at(rows, 0).name
top   = max_by(rows, fn(r: Row) -> i64 r.hits).name
pick  = (if prod { primary } else { backup }).host
local = { let r = at(rows, 1); r }.name
lit   = { name: "x", inner: { zone: "z" } }.inner.zone
```

- **Member binds tighter than call.** `f(x).g(y)` is `(f(x).g)(y)`: it calls the
  function-valued field `g` of `f(x)`'s result.
- **A missing field** gives the record error above: `unresolved reference 'at(rows, 0).nope':
  record has no field 'nope'`. The path in the message is the receiver as the formatter writes
  it (`<expr>` when that spans lines).
- **A non-record value** names its type: `(1 + 2).name` is `value of type 'i64' has no field
  'name'`.

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

Note what each line depends on. `ratio` needs `??` because `width` is optional. `headline` needs
`format`, because `+` does not join a number to a string. `at` appears because there is no index
operator.
