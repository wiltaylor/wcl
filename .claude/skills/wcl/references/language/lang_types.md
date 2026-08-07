# Types

The type declarations you write, and the type references that name them. Values are in
`lang_values.md` and `lang_collections.md`; the schema decorators that turn a type into a
document shape are in `lang_schemas.md`.

## The built-in types

| Group | Types |
| --- | --- |
| Boolean | `bool` |
| Signed integer | `i8` `i16` `i32` `i64` `i128` `isize` |
| Unsigned integer | `u8` `u16` `u32` `u64` `u128` `usize` |
| Float | `f32` `f64` |
| String | `utf8` `ascii` `utf16` `utf32` |
| Name | `symbol`, `identifier` |

`identifier` is a bare name rather than text. A quoted string in an `identifier`-typed slot
coerces to the identifier it spells, so `parent = "web"` and `parent = web` evaluate the same.
That is what lets a reference be written either way.

## Type references

Anywhere a type is expected you may write one of these forms:

| Form | Means |
| --- | --- |
| `utf8`, `u32`, … | a built-in type |
| `Name`, `a.b.Name` | a declared record, alias, union or symbol set |
| `list<T>` | an ordered homogeneous sequence |
| `tensor<T, [dims…]>` | a shape-carrying N-dimensional array |
| `fn(T1, T2) -> R` | a callable |
| `&T` | a **reference** to a document node satisfying interface `T` |
| `T?` | optional — the value may be `none` |

An **interface** name is not in that list on its own: it is legal only behind `&`.

> **Type arguments on a named type are syntax only.** `content<SvgBlock>` parses, formats and
> reads back as metadata. Nothing checks their arity and nothing substitutes them. A named type
> resolves by its path alone, so `content<Nonsense>` parses happily. There is no `type Foo<T>`
> declaration form. Full generics are not implemented, so treat an argument list on a named type
> as documentation.

## Records — `type Name { … }`

A `type` with a body declares a named record: a fixed set of named, typed fields. `extends`
inherits another record's fields. You write a record value as a bare `{ … }` literal. See
`lang_collections.md`.

```wcl
type Dog {
  name: utf8
  age:  u32
}

type Pet extends Dog {
  breed: utf8
}
```

## Aliases — `type Name = TypeRef`

An alias is a transparent second name for any type. WCL resolves it wherever it is used,
transitively. Its value is that **constraint decorators travel with the name**: `wcl check`
validates every field declared with the alias.

```wcl
@min(1) @max(65535)
type Port = u16

@non_empty
type Name = utf8

type Service {
  name: Name        // rejects ""
  port: Port        // rejects 0u16 and anything above 65535
}
```

An alias is also where `@unit` decorators live — see `lang_values.md`.

### Constraints

| Decorator | Constraint |
| --- | --- |
| `@min(n)` | a numeric value must be at least `n` |
| `@max(n)` | a numeric value must be at most `n` |
| `@non_empty` | a string or list value must not be empty |

They attach to a field directly, or to a type alias, in which case every field using the alias
inherits them. A constraint bounds the value `T`, not its absence: `@min(1) retries: i64?`
still accepts `none`.

## Unions

A `union` is a tagged set of variants. A value is exactly one of them. Three variant body
shapes:

```wcl
union Shape {
  Circle { radius: f64  stroke: f64 }   // record body — no commas between fields
  Polygon i32                           // typeref body — one positional payload
  Empty none                            // unit body — no payload
}
```

Construct with `Union::Variant`. A record payload takes braces, a typeref payload parentheses,
a unit variant nothing.

```wcl
a = Shape::Circle { radius: 5.0, stroke: 0.5 }
b = Shape::Polygon(7)
c = Shape::Empty
```

An optional member of a record body defaults to `none` when omitted:

```wcl
union Outline {
  Circle { radius: f64  stroke: f64?  fill: utf8? }
}

a = Outline::Circle { radius: 5.0 }                // stroke and fill are none
b = Outline::Circle { radius: 5.0, stroke: none }  // the same value
```

### Explicit tag versus bare record

Where the expected type is a union, you may drop the tag and write a bare record. WCL infers
the variant from the record's field-name set, and narrows by field types when two variants
share a name set. Exactly one match is required. No match is the `VariantNoMatch` violation —
`no variant of 'X' matches the supplied shape`. More than one match is an ambiguity error.

```wcl
series = [
  { name: "North", values: [42.0, 55.0] },   // inferred
  ChartSeries::Ref { source: "sales.csv" },  // explicit
]
```

A bare record must carry the **full** field set, optionals included — the shape is what picks
the variant. Only the explicit form may omit them. The full rule, and the three places it runs,
are in `lang_collections.md`.

### `extends`

A union may `extends` another, inheriting its variants and adding more. This is how a host
extends an open vocabulary without editing the base declaration.

```wcl
union BaseShape {
  Empty none
}

union Shape extends BaseShape {
  Circle { radius: f64 }
  Square { side: f64 }
}
```

Destructure a union with `match` or `if let` — see `lang_control_flow.md`.

## Interfaces

An `interface` declares a **structural** contract: the fields a value must have. There is no
`implements`. Any value with the right fields, of compatible types, satisfies it automatically.

```wcl
interface Drawable {
  x: f64
  y: f64
}

interface Sized extends Drawable {
  width:  f64
  height: f64
}
```

Extra fields the interface never mentions are fine. What the value has decides satisfaction,
not what its type declared.

> **An interface is only usable through `&`.** A field typed by a bare interface name is a parse
> error: `interface 'Drawable' must be used through a reference ('&Drawable')`. An interface
> names the contract a *reference target* must meet; it is not a value type. See the next
> section.

## Reference types — `&T`

A `&T` field holds a **lazy navigator into the document** — a pointer to something declared
elsewhere that is expected to satisfy interface `T`. The interface says what shape the target
has; `&` says the field points at it rather than containing it.

**Write a path, not a value.** The path resolves through lexical scope: the innermost enclosing
block outward, then the document root.

```wcl
interface Endpoint {
  host: utf8
  port: u16
}

@block("host") type Host {
  @inline(0) id: identifier
  host: utf8
  port: u16
}

@block("route") type Route {
  @inline(0) id: identifier
  upstream: &Endpoint
}

@document type Config {
  @children("host")  hosts:  list<Host>
  @children("route") routes: list<Route>
}

host cdn {
  host = "cdn.internal"
  port = 443u16
}

route assets {
  upstream = hosts.cdn      // a path into the document
}
```

```console
$ wcl get config.wcl routes.assets.upstream
&hosts.cdn<block>
```

A reference reads back as the path it names, not as the target's contents — following it is the
consumer's job.

> **A `&T` field wants a document node, not a value.** A record literal fails with
> `not_a_reference`; a path to a `let` holding a record fails with `declared as &T but value is
> record`. Point it at a block. If you want the shape inline, declare the field with a concrete
> **record** type instead — `type Endpoint { … }` rather than `interface Endpoint { … }`.

`self` and `parent` reify to the same kind of lazily-resolved navigator — see
`lang_documents.md`.

> **WCL checks interface satisfaction structurally, and not everywhere.** It walks a record, or
> a variant carrying a record, field by field. It passes through closures, lists, tensors, bare
> scalars and a reference's block target, because the runtime carries no type tag for them.
> Treat `&T` as documentation of intent plus a resolvable link. Do not rely on it to reject a
> wrong target.

## Optionals — `T?`

Suffix any type with `?` and the field accepts either a `T` or `none`. Without the `?` the
field is required and rejects `none`.

```wcl
type Profile {
  name: utf8       // required
  bio:  utf8?      // optional
  age:  u32?
}

complete = { name: "Alice", bio: "Author.", age: 34u32 }
partial  = { name: "Bob",   bio: none,      age: none }
```

**Absence has two spellings and they mean the same thing:** omit the field, or assign `none`.
An omitted `T?` reads as `none`, and an explicit `none` satisfies the declared type — inside a
record literal too.

```wcl
@block("note") type Note {
  @inline(0) id: identifier
  body: utf8?
}

note first  { }                 // body → none
note second { body = none }     // identical, and the line is dead weight
```

Handle the two cases with `??`, `match` or `if let`:

```wcl
width   = box.width ?? 480.0
display = match maybe_name { none => "anonymous", n => n }
```

> **Optional or union?** Use `T?` when the only states are present and absent. When absence
> carries information — a reason, a fallback, several shapes — declare a union instead.

`?` is a **field** annotation, not part of a type reference. It is legal on a record field, a
variant field and a `type` body field. It is **not** legal in a function parameter list, where
`fn(x: utf8?)` is a parse error. Annotate the parameter as required and let the `none` flow
through — see `lang_functions.md`.

## Symbol sets

A `symbol_set` names a **closed vocabulary** of symbols: an enum, without a union's weight. A
field typed by the set accepts only its members.

```wcl
symbol_set Color {
  red
  green
  blue
}

type Paint {
  shade: Color     // only :red, :green or :blue
}

@document type Doc {
  cream: Paint
}

cream = { shade: :red }
```

Use a symbol set wherever another language would reach for an enum: severity levels, edge
kinds, layout modes, palette hues. Use the open `symbol` type only when the vocabulary is
genuinely unbounded.

## Function types

`fn(T1, T2, …) -> R` types a field or parameter that holds a callable.

```wcl
type Step {
  apply: fn(i32) -> i32
}
```

Function values take part in lazy field evaluation, and each call evaluates its body in a fresh
context. See `lang_functions.md`.

## Worked example

One file with an alias, a constrained field, a symbol set, an interface, a reference, a union
and an optional.

```wcl
@min(1) @max(65535)
type Port = u16

symbol_set Scheme { http  https }

interface Endpoint {
  host: utf8
  port: Port
}

union Backend {
  Static { root: utf8 }
  Proxy  { scheme: Scheme  weight: u8 }
}

@block("host") type Host {
  @inline(0) id: identifier
  host: utf8
  port: Port
}

@block("route") type Route {
  @inline(0) id: identifier
  backend:  Backend
  upstream: &Endpoint
  note:     utf8?
}

@document type Config {
  @children("host")  hosts:  list<Host>
  @children("route") routes: list<Route>
}

host cdn  { host = "cdn.internal"  port = 443u16 }
host edge { host = "10.0.0.4"      port = 8080u16 }

route assets {
  backend  = { root: "./public" }              // → Backend::Static, by shape
  upstream = hosts.cdn
}

route api {
  backend  = { scheme: :https, weight: 3u8 }   // → Backend::Proxy, by shape
  upstream = hosts.edge
}
```

```console
$ wcl check config.wcl
OK
$ wcl get config.wcl routes.api.backend
Backend::Proxy { scheme: :https, weight: 3u8 }
$ wcl get config.wcl routes.api.upstream
&hosts.edge<block>
$ wcl get config.wcl routes.api.note
none
```

Change `port = 8080u16` to `port = 0u16` and the check fails on `@min(1)` — the constraint
travelled from the `Port` alias to the field that used it.
