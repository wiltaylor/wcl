# Schemas and Decorators

Source of truth for decorators: `crates/wcl_lang/src/schema/decorator.rs` lines 52–410 (`register_builtins`).

## Schema Declaration

```wcl
schema "service" {
  port:     u16    @required
  region:   string @required
  env:      string @default("production")
  tags:     list   @optional
  replicas: u32    @validate(min = 1, max = 100)
}
```

Applies to blocks whose kind matches the schema name. Unknown fields are rejected unless `@open`.

## Field Types

```
string, i8, u8, i16, u16, i32, u32, i64, u64, i128, u128,
f32, f64, bool, date, duration, list, map, any, symbol, identifier
```

A field with no decorator is required by default (E070 if missing).

## Schema Reference Decorators

```wcl
# Inline ID pattern
schema "service" @id_pattern("^[a-z][a-z0-9-]*$") { ... }

# Open schema — allows unknown attributes
schema "config" @open { ... }

# Auto-generate an ID for anonymous sibling blocks
schema "rule" @auto_id { ... }

# Text-block form: body can be a string instead of attribute set
schema "note" { content: string @text }
# Usage:
#   note "this is the content"

# Inline arg mapping — positional inline arg N → named field
schema "service" {
  id:   identifier @inline(0)
  port: i64        @inline(1)
}
# Usage:
#   service api 8080 { ... }   # id=api, port=8080

# Parent/child constraints
schema "port" @parent(kinds = ["service"]) { weight: i64 }
schema "service" @children(kinds = ["port", "health"]) { ... }
schema "menu" @child(kind = "item", min = 1, max = 10, max_depth = 3) { ... }

# Tagged union discriminator
schema "event" @tagged(field = "kind") { ... }

# Symbol constraints
symbol_set http_methods { GET, POST, PUT, DELETE }
schema "endpoint" { method: symbol @symbol_set("http_methods") }
```

## Cross-References

```wcl
schema "service" { id: identifier @inline(0), port: u16 }
schema "deployment" {
  service_id: string @ref("service")
  region:     string
}

service api { port = 8080 }
deployment my_dep {
  service_id = "api"
}
```

- E076 / E074: `@ref` target not found.
- `@id_pattern` mismatch: E077.

## Validation Constraints

```wcl
port:      u16    @validate(min = 1, max = 65535)
slug:      string @validate(pattern = "^[a-z][a-z0-9-]*$")
env:       string @validate(one_of = ["dev", "staging", "prod"])
replicas:  i64    @validate(min = 1, max = 100, custom_msg = "must be 1–100")
```

- E073: constraint violation. `@validate` requires at least one of `min`, `max`, `pattern`, `one_of` (E064).

## Built-in Decorator Registry (24 total)

Every built-in decorator, from the registry. "Targets" column shows where it can be applied. Required params are bold.

| Decorator | Targets | Params |
|-----------|---------|--------|
| `@optional` | Schema | — |
| `@required` | Schema | — |
| `@default(value)` | Schema | **`value: any`** |
| `@sensitive(redact_in_logs?)` | Attribute | `redact_in_logs: bool = true` |
| `@deprecated(message, since?)` | Block, Attribute | **`message: string`**, `since: string` |
| `@validate(min?, max?, pattern?, one_of?, custom_msg?)` | Attribute | `min: f64`, `max: f64`, `pattern: string`, `one_of: list`, `custom_msg: string`. AnyOf(min, max, pattern, one_of). |
| `@doc(text)` | Block, Attribute, Table, Schema | **`text: string`** |
| `@allow(rule)` | Attribute, Block | **`rule: string`** |
| `@id_pattern(pattern)` | Schema | **`pattern: string`** |
| `@ref(schema)` | Schema | **`schema: string`** |
| `@open` | Schema | — |
| `@auto_id` | Schema | — |
| `@warning` | Block | — |
| `@partial_requires(fields)` | Block | **`fields: list(string)`** |
| `@merge_order(order)` | Block | **`order: i64`** |
| `@example` | Schema | — |
| `@text` | Schema | — |
| `@child(kind, min?, max?, max_depth?)` | Schema | **`kind: string`**, `min: i64`, `max: i64`, `max_depth: i64` |
| `@tagged(field)` | Schema | **`field: string`** |
| `@children(kinds)` | Schema | **`kinds: list(string)`** |
| `@parent(kinds)` | Schema | **`kinds: list(string)`** |
| `@symbol_set(name)` | Schema | **`name: string`** |
| `@embedded_lsp(language)` | Attribute | **`language: string`** |
| `@table_index(columns, unique?)` | Table | **`columns: list(string)`**, `unique: bool = false` |

### Notes on select decorators

- **`@inline(N)`** — NOT in the decorator registry; it's a parser-level modifier that maps positional arg N to a schema field. Used as `port: u16 @inline(1)`.
- **`@schema("name")`** — shorthand on a `table` block that sets its schema. Equivalent to `table x : name { ... }`.
- **`@sensitive`** — attributes are redacted in logs/output by default. Pass `redact_in_logs = false` to opt out.
- **`@warning`** — downgrades schema errors on the block to warnings.
- **`@open`** — relaxes unknown-attribute check (E072).
- **`@auto_id`** — auto-generates an ID for anonymous sibling blocks so they don't collide under E030.

## User-Defined Decorators

```wcl
decorator_schema "my_decorator" targets Block {
  label:   string @required
  enabled: bool   @default(true)
}

@my_decorator(label = "prod", enabled = true)
server web-1 { port = 8080 }
```

- E060 unknown decorator (after macro expansion, an unknown decorator is an error).
- E061 applied to wrong target.
- E062 missing required param.
- E063 param type mismatch.
- E064 constraint violation (AnyOf / AllOf / OneOf / Requires).

## Applying Decorators

```wcl
# On a block
@deprecated(message = "use newer service", since = "0.5.0")
server old-api { port = 8080 }

# On an attribute (before or after the type)
schema "user" {
  email: string @sensitive(redact_in_logs = true)
  role:  string @validate(one_of = ["admin", "user"])
  notes: string @optional @doc("Free-form notes")
}

# Multiple decorators chain
@doc("Internal service")
@deprecated(message = "use v2")
service legacy { port = 8001 }
```
