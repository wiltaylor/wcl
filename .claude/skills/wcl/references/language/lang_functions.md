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

> Both annotations are **enforced** at every call — see [What a call checks](#what-a-call-checks).
> Writing the body as `x * 2` computes `4i128`, because the untyped `2` promotes the pair (see
> `lang_expressions.md`), and the `-> i32` return converts it back to `4i32` because it fits. A
> result that does not fit fails the call: `double(2000000000)` is `'double' declared -> i32 but
> returned 4000000000i128, which is out of range for i32`.

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

## Parameter lists are fixed at declaration

- No **default** arguments.
- No **named** arguments — every argument is positional.
- No **omitted** arguments. `T?` on a parameter makes the *value* optional, not the argument:
  `fn greet(name: utf8?)` still needs `greet(none)`; `greet()` is
  `'greet' expects 1 argument(s), got 0`.
- No variadics in user code. A few builtins (`format`) are variadic; nothing you declare can be.

An argument count that does not match the parameter count is an error at call time.

### `T?` in a signature

A parameter or return type may end in `?`, with an optional field's meaning: `none` passes
through untouched, and any other value meets `T` by the field rule (conversion included). Use it
for a parameter that receives an optional field's value.

```wcl
union Html {
  Element { tag: utf8  id: identifier?  class: list<utf8>?  children: list<Html> }
}

let el  = fn(tag: utf8, cls: list<utf8>?, kids: list<Html>) -> Html
  Html::Element { tag: tag, class: cls, children: kids }

let eli = fn(tag: utf8, id: identifier?, cls: list<utf8>?, kids: list<Html>) -> Html
  Html::Element { tag: tag, id: id, class: cls, children: kids }

fn first(xs: list<utf8>) -> utf8? if len(xs) == 0 { none } else { at(xs, 0) }

a = el("div", none, [])                // class → none
b = eli("section", "intro", [], [])    // "intro" converts to the identifier intro
c = el("div", "wide", [])              // × argument 2 ('cls') of 'el' expects list<utf8>?, got utf8
```

- The message prints the `?`: `expects utf8?, got i64`, `'f' declared -> i64? but returned utf8`.
- `?` goes on the **whole** parameter or return type only. `list<utf8?>` is a parse error, as it
  is in a field, and so is `?` inside a function type: `fn(i64?) -> i64` fails with
  `expected ',' or ')' in fn type`. In a field, `f: fn() -> i64?` makes the **field** optional.
- `fn(a, b) -> fn(i64) -> i64? body`: the `?` belongs to the **outer** return type.
- `fn_signature(f).signature`, `ast_string(f)`, `wcl fmt` and LSP signature help all keep the `?`.
- Keep `any` for a parameter that takes several unrelated types. `T?` is the narrower contract.

This is exactly why libraries built on WCL grow **families** of near-identical constructors —
one per useful combination — rather than one function with defaults. When two members of such a
family share an arity, only a parameter **type** can catch a transposed argument, and an `any`
parameter catches nothing. wdoc's `ela(tag, cls, attrs, kids)` and `eli(tag, id, cls, kids)`
catch each other: `eli` handed `ela`'s arguments fails on `id: identifier?` (`got list`), and
`ela` handed `eli`'s fails on `cls: list<utf8>?` (`got utf8`).

> **A `{` straight after the return type is always a block expression, never a record literal.**
> `fn(x: i64) -> Point { x: x, y: 0 }` is a parse error. Return the explicit
> `Union::Variant { … }` form, or build the record in a block and return the binding.

## What a signature may name

Every type in a signature must resolve, as a field type must. A name nothing declares fails at
open, wherever the literal sits (an item, a `let`, a lambda inside a call), in the root or any
eagerly-imported file:

```text
fn f(x: bogus) -> i64 1      // × unknown type 'bogus'   (exit 1)
```

A signature is allowed two things a field is not:

- The value classes from the builtin notation: `any`, `number`, `integer`, `record`, at any depth
  (`list<any>`, `fn(any) -> any`). Type variables (`T`) and `never` are not among them.
- An interface by bare name (`fn(s: Shape) -> …`). A field needs `&Shape`.

`?` is legal on a parameter's type and on the return type (see `T?` above), and nowhere inside
a type: not in `list<utf8?>`, not in a function type (`fn(i64?) -> i64`).

## What a call checks

Every call checks, in order: the argument **count**, each **argument** against its parameter's
type, then the body's **result** against the return type. Builtins are not covered by this
section; they check their own arguments.

**The rule is the field rule.** An argument is accepted exactly when a field declared with the
parameter's type would accept it, and it is **converted** exactly as that field would convert it.
The result meets the return type the same way. Concretely:

| Value handed to the slot | Outcome |
| --- | --- |
| a number that fits the declared numeric type | converted to that width (`8080` → `8080u16`; an `i128` product → `i32`) |
| a number that does not fit (`300` for `u8`, `2.5` for an integer type) | refused: `got 300, which is out of range for u8` |
| a string for `identifier` | converted to the identifier (the reverse is refused: an `identifier` for `utf8` fails) |
| a bare record for a union type | converted to the variant its shape matches; no match is `wcl::eval::schema_violation`: `no variant of 'Capacity' matches the supplied shape` |
| a variant of a different union | refused |
| a block's record for a type with a different name | refused — even the type its schema `extends` (records match by type name) |
| a record literal for a plain (non-union) named type | accepted unchecked, as a field accepts it |
| a non-variant, non-record value for a union type | accepted (field-rule gap: `fn(s: Shape)` takes `"x"`) |
| a symbol outside the declared `symbol_set` | refused: `got :medium, which is not a member of 'Mode'` |
| a value an alias's constraint rejects (`@min(1) type Port = u16`) | refused: `got u16, but value 0 is below @min(1)` |
| a unit literal (`5MiB`) | resolved against the type's `@unit`, as in a field |
| `none` | accepted by `T?` and `any`, refused by every other type (`expects utf8, got none`) |

The signature-only types check and **never convert**:

| Type | Accepts |
| --- | --- |
| `any` | every value, `none` included |
| `number` | any numeric value, any width (not widened to `f64`, unlike a builtin's `number`) |
| `integer` | any integer value, any width or signedness |
| `record` | a record, or a variant whose payload is a record (`U::P(1)` is refused) |
| bare interface `Shape` | a record or record-payload variant with every interface field, correctly typed; other values pass unchecked |
| `fn(A, B) -> R` | any function with **two** parameters — parameter and return types are not compared |
| `&T` | a document reference: a declaration named bare (`f(Service)`) or a `&T` field's value. A record fails, and a block reached by path (`f(services.web)`) is a record by then. The target is not checked against `T`. |
| `list<…>` of any of these | a list whose every non-`none` element passes |

**Errors** — both are evaluation errors: `try` catches them, `wcl check` without `--eval` does not
see them, `wcl check --eval` and `wcl get` exit 3.

| Code | Message | Points at |
| --- | --- | --- |
| `wcl::eval::argument_type` | `argument 2 ('id') of 'eli' expects identifier, got bool` | the argument expression; the whole call when the argument is a literal or the caller is a builtin (`map`, `fold`) |
| `wcl::eval::return_type` | `'f' declared -> utf8 but returned i64` | the function body, in the file the function was written in |

The function is named by the call (`f(x)` → `'f'`, `steps.scale.apply(x)` → `'apply'`), else by the `let`
or `fn` item that first bound it (so `fold(xs, 0, add)` reports `'add'`), else `fn literal`
(`argument 1 ('x') of fn literal expects utf8, got i64`). A list argument names its first bad
element: `got a list whose element [1] is utf8`.

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

Three rules follow:

- **A capture holds locals only.** A field, a block, a type or a `let` **item** resolves through
  the scope chain when the call runs, not when you write the literal. The document is immutable
  once open, so the answer is the same either way.
- **A parameter shadows a capture of the same name.** Captures are bound first; parameters are
  bound over them.
- **A call never sees its caller's locals.** A name the body neither captured nor took as a
  parameter resolves only through document scope, from the block the call sits in: `fn boo() ->
  i64 ghost` called as `{ let ghost = 1i64; boo() }` fails with `unresolved reference 'ghost'`.

## Evaluation

Function values take part in the document's lazy, cached field evaluation. A field holding a
call stays unevaluated until something reads it. A cycle between fields reports an error rather
than looping, and calls are depth-limited, so a function that recurses without a base case
reports `call depth limit exceeded (max 200)` rather than hanging. The 200 levels are shared
with the fields and `let`s the calls force, so recursion that reads fields runs out sooner. Each call evaluates the body in a fresh frame. See
`lang_evaluation.md`.

Arguments are ordinary expressions evaluated at the call, then converted and checked against the
parameter types (see [What a call checks](#what-a-call-checks)). A bare record argument coerces
to the parameter's declared union variant by shape, and a bare record result to the return
type's — two of the four places that coercion runs; see `lang_collections.md`.

## Worked example

A small pipeline built from an `fn` item, a captured binding and three higher-order builtins.

Every parameter needs a **named** type. There is no anonymous record type: `fn(r: { name:
utf8 })` is a parse error, so the row shape is declared once as `Row`.

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
