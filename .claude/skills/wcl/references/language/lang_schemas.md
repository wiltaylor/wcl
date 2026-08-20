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
  ```

- A child kind no field declares is refused outright:

  ```console
  × block kind 'note' is not allowed inside 'recipe'
  ```

- If the field's element type is a **union**, each instance is dispatched to the matching
  variant by record shape. See [`lang_types.md`](lang_types.md).

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
| `block kind 'k' has no @block or @table declaration` | Unregistered kind — usually a missing import. |

`wcl check --json` emits the same errors plus the warnings as structured data.

## Gotchas

- Type declarations use `:`; instances use `=`.
- `@schemaless` covers the next declaration only.
- 23 type names are already taken in the root namespace by the built-in decorator schemas —
  `Table`, `Document`, `Block`, `Default` and friends. See
  [`lang_decorators.md`](lang_decorators.md).
- `max_children` counts **every** nested block, not the members of one slot.
- A gather field named like a library's silently loses blocks. It is a warning, and warnings do
  not fail the build.
- A field whose expression *fails to evaluate* is skipped by validation rather than reported.
  `wcl check` says `OK` for `n = error("boom")`. See
  [`lang_evaluation.md`](lang_evaluation.md).
