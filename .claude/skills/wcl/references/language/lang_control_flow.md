# Control flow

Everything here is an **expression**. There are no statements, no loops and no early return. A
`match` produces a value, an `if` produces a value, and a `try` produces a value. Iteration is
done with the collection builtins (`map`, `filter`, `fold`) — see `lang_collections.md`.

## `if` / `else`

```wcl
sign = if x < 0 { :neg } else if x > 0 { :pos } else { :zero }
```

The branches must agree on a type. `else if` chains for a multi-way branch. Both branch bodies
are block expressions, so the braces are mandatory even for a single value.

### An `else`-less `if` is `none`

Omit the `else` and the untaken branch evaluates to `none`. An else-less `if` therefore has type
`T?`.

```wcl
// A conditional list element — `none` when the condition is false.
class = ["nav-item", if entry.current { "current" }]

// A chain that may take no branch at all.
badge = if count == 0 { "empty" } else if count > 99 { "many" }

// An optional field, without the `else { none }` ceremony.
subtitle = if page.tagline != "" { page.tagline }
```

That `none` is a **value**, so it has to land somewhere that accepts absence: an optional field,
or a list element. Assigning an else-less `if` to a required field is a schema violation
whenever the condition is false. A `@non_empty` list needs at least one element that is not
`none`, because that is what its readers will see.

## `match`

`match` tests a value against patterns in order and returns the body of the first that matches.
It is the primary tool for destructuring unions and optionals.

```wcl
area = match shape {
  Shape::Circle { radius, .. } => pi() * radius * radius,
  Shape::Polygon(n) if n > 2   => 0.0,
  Shape::Empty                 => 0.0,
  _                            => 0.0,
}
```

Separate the arms with commas. A trailing comma is fine.

### The last arm must be irrefutable

**A `match` must end with a bare `_` or a plain binding arm, with no guard and no alternation.**
The parser checks this, so you learn about it before evaluation. It is how WCL gets
exhaustiveness without a type-directed check.

```wcl
// Parse error: "match must end with a wildcard or binding arm".
bad = match n {
  0 => "zero",
  1 => "one",
}

// Fine.
good = match n {
  0 => "zero",
  1 => "one",
  _ => "many",
}
```

A final `n => …` binding arm counts as irrefutable and lets the body use the value. A final
`_ => …` with a guard does not.

### Guards

An `if expr` after a pattern adds a runtime test. The arm fires only when the pattern matches
**and** the guard is true.

```wcl
classify = match n {
  k if k < 0  => :neg,
  0           => :zero,
  k if k > 10 => :big,
  _           => :small,
}
```

### Alternation

Separate alternatives with `|`. Every alternative must bind **the same names**, so the body is
unambiguous whichever one fired.

```wcl
kind = match tag {
  :info | :note    => "informational",
  :warn | :error   => "problem",
  _                => "other",
}
```

## The pattern vocabulary

| Pattern | Matches |
| --- | --- |
| `_` | anything, binding nothing |
| `name` | anything, binding the value to `name` |
| `name @ inner` | `inner`, also binding the whole value to `name` |
| `42`, `true`, `"hi"`, `:red`, `none` | equality with that literal |
| `Union::Variant` | a unit variant |
| `Union::Variant(x)` | a typeref variant, binding the payload to `x` |
| `Union::Variant { field: pat, .. }` | a record variant; `..` ignores the remaining fields |
| `Union::Variant { field }` | shorthand — binds the field to its own name |
| `Variant(x)` / `Variant { … }` | **unqualified** — the variant is resolved from the value's own union |
| `p1 \| p2` | either alternative (arm level; both must bind the same names) |

Two limits. A string pattern must be a `utf8` or `ascii` literal; a `utf16` or `utf32` literal
fails. And there are no list or bare-record patterns — you destructure through a variant.

## `if let`

`if let pattern = value { … } else { … }` binds and branches in one step. Reach for it when only
one case interests you.

```wcl
label = if let Shape::Circle { radius, .. } = s {
  format("circle r={}", radius)
} else {
  "other"
}
```

> **The `else` is required.** Unlike a plain `if`, an `if let` with no `else` is a parse error:
> `'if let' requires an 'else' branch`. This is a known gap rather than a semantic obstacle —
> the evaluator would happily answer `none`. Write the `else` for now.

## Block expressions

A `{ … }` expression holds zero or more `let … ;` bindings followed by a **mandatory tail
expression**, which is the block's value.

```wcl
result = {
  let a = to_upper("x");
  let b = to_upper("y");
  len(a) + len(b)
}
```

Note the semicolons: a binding **inside** a block expression ends with `;`. An empty `{}` is not
a valid expression — there would be no value.

### Block bindings versus `let` items

They look alike and are different things.

| | `let` item | `let … ;` binding |
| --- | --- | --- |
| Written | `let n = 1` at item scope | `let n = 1;` inside `{ … }` |
| Terminator | none | `;` |
| Scope | the enclosing block and its descendants | that one block expression |
| Visible to the document | no, in both cases | no |

The item form is a document-wide composition helper — see `lang_documents.md`. The binding form
names an intermediate inside one expression.

## `try` / `catch`

`try body catch name => handler` evaluates the body. On failure the **rendered error message**
binds to `name` as a `utf8`, and the handler's value becomes the result.

```wcl
rate = try parse_rate(raw) catch m => 1.0

summary = try {
  let r = risky();
  format("ok: {}", r)
} catch msg {
  format("failed: {}", msg)
}
```

Either side may be a `{ … }` block. When the handler is a block the `=>` is optional, so both
`catch name => expr` and `catch name { … }` are valid.

`try` catches **everything**: a builtin error, an `error()` call, an arithmetic fault, a
reference cycle, a propagated failure from another field. That breadth is the point and also the
warning — use it where a fallback is genuinely meaningful, not to hide a schema mistake. `wcl
check` still reports schema violations regardless.

`try` handles *failure*. `??` handles *absence*. They are not interchangeable: a value that
errors is not `none`, and `none` is not an error.

## Choosing between them

| Situation | Reach for |
| --- | --- |
| An optional needs a default | `??` |
| One variant matters, the rest share a fallback | `if let` |
| Several variants each need their own answer | `match` |
| A boolean condition picks between two values | `if` / `else` |
| A conditional element with nothing on the false side | else-less `if` |
| The expression may **fail**, not merely be absent | `try` / `catch` |

## Worked example

```wcl
union Source {
  Inline { rows: list<i64> }
  File   { path: utf8  header: bool }
  Absent none
}

@document
type Report {
  source:  Source
  label:   utf8
  count:   i64
  warning: utf8?
}

source = { path: "sales.csv", header: true }

label = match source {
  Source::File { path, .. }  => format("file {}", path),
  Source::Inline { rows }    => format("{} inline rows", len(rows)),
  Source::Absent             => "no source",
  other                      => "unknown",
}

count = {
  let rows = match source {
    Source::Inline { rows } => rows,
    _                       => [],
  };
  len(rows)
}

warning = if count == 0 { "the report is empty" }
```

```console
$ wcl get report.wcl label
"file sales.csv"
$ wcl get report.wcl count
0
$ wcl get report.wcl warning
"the report is empty"
```

The final `other => …` arm exists because the parser demands one, even though the three variants
above it are already complete.
