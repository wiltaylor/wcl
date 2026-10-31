# Schemas

A schema is an ordinary `type` declaration carrying decorators. The decorators say what the
type means structurally. Which keyword authors it, which labels are positional, which nested
kinds it accepts, and which type is the document root. `wcl check` enforces the result.

This page covers the *structural* decorators. The full decorator vocabulary, including the ones
that carry no structural meaning and the ones you declare yourself, is in
[`lang_decorators.md`](lang_decorators.md).

## The shape of a schema

```wcl
@block("server") type Server {
  @inline(0) id: identifier
  host: utf8
  @default(8080) port: u16
}

@document type Config {
  @children("server") servers: list<Server>
}

server web {
  host = "localhost"
}
```

Read it as four statements. You write a `Server` instance with the keyword `server`. The first
label fills `id`. `port` is optional, because it has a default. `Config` is the document root,
and it gathers every top-level `server` block into `servers`.

Note the two different assignment tokens. A **type declaration** uses `:` (`host: utf8`). An
**instance** uses `=` (`host = "localhost"`). Mixing them is the most common first mistake.

## `@block(kind, …)`

`@block("kind")` makes a type an authorable, nestable block. The kind string is the keyword.

Three optional named arguments constrain instances as a whole:

| Argument | Meaning |
| --- | --- |
| `required_fields = ["a", "b"]` | Each listed field must be written, even if the declaration makes it optional. |
| `required_children = ["kind"]` | Each listed child kind must appear at least once. |
| `max_children = N` | The **total** nested-block count must not exceed `N`. |

```wcl
@block("step") type Step { @inline(0) label: utf8 }

@block("recipe", required_children = ["step"], max_children = 2)
type Recipe {
  @inline(0) id: identifier
  @children("step") steps: list<Step>
}
```

```console
$ wcl check r.wcl
wcl::eval::schema_violation

  × block 'recipe' contains 3 children (max allowed: 2)
    ╭─[r.wcl:11:1]
 10 │
 11 │ ╭─▶ recipe tea {
 12 │ │     step "boil"
 13 │ │     step "steep"
 14 │ │     step "pour"
 15 │ ├─▶ }
    · ╰──── schema violation
    ╰────

r.wcl: 1 schema violation
```

`required_fields` is the escape hatch for a field the *type* must leave optional but an instance
must still write. The type leaves it optional because it is `@schemaless`, or because a subtype
fills it.

## `@inline(slot)` — positional labels

`@inline(N)` moves a field out of the braces and binds it to the block's zero-based label slot.

```wcl
@block("route") type Route {
  @inline(0) method: utf8
  @inline(1) path: utf8
  @default(200) status: u16
}

route "GET"  "/users"
route "POST" "/users" { status = 201 }
```

A field typed `identifier` in slot 0 is by convention the block's **id**: two sibling blocks of
that kind may not share it (`DuplicateBlockId`). A label of any other type — `code wcl`,
`li`, a `route`'s method string — repeats freely.

`@inline` means the same thing on a decorator schema: it makes the argument positional. See
[`lang_decorators.md`](lang_decorators.md).

## `@child` and `@children`

`@child("kind")` holds **one** nested block; `@children("kind")` holds a **list** of them.

```wcl
@block("meta")  type Meta  { owner: utf8 }
@block("route") type Route { @inline(0) method: utf8  @inline(1) path: utf8 }

@block("service") type Service {
  @inline(0) id: identifier
  @child("meta")     meta:   Meta?          // optional: `Meta` without `?` is required
  @children("route") routes: list<Route>
}
```

- `@child(K)` on a non-optional field means exactly one; on an optional field, zero or one.
- `@children(K)` accepts `min` and `max`:

  ```wcl
  @children("step", min = 2, max = 3) steps: list<Step>
  ```

  ```console
  $ wcl check p.wcl
  wcl::eval::schema_violation

    × field 'steps' requires at least 2 'step' children, found 1
      ╭─[p.wcl:10:1]
    9 │
   10 │ ╭─▶ plan deploy {
   11 │ │     step "build"
   12 │ ├─▶ }
      · ╰──── schema violation
      ╰────

  p.wcl: 1 schema violation
  ```

- A child kind no field declares is refused outright:

  ```console
  × block kind 'note' is not allowed inside 'recipe'
  ```

- If the field's element type is a **union**, each instance is dispatched to the matching
  variant by record shape. See [`lang_types.md`](lang_types.md).
- A schema may hold a string-kind slot and a union slot side by side. A nested block whose kind
  a string-kind slot names belongs to that slot and never reaches union dispatch. Every other
  nested block is dispatched:

  ```wcl
  @block("mixed") type Mixed {
    @child("config")  cfg:    ConfigSpec      // takes `config { … }`
    @children(Shape)  shapes: list<Shape>     // takes `circle { … }`, `square { … }`
  }
  ```

## `@default(expr)`

`@default(expr)` supplies the value a field takes when the author omits it, and makes the field
optional. The default belongs to the *evaluated* view — a reader never has to know it was not
written:

```wcl
@block("server") type Server {
  @default(8080) port: u16
  @default([]) tags: list<utf8>
}
```

```console
$ wcl get config.wcl servers.web.port
8080
```

## `extends` — a kind that inherits fields

An instance is validated against its type's **effective** fields: its own plus every field
inherited through `extends`, at any depth, from a `type` or an `interface`. An inherited field is
written, type-checked, defaulted and label-bound like an own one. Do not redeclare a field just
to make it writable.

```wcl
type Part    { vendor: utf8  @default(64u32) width: u32 }
type CpuBase extends Part { features: list<utf8> }

@block("cpu", required_fields = ["vendor"])
type Cpu extends CpuBase { @inline(0) id: identifier  cmov: bool }

@document type Cfg { @children("cpu") cpus: list<Cpu> }

cpu p6 { vendor = "intel"  features = ["x87"]  cmov = true }
```

```console
$ wcl check cpus.wcl
OK
$ wcl get cpus.wcl cpus.p6.width
64u32
```

| Situation | Rule |
| --- | --- |
| Child redeclares an inherited field | The child's declaration wins **whole**: type, `?`, every decorator. Nothing merges, so a dropped `@default` / `@min` is gone. |
| Redeclaration with a different type | Open-time error: `conflicting type for field 'f' in 'T' (across extends)`. Optionality is not compared, so `u32?` over `u32` is fine. |
| Two parents declare one name | Types must agree; the **later** parent in the `extends` list supplies the declaration. |
| `@inline` across the set | Slots combine (`Named { @inline(0) name }` + child `@inline(1) version`). Two *different* names on one slot: `fields 'a' and 'b' of 'T' both claim @inline(0)`. Same name on the same slot is an override. |
| Parent in another file | Checked the same: every eager import, transitively, the wdoc stdlib included, and chains that cross files. The caret is on the child's field in its file; the message gains `; 'Part' in /abs/parts.wcl declares it as u32` (slot clash: `... declares 'id'`). Cycles may cross files: `cyclic extends graph involving 'A'`. Same exit code (1) as the same-file case. Each conflict is reported once, on the type that introduces it. |
| `@table` type with a parent | Columns are the effective fields in order: inherited first, then own. |
| `@block(…)` arguments | Not inherited. `required_fields`, `max_children`, `required_children` bind to the type they are written on. |

Every check reads the effective set: unknown-field membership, `required_fields` names,
value-vs-type, symbol sets, unions, `@child` / `@children` slots and cardinality, `@schemaless`
on a field, the `@inline` id rule, decorator-argument schemas (`@decorator` types), and the
lazy check `wcl get` runs. `wcl check` and `wcl get` agree.

## `@table(kind)`

`@table("kind")` marks a **row** type as the schema for pipe-table syntax. Each `| … |` row is
matched against the type's fields in declaration order.

```wcl
@table("user") type User {
  name: utf8
  age:  u8
}

@document type Roster {
  users: list<User>
}

users:                    // the FIELD name, then a colon; rows follow
  | "Ada"   | 36 |
  | "Grace" | 45 |
```

Row arity and cell types are checked like any other field. Table kinds share the block-kind
namespace, so `@table` and `@block` cannot both claim one kind string in one namespace.

## `@schemaless`

`@schemaless` switches validation off, and it is an escape hatch rather than a convenience.

- On a **type**, it short-circuits every check for instances of that block: unknown fields,
  disallowed nested kinds, child quotas, table-row arity.
- On a **field**, it exempts only that field from the value-versus-type check. The rest of the
  block stays validated.
- `@schemaless(annotations = true)` exempts only the *decorators* on that node, leaving the
  values checked.

```wcl
// Whole block: `from` resolves to a body reference, not a static scalar.
@block("project") @schemaless
type Project {
  from: utf8
}

// Single field: rows may hold any scalar; the rest of the block is still checked.
@block("grid") type Grid {
  header: list<utf8>?
  @schemaless rows: list<list<utf8>>?
}
```

It applies to the **next declaration only**. Writing one `@schemaless` above several top-level
fields exempts the first and leaves the others reported as `has no @document schema`.

## `@document` — and how document schemas merge

`@document` marks the type that describes the file's top level. `wcl check` validates every
top-level field and block against it. A top-level value with no `@document` in scope is an
error (`NoDocumentSchema`).

**The effective document schema for a namespace is the merge of every `@document` type visible
there.** A top-level field or block is legal if *any* member declares it. This is deliberate. It
is what lets you `import <wdoc.wcl>` and still declare your own root `@document` to add
top-level tags of your own:

```wcl
import "./lib.wcl"          // declares @document lib.LibDoc with `title` and `widgets`

@block("note") type Note { @inline(0) id: identifier  text: utf8 }

@document type MyDoc {
  @children("note") notes: list<Note>
}

title = "mine"              // from the library's schema
widget w1 { label = "One" } // from the library's schema
note   n1 { text = "hi" }   // from mine
```

```console
$ wcl check m.wcl
OK
```

`MultipleDocumentSchemas` fires only on a **second root-authored** `@document` in a namespace.
Imported (library) ones merge silently:

```console
$ wcl check o.wcl
wcl::eval::schema_violation

  × type 'B' declares a second root @document schema (only one root-authored
  │ @document is allowed per namespace; imported library schemas merge
  │ automatically)
   ╭─[o.wcl:2:11]
 1 │ @document type A { x: i64 }
 2 │ @document type B { y: i64 }
   ·           ────────┬────────
   ·                   ╰── schema violation
 3 │
   ╰────

o.wcl: 1 schema violation
```

When both a root-authored and an imported schema declare the same field name, the
root-authored one is preferred for type checks.

### How a gather field collides

This is the failure that costs the most time.

Merged document schemas share **one flat space of field names**. Say your `@document` declares
a gather field (`@child` / `@children`) under a name a library's `@document` already uses. The
merge then resolves that name to one declaration only. The other schema's gathered blocks
silently vanish from anything that iterates the field. A template's `each = widgets` fails as
an *unresolved reference*, at build time, far from the cause.

`wcl check` reports this as a **warning**, not an error. Merging is a designed feature, and
existing documents must keep building. Warnings go to stderr and never change the exit code, so
read them:

```console
$ wcl check n.wcl
warning: gather field 'widgets' of @document 'MyDoc' (this document) collides with 'widgets'
declared by @document 'lib.LibDoc' (…/liba.wcl) — the merged document schema resolves 'widgets'
to only one declaration, so the other schema's gathered blocks silently vanish; rename one field
n.wcl: 1 warning
OK
```

The fix is always the same: rename your field. Prefix it if the vocabulary is generic
(`sw_components` rather than `components`).

## Reading a validation error

Every violation carries a span and a snippet through `miette`:

```console
$ wcl check config.wcl
wcl::eval::schema_violation

  × field 'host' declared as utf8 but value is i64
    ╭─[config.wcl:12:3]
 11 │ server web {
 12 │   host = 3
    ·   ────┬───
    ·       ╰── schema violation
 13 │ }
    ╰────

config.wcl: 1 schema violation
```

The messages you will meet most often:

| Message | Means |
| --- | --- |
| `top-level field 'x' has no @document schema` | No `@document` in scope declares `x`. |
| `field 'x' is not declared by schema 'T'` | Unknown field inside a block. |
| `block 'b' is missing required field 'x'` | Required by the type or by `required_fields`. |
| `block kind 'k' is not allowed inside 'b'` | No `@child` / `@children` slot accepts `k`. |
| `field 'f' requires at least N 'k' children, found M` | `@children(min = N)`. |
| `block 'b' contains N children (max allowed: M)` | `@block(max_children = M)`. |
| `field 'f' declared as T but value is U` | Value-versus-type mismatch. |
| `field 'f' declared as u8 but value 300 is out of range for u8` | A number that does not fit its numeric type. |
| `field 'f' declared as u8 but value 2.5 is not a whole number, so it cannot be u8` | A fraction for an integer type. |
| `block kind 'k' has no @block or @table declaration` | Unregistered kind — usually a missing import. |

`wcl check --json` emits the same errors plus the warnings as structured data.

## Gotchas

- Type declarations use `:`; instances use `=`.
- `@schemaless` covers the next declaration only.
- 23 type names are already taken in the root namespace by the built-in decorator schemas —
  `Table`, `Document`, `Block`, `Default` and friends. See
  [`lang_decorators.md`](lang_decorators.md).
- `max_children` counts **every** nested block, not the members of one slot.
- Extending a wdoc type holds you to its field types. `ContentBlock` and `SvgBlock` declare
  `id: identifier?`, so `id: utf8?` on a child is refused at open (`'wdoc.ContentBlock' in
  <wcl-system>/wdoc/core.wcl declares it as identifier`). Redeclare it as `identifier`.
- A conflict *inside* an imported file fails every document that imports it, even one that never
  names the types involved. The caret renders against the imported file's absolute path. The
  same holds for every other open-time declaration rule: an unknown field or signature type, an
  interface without `&`, a duplicate variant, an unknown `extends` parent. See
  [`lang_namespaces.md`](lang_namespaces.md) for how an imported file's names resolve.
- Declarations inside an in-block (lazy) `import` are not checked at open at all, neither the
  `extends` graph nor their type names; only top-level imports are.
- Redeclaring an inherited field to tweak one decorator drops the others. Copy every decorator
  you mean to keep.
- A gather field named like a library's silently loses blocks. It is a warning, and warnings do
  not fail the build.
- A number that does not fit its declared numeric type (`300` for `u8`) fails `wcl check`
  (exit 2) *and* any read of the field: `wcl get` exits 3 with the same message, and
  `Field::value` returns the `SchemaViolation`. See
  [`lang_types.md`](lang_types.md#a-number-must-fit-its-numeric-type).
- A unit literal its type does not declare (`5km` for `u32`, `1e39` for `f64`) behaves the same
  way, code `wcl::eval::unit_no_match` on both paths; so does a unit product that is fractional
  or out of range (`1.3B` for `std.ByteSize`). Top level, nested block, list element, imported
  file alike. See [`lang_values.md`](lang_values.md#literal-units).
- A field whose expression *fails to evaluate* is skipped by validation rather than reported.
  `wcl check` says `OK` for `n = error("boom")`. See
  [`lang_evaluation.md`](lang_evaluation.md).
