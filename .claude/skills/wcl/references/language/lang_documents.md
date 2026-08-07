# Documents, fields and blocks

A `.wcl` file is a **document**. It carries data and the schema that describes that data. You
read a document in two ways, and the two ways never mix:

- **Evaluated** — `wcl check`, `wcl get`, a host opening the file. Fields resolve to values.
  `let` items and comments are gone. This is what a consumer sees.
- **For edit** — `wcl set`, `wcl fmt`, an editor. The syntax tree survives whole, comments
  included. This is what a tool that rewrites the file sees.

This chapter covers the surface you write: the top-level forms, fields, blocks, labels,
nesting, `table` items, `self` / `parent`, and `let` items.

## The top-level forms

A document is a sequence of **items**. There are thirteen forms, and everything else in the
language sits inside one of them.

| Form | Written | Covered in |
| --- | --- | --- |
| Field | `name = expr` | this chapter |
| Block | `kind "label" { … }` | this chapter |
| Table | `name:` then `\| … \|` rows | this chapter |
| `let` item | `let name = expr` | this chapter |
| Type declaration | `type Name { … }` / `type Name = TypeRef` | `lang_types.md` |
| Interface declaration | `interface Name { … }` | `lang_types.md` |
| Union declaration | `union Name { … }` | `lang_types.md` |
| Symbol-set declaration | `symbol_set Name { … }` | `lang_types.md` |
| Namespace declaration | `namespace a.b` | `lang_namespaces.md` |
| `use` declaration | `use a.b.C` | `lang_namespaces.md` |
| Import | `import "./x.wcl"` / `import <wdoc.wcl>` | `lang_namespaces.md` |
| Connection declaration | `connection Uses : A -> B : path` | `lang_connections.md` |
| Connection statement | `a -> b` | `lang_connections.md` |

There is no statement separator. An item ends where the next one starts.

Order does not matter. A field may name a type declared below it, and an import may sit at the
bottom of the file. Name resolution runs over the whole document after it parses.

## Comments

Two line-comment forms, `//` and `#`. Both run to the end of the line. There are **no block
comments**.

```wcl
// A leading comment describes the next item.
service "web" {
  port   = 8080u32   // a trailing comment stays on its line
  # hash comments are the same thing
  region = "us-east-1"
}
```

Comments and blank-line grouping survive `wcl fmt` and the `wcl set` edit path. One thing does
not survive: **`wcl fmt` rewrites every comment with a `#` prefix.** The prefix style is not
preserved, so a formatted file has no `//` comments left. Two or more blank lines between
items collapse to one.

A comment is not documentation. To attach prose that a tool can read back, use the `@doc`
decorator — see `lang_decorators.md`.

## Fields

A field binds a name to a value with `=`. The value is any expression: a literal, a reference
to another field, a call, arithmetic, a `match`, or a function.

```wcl
name    = "alpha"
count   = 3u32
enabled = true
ratio   = count / 2u32
label   = $"${name}-${count}"
```

Fields are the leaves of a document. Blocks, types and schemas exist to group and constrain
them.

> **`=` writes a value, `:` declares a type.** Inside a `type`, an `interface` or a record
> literal you write `name: Type` or `name: value`. Where you write *data* — a top-level field,
> a field inside a block — you write `name = expr`. Confusing the two is the most common
> first-day parse error.

## Blocks

A block is a named group of fields that can also nest other blocks. The block **kind** is the
first word. The schema decides which fields and how many labels a kind takes — see
`lang_schemas.md`.

```wcl
service "web" {
  port   = 8080u32
  region = "us-east-1"
}
```

A kind may be namespace-qualified:

```wcl
wdoc::process "build" {
  step = "compile"
}
```

### Labels

Everything between the kind and the `{` is a **label**. A label binds to whichever field the
schema marks `@inline(N)`, by position.

```wcl
// @inline(0) id  →  id = "web"
service "web" { }

// @inline(0) verb, @inline(1) path  →  verb = "GET", path = "/users"
route "GET" "/users" {
  handler = "list_users"
}
```

Four label spellings, all filling the same field:

```wcl
class "dgm-box" { }             // quoted string
class dgm-box { }               // bare — `-` and `/` may connect name parts
page api/v1/users { }           // so kebab-case and paths need no quotes
tab $"panel-${index}" { }       // interpolated; evaluated in the block's scope
```

A newline ends the label list. What follows a newline is a new item, never another label.

### Nesting

Blocks hold blocks. Nesting depth is unbounded; the schema constrains the shape with `@child`
(exactly one) and `@children` (a list).

```wcl
service "web" {
  metadata {
    region = "us-east-1"
    tags {
      environment = "prod"
    }
  }
}
```

A block the schema does not declare, or a field the block's type does not have, is a schema
violation. `wcl check` reports it with the span.

## Table items

A `table` item writes many records of one shape as pipe rows. It needs two things:

1. The row type carries `@table("kind")`.
2. The field it fills is **gathered** — declared with `@children("kind")`, exactly like a nested
   block family. Without the `@children`, the rows parse and validate but nothing reads them.

The field is introduced with a **colon** rather than `=`, because the rows follow on the lines
beneath it.

```wcl
@table("user")
type User {
  name:    utf8
  age:     u32
  enabled: bool
}

@document
type Config {
  @children("user") users: list<User>
}

users:
  | "alice" | 30 | true  |
  | "bob"   | 25 | false |
  | "cara"  | 42 | true  |
```

Each `| … |` row becomes one `User`. Cells are **expressions** in the row type's field
positions, not raw text — a cell may be a string, a number, a symbol, or a computed value.
Columns bind in declaration order, and `wcl check` validates each cell against its column's
type.

A gathered table is not a leaf value, so `wcl get` on the field answers `not_a_leaf`. The rows
are there for whatever consumes the document — a host, or a template that iterates the gather —
not for other expressions in the same file.

## `self` and `parent`

Inside a block, `self` names the current block and `parent` names the block one level out.
Both walk the lexical scope outward, so a field can read a sibling or an ancestor without
naming a path from the root.

```wcl
box "panel" {
  width  = 480.0
  height = 270.0
  ratio  = self.width / self.height     // this block's own fields
}

service "web" {
  region = "us-east-1"
  metadata {
    inherited_region = parent.region    // one level out
  }
}
```

`parent` at the document root is an error: there is no scope above it. Both resolve lazily,
like any other reference, so the order of fields inside the block does not matter.

## `let` items — helpers, not data

`let name = expr` at file scope or inside a block introduces a name that sibling and
descendant expressions can use. It has **no terminator**.

```wcl
let base_port = 8080u32

service "web" { port = base_port }
service "api" { port = base_port + 1u32 }
```

A `let` binds anything an expression can produce, functions included, which is what makes it a
composition helper:

```wcl
let scale = fn(p: f64) -> f64 p * 2.0

a = scale(3.0)    // 6.0
```

**A `let` item is deliberately invisible to the evaluated document.** It does not appear in:

- the document's field or block lists
- the symbol index
- a `wcl get` path
- JSON output
- schema validation

That is the point: a `let` is scaffolding for the author, and a consumer never learns it
existed. If a value must reach a consumer, it has to be a field.

Do not confuse the `let` **item** with the `let … ;` **binding** inside a `{ … }` block
expression. The item has no semicolon and lives at item scope; the binding has one and lives
inside a single expression. See `lang_control_flow.md`.

## Reading a document back

`wcl get` (aliased `wcl eval`) resolves a dotted path from the document root. A gathered block
list is addressed by **label**, not by position.

```console
$ wcl get config.wcl services.web.region
"us-east-1"
$ wcl get config.wcl services.web.metadata.tags.environment
"prod"
```

A numeric path segment is still a label match. `steps.1` means "the block labelled `1`", not
"the second block". There is no positional index operator in WCL at all — see
`lang_expressions.md`.

A path must end at a leaf value. `wcl get config.wcl services` fails with `not_a_leaf`, because
a gathered block list is not a scalar. Add `--json` for machine-readable output.

> **A path walks the document tree, and stops at a field.** It descends through blocks, nested
> blocks and gathered lists (by label), and reads the field it lands on. It does **not** continue
> into that field's *value*: if `meta` holds a record, `meta.region` is an unresolved reference,
> from `wcl get` and from another field alike. Bind the value to a local first, or destructure it
> — see `lang_expressions.md`.

## A worked example

One file that uses every form in this chapter.

```wcl
// Ports come from one base, so a redeploy moves them together.
let base_port = 8000u32

@table("route")
type Route {
  verb: utf8
  path: utf8
}

@block("service") type Service {
  @inline(0) id: identifier
  region: utf8
  offset: u32
  port:   u32
  @children("route") routes: list<Route>
}

@document type Config {
  @children("service") services: list<Service>
}

service web {
  region = "us-east-1"
  offset = 0u32
  port   = base_port + self.offset

  routes:
    | "GET"  | "/users" |
    | "POST" | "/users" |
}

service api {
  region = "us-east-1"
  offset = 1u32
  port   = base_port + self.offset

  routes:
    | "GET" | "/health" |
}
```

```console
$ wcl check config.wcl
OK
$ wcl get config.wcl services.api.port
8001u32
$ wcl get config.wcl services.web.region
"us-east-1"
$ wcl get config.wcl base_port
no such path: base_port
```

`base_port` is unreachable because it is a `let`, not a field. `port` answers, and it answers
with its width suffix — `wcl get` prints a typed value, not a bare number.

## Where to go next

- `lang_values.md` — what you can write on the right of an `=`.
- `lang_schemas.md` — `@block`, `@document`, `@inline`, `@children` and the rest of the
  vocabulary this chapter used without explaining.
- `lang_evaluation.md` — the evaluate-vs-edit split in full, and the error model.
