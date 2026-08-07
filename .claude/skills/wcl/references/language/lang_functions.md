# Functions

A function is a value. You write one inline, bind it with `let`, declare it with `fn`, pass it
to a builtin, or store it in a field. There is no separate function namespace.

## Function literals

```
fn(name: Type, …) -> ReturnType body
```

The body is a single expression, or a `{ … }` block expression. Both the parameter types and
the return type are **required**.

```wcl
double = fn(x: i32) -> i32 x * 2i32
sum_sq = fn(x: i32, y: i32) -> i32 { let s = x + y; s * s }
```

Bind a literal with `let` and call it like anything else:

```wcl
let double = fn(x: i32) -> i32 x * 2i32
let sum_sq = fn(x: i32, y: i32) -> i32 { let s = x + y; s * s }

four = double(2i32)          // 4i32
nine = sum_sq(1i32, 2i32)    // 9i32
```

> The declared return type documents the intent; it is **not** enforced. Writing the body as
> `x * 2` there would answer `4i128`, because the untyped `2` promotes the pair — see
> `lang_expressions.md`. Suffix your literals when the width matters.

## `fn` items

`fn name(params) -> R body` at item scope declares a named function. It is **sugar for
`let name = fn(…)`** — the same thing, with two extras: the name reaches editor tooling, and it
can carry decorators such as `@doc`.

```wcl
@doc("Clamp a value into [lo, hi].")
fn clamp_to(x: f64, lo: f64, hi: f64) -> f64 {
  min(max(x, lo), hi)
}

gain = clamp_to(raw_gain, 0.0, 1.0)
```

Because it is a `let`, an `fn` item is a **composition helper, not data**. It never appears in
the evaluated document, in `wcl get`, in JSON output, or in schema validation. See
`lang_documents.md`.

## The one constraint that shapes every WCL API

**A parameter list is fixed at declaration.**

- No **default** arguments.
- No **named** arguments — every argument is positional.
- No optional parameters: `?` in a parameter list is a **parse error** (`fn(x: utf8?)` fails
  with `expected ',' or ')' in parameter list`).
- No variadics in user code. A few builtins (`format`) are variadic; nothing you declare can be.

An argument count that does not match the parameter count is an error at call time.

The workaround for an optional parameter is to **annotate it as required and let `none` flow
through the body**. A `none` argument satisfies the call, and an optional field it lands in
reads as unset:

```wcl
union Html {
  Element { tag: utf8  id: identifier?  class: list<utf8>  children: list<Html> }
}

// Two constructors, because `id` cannot be given a default.
let el  = fn(tag: utf8, cls: list<utf8>, kids: list<Html>) -> Html
  Html::Element { tag: tag, class: cls, children: kids }

let eli = fn(tag: utf8, id: identifier, cls: list<utf8>, kids: list<Html>) -> Html
  Html::Element { tag: tag, id: id, class: cls, children: kids }

a = el("div", ["card"], [])            // id → none
b = eli("section", "intro", [], [])    // a string coerces to the identifier
```

This is exactly why libraries built on WCL grow **families** of near-identical constructors —
one per useful combination — rather than one function with defaults. When two members of such a
family share an arity, WCL cannot tell a transposed argument from a correct one: it checks
argument **count**, never argument **types**. Order the parameters so a mistake is obvious, or
give the two forms different arities.

> **A `{` straight after the return type is always a block expression, never a record literal.**
> `fn(x: i64) -> Point { x: x, y: 0 }` is a parse error. Return the explicit
> `Union::Variant { … }` form, or build the record in a block and return the binding.

## Function types

`fn(T1, T2, …) -> R` types a field, a parameter or a record member that holds a callable.

```wcl
type Step {
  apply: fn(i32) -> i32
}

@document type Pipeline {
  first: Step
}

first = { apply: fn(x: i32) -> i32 x + 1 }
```

A function type names only the shapes, not the parameter names.

## Higher-order functions

Functions take and return functions. That is how the collection builtins take their behaviour.

```wcl
doubled = map([1, 2, 3], fn(x: i64) -> i64 x * 2)

let adder = fn(x: i32) -> fn(i32) -> i32 fn(y: i32) -> i32 x + y
let add3  = adder(3i32)
seven     = add3(4i32)
```

The builtins that take a function argument:

| Builtin | Function argument |
| --- | --- |
| `map(xs, f)` | element → new element |
| `filter(xs, pred)` | element → `bool` |
| `fold(xs, init, f)` | accumulator, element → accumulator |
| `any(xs, pred)` / `all(xs, pred)` / `find(xs, pred)` | element → `bool` |
| `sort_by(xs, key)` / `min_by(xs, key)` / `max_by(xs, key)` | element → sort key |
| `group_by(xs, key)` | element → bucket key |
| `map_values(record, f)` | value → new value |

Signatures and return values are in `lang_builtins.md`.

## Closures and capture

A function literal **captures the local bindings in scope where it is written**. Those are the
`let … ;` bindings of an enclosing block expression, and the parameters of an enclosing
function. WCL takes the snapshot when the literal evaluates, and the snapshot travels with the
value.

```wcl
let make_scaler = fn(factor: f64) -> fn(f64) -> f64
  fn(v: f64) -> f64 v * factor        // `factor` is captured

let half = make_scaler(0.5)
result   = half(9.0)                  // 4.5
```

Two rules follow:

- **A capture holds locals only.** A field, a block, a type or a `let` **item** resolves through
  the scope chain when the call runs, not when you write the literal. The document is immutable
  once open, so the answer is the same either way.
- **A parameter shadows a capture of the same name.** Captures are bound first; parameters are
  bound over them.

Calls are depth-limited, so a function that recurses without a base case reports a depth error
rather than hanging.

## Evaluation

Function values take part in the document's lazy, cached field evaluation. A field holding a
call stays unevaluated until something reads it. A cycle between fields reports an error rather
than looping. Each call evaluates the body in a fresh frame. See `lang_evaluation.md`.

Arguments are ordinary expressions evaluated at the call. A bare record argument coerces to the
parameter's declared union variant by shape — one of the three places that coercion runs; see
`lang_collections.md`.

## Worked example

A small pipeline built from an `fn` item, a captured binding and three higher-order builtins.

Note that every parameter needs a **named** type. There is no anonymous record type: `fn(r: {
name: utf8 })` is a parse error, so the row shape is declared once as `Row`.

```wcl
type Row {
  name: utf8
  hits: i64
}

@doc("Percent of `total`, rounded down, guarding a zero total.")
fn pct(part: i64, total: i64) -> i64 {
  if total == 0 { 0 } else { part * 100 / total }
}

@document
type Summary {
  rows:    list<Row>
  busiest: utf8
  shares:  list<utf8>
  quiet:   list<utf8>
}

rows = [
  { name: "north", hits: 42 },
  { name: "south", hits: 18 },
  { name: "east",  hits: 0  },
]

// `max_by(…).name` is not legal — a call result takes no member access.
busiest = { let top = max_by(rows, fn(r: Row) -> i64 r.hits); top.name }

shares = {
  let total = fold(rows, 0, fn(a: i64, r: Row) -> i64 a + r.hits);
  map(rows, fn(r: Row) -> utf8 format("{}: {}%", r.name, pct(r.hits, total)))
}

quiet = map(
  filter(rows, fn(r: Row) -> bool r.hits == 0),
  fn(r: Row) -> utf8 r.name,
)
```

```console
$ wcl get summary.wcl busiest
"north"
$ wcl get summary.wcl shares
["north: 70%", "south: 30%", "east: 0%"]
$ wcl get summary.wcl quiet
["east"]
```

`total` is a block binding, captured by the literal that mentions it. `pct` is an item, so it
resolves at call time through the document scope, and never appears in the output. `rows` is a
field, so it does.
