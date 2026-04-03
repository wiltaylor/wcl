# Schemas

Schemas define the expected shape of blocks — their fields, types, and constraints. When a schema exists with the same name as a block type, WCL automatically validates every block of that type against it.

## Syntax

```wcl
schema "service" {
    port:     u16    @required
    region:   string @required
    env:      string @default("production")
    tags:     list   @optional
    replicas: u32    @validate(min = 1, max = 100)
}
```

A schema body is a list of field declarations. Each field has a name, a type, and zero or more decorators.

## Matching Blocks to Schemas

A schema named `"service"` is automatically applied to every `service` block in the document:

```wcl
service "api" {
    port    = 8080
    region  = "us-east-1"
    env     = "staging"
    replicas = 3
}
```

Matching is done by block type name. There is no explicit `@schema` annotation needed on the block.

## Field Types

The following primitive types are available for schema fields:

| Type       | Description                                          |
|------------|------------------------------------------------------|
| `string`   | A string value                                       |
| `i8`       | Signed 8-bit integer (-128 to 127)                   |
| `u8`       | Unsigned 8-bit integer (0 to 255)                    |
| `i16`      | Signed 16-bit integer (-32,768 to 32,767)            |
| `u16`      | Unsigned 16-bit integer (0 to 65,535)                |
| `i32`      | Signed 32-bit integer (-2^31 to 2^31-1)             |
| `u32`      | Unsigned 32-bit integer (0 to 2^32-1)               |
| `i64`      | Signed 64-bit integer (-2^63 to 2^63-1)             |
| `u64`      | Unsigned 64-bit integer (0 to 2^64-1)               |
| `i128`     | Signed 128-bit integer (-2^127 to 2^127-1)          |
| `u128`     | Unsigned 128-bit integer (0 to 2^128-1)             |
| `f32`      | 32-bit floating-point (IEEE 754 single precision)    |
| `f64`      | 64-bit floating-point (IEEE 754 double precision)    |
| `bool`     | A boolean value                                      |
| `date`     | ISO 8601 date (YYYY-MM-DD), literal syntax `d"2024-03-15"` |
| `duration` | ISO 8601 duration, literal syntax `dur"P1Y2M3D"`    |
| `list`     | A list of values                                     |
| `map`      | A key-value map                                      |
| `any`      | Accepts any value type                               |
| `symbol`   | A symbol literal (e.g. `:GET`)                       |

## Field Decorators

### @required

Fields are required by default. You may add `@required` explicitly for clarity:

```wcl
schema "database" {
    host: string @required
    port: u16    @required
}
```

### @optional

Marks a field as not required. If the field is absent from the block, no error is raised:

```wcl
schema "service" {
    debug_port: u16 @optional
}
```

### @default(value)

Provides a default value used when the field is absent. Implies `@optional`:

```wcl
schema "service" {
    env:      string @default("production")
    replicas: u32    @default(1)
}
```

### @validate(...)

Attaches constraints to a field's value:

```wcl
schema "service" {
    port:     u16    @validate(min = 1, max = 65535)
    env:      string @validate(one_of = ["development", "staging", "production"])
    name:     string @validate(pattern = "^[a-z][a-z0-9-]*$")
    replicas: u32    @validate(min = 1, max = 100, custom_msg = "replicas must be between 1 and 100")
}
```

Available constraint arguments:

| Argument     | Applies to       | Description                              |
|--------------|------------------|------------------------------------------|
| `min`        | numeric types    | Minimum value (inclusive)                |
| `max`        | numeric types    | Maximum value (inclusive)                |
| `pattern`    | string           | Regex pattern the value must match       |
| `one_of`     | string, numeric  | Value must be one of the listed options  |
| `custom_msg` | any              | Custom error message on violation        |

## Cross-References with @ref

Use `@ref("schema_name")` on an identifier field to require that the referenced value points to a valid block of the named type:

```wcl
schema "deployment" {
    service_id: string @ref("service")
    region_id:  string @ref("region")
}
```

When a `deployment` block has `service_id = "api"`, WCL verifies that a `service api` block exists in the document.

### Qualified IDs

Nested blocks receive **qualified IDs** formed by joining ancestor inline IDs with dots. For example:

```wcl
service alpha {
    port http {
        weight = 100
    }
}
```

Here `http` has the qualified ID `alpha.http`. Qualified IDs are globally unique across all block kinds within a scope — you cannot have `service alpha` and `deployment alpha` in the same document.

### Scoped Resolution

When a `@ref` field value is resolved, WCL tries multiple strategies:

1. **Bare ID** — matches any block of the target kind by its inline ID.
2. **Peer lookup** — if the reference is inside a block, bare names are also tried as peers (siblings in the same parent scope).
3. **Qualified path** — dotted paths like `"alpha.http"` resolve from the root.
4. **Relative path** — `"../beta"` navigates up one level from the current block's parent scope, then resolves `beta` there.

### Ref Expressions

The `ref()` expression resolves a block reference at evaluation time:

```wcl
service alpha {
    port http { weight = 100 }
    port grpc { weight = 50 }
}

// Bare ID
let svc = ref(alpha)

// Qualified path (string syntax for dots)
let p = ref("alpha.http")

// Relative path from inside a block
service beta {
    sibling_port = ref("../alpha.http")
}
```

Namespaces (`namespace foo::bar`) do not affect qualified IDs — they only qualify block kind names via `::` syntax.

## ID Naming Conventions with @id_pattern

Use `@id_pattern("glob")` on a schema's identifier field to enforce naming conventions on block IDs:

```wcl
schema "service" @id_pattern("svc-*") {
    port: u16
}
```

Any `service` block whose ID does not match the glob `svc-*` will produce a validation error.

## Nested Schema References with ref()

Use `ref("other_schema")` as a field type to require that the field's value conforms to another schema:

```wcl
schema "address" {
    street: string
    city:   string
    zip:    string @validate(pattern = "^[0-9]{5}$")
}

schema "contact" {
    name:    string
    address: ref("address")
}
```

## Open vs Closed Schemas

By default schemas are **closed**: any attribute present in a block but not declared in the schema produces an error (error code E072).

Add the `@open` decorator to allow extra attributes:

```wcl
schema "service" @open {
    port:   u16
    region: string
}
```

An open schema validates all declared fields but silently permits additional attributes not listed in the schema.

## Validation Timing

Schema validation runs at **phase 9** of the WCL pipeline, after:

1. Import resolution
2. Macro expansion
3. Control flow expansion (for/if)
4. Partial merging
5. Scope construction and evaluation

This means schema validation sees the fully resolved document. Computed values, macro-generated blocks, and merged partials are all validated.

## Accumulative Error Reporting

Schema validation is accumulative. All violations across all blocks are collected before reporting, so you see every error in a single pass rather than stopping at the first failure.

## Composition: ref() and Partials

WCL schemas do not support inheritance. Instead, use two composition mechanisms:

- **`ref("schema")`** — reference another schema as a field type.
- **Partials** — share common attribute groups across blocks and merge them before validation runs.

```wcl
schema "base_service" {
    port:   u16
    region: string
}

schema "web_service" {
    base:   ref("base_service")
    domain: string
    tls:    bool @default(true)
}
```

## Full Example

```wcl
schema "service" @id_pattern("svc-*") {
    port:     u16    @required @validate(min = 1, max = 65535)
    region:   string @required @validate(one_of = ["us-east-1", "eu-west-1", "ap-south-1"])
    env:      string @default("production") @validate(one_of = ["development", "staging", "production"])
    tags:     list   @optional
    replicas: u32    @default(1) @validate(min = 1, max = 100)
}

service "svc-api" {
    port     = 8080
    region   = "us-east-1"
    env      = "staging"
    tags     = ["web", "critical"]
    replicas = 3
}

service "svc-worker" {
    port   = 9090
    region = "eu-west-1"
}
```

The `svc-worker` block inherits `env = "production"` and `replicas = 1` from the schema defaults.

## Per-Child Cardinality with @child

Use `@child("kind", min=N, max=N)` to enforce how many children of a given kind a block must/may have:

```wcl
schema "server" {
    @child("endpoint", min=1, max=10)
    @child("config", max=1)
    port: u16
    host: string
}
```

- `min` — error if fewer children of that kind exist (E097)
- `max` — error if more children of that kind exist (E098)
- `@child("kind")` with no min/max just adds the kind to the allowed children set (like `@children`)
- `@child` entries merge into the `@children` constraint automatically

### Self-Nesting with max_depth

Use `@child("kind", max_depth=N)` to allow a block to contain itself, up to a depth limit:

```wcl
schema "menu" {
    @child("menu", max_depth=3)
    label: string
}

menu top {
    label = "File"
    menu sub {
        label = "Open"
        menu deep {
            label = "Recent"  // depth 3 — allowed
            // menu too-deep { ... }  // ERROR E099: exceeds max depth
        }
    }
}
```

## Union Field Types

Use `union(t1, t2, ...)` to declare that a field accepts any of the listed types:

```wcl
schema "config" {
    value: union(string, i64, bool)
}

config a { value = "hello" }
config b { value = 42 }
config c { value = true }
```

## Tagged Variant Schemas

Use `@tagged("field")` and `variant "value" { ... }` to define schemas where required fields depend on a discriminator value:

```wcl
@tagged("style")
schema "api" {
    style: string
    version: string @optional

    @children(["resource"])
    variant "rest" {
        base_path: string
    }

    @children(["gql_query", "gql_mutation"])
    variant "graphql" {
        schema_path: string @optional
    }
}

api rest-api {
    style = "rest"
    base_path = "/api/v1"
}

api gql-api {
    style = "graphql"
}
```

- Common fields (outside variants) apply to all blocks
- When the tag field matches a variant, that variant's fields are also validated
- When no variant matches, only common fields are validated
- Variant `@children`/`@child` decorators override the base schema's containment for that variant
- Variant fields are accepted by closed schemas even when not in the active variant

## Symbols

Symbol literals are lightweight, identifier-like values prefixed with a colon. They are useful when a field represents a fixed set of named options rather than arbitrary strings.

### Symbol Literals

A symbol literal is written as a colon followed by an identifier:

```wcl
endpoint list_users {
    method = :GET
    path   = "/users"
}
```

Symbol values are distinct from strings. `:GET` is not the same as `"GET"`.

### Symbol Sets

A `symbol_set` declaration defines a named group of valid symbols:

```wcl
symbol_set http_method {
    :GET
    :POST
    :PUT
    :PATCH
    :DELETE
    :HEAD
    :OPTIONS
}
```

### Value Mappings

Each member of a symbol set can optionally map to a string value using `=`. This controls how the symbol serializes to JSON:

```wcl
symbol_set curl_option {
    :unix_socket = "unix-socket"
    :compressed  = "compressed"
    :verbose     = "verbose"
}
```

Without an explicit mapping, a symbol serializes to its name as a string (e.g. `:GET` becomes `"GET"` in JSON output).

### Using @symbol_set in Schemas

Use the `symbol` type and the `@symbol_set` decorator to constrain a field to members of a declared set:

```wcl
schema "endpoint" {
    method: symbol @symbol_set("http_method")
    path:   string
}
```

If a block provides a symbol value that is not a member of the referenced set, error E100 is raised. If the named set does not exist, error E101 is raised.

### The Special "all" Set

Use the set name `"all"` to accept any symbol value without restricting to a specific set:

```wcl
schema "tag" {
    kind: symbol @symbol_set("all")
}

tag important {
    kind = :priority    // any symbol is accepted
}
```

This is useful when you want the `symbol` type for its semantics (not a free-form string) but do not want to enumerate every valid value.

### JSON Serialization

Symbols serialize to JSON as strings:

- A symbol with no value mapping serializes to its identifier name: `:GET` becomes `"GET"`.
- A symbol with a value mapping serializes to the mapped string: `:unix_socket = "unix-socket"` becomes `"unix-socket"`.

```wcl
symbol_set http_method { :GET :POST }

endpoint example {
    method = :GET
}

// JSON output:
// { "endpoint": { "example": { "method": "GET" } } }
```

### Symbol Error Codes

| Code | Meaning |
|------|---------|
| E100 | Symbol value not in declared `symbol_set` |
| E101 | Referenced `symbol_set` does not exist |
| E102 | Duplicate `symbol_set` name |
| E103 | Duplicate symbol within a `symbol_set` |

## Error Codes

| Code | Meaning                                       |
|------|-----------------------------------------------|
| E001 | Duplicate schema name                         |
| E030 | Duplicate block ID                            |
| E070 | Missing required field                        |
| E071 | Type mismatch                                 |
| E072 | Unknown attribute in closed schema            |
| E073 | min/max constraint violation                  |
| E074 | Pattern constraint violation                  |
| E075 | one_of constraint violation                   |
| E076 | @ref target not found                         |
| E077 | @id_pattern mismatch                          |
| E080 | Validation block failure                      |
| E092 | Inline columns defined when schema is applied |
| E095 | Child not allowed by parent's `@children` list |
| E096 | Item not allowed by its own `@parent` list     |
| E097 | Child count below `@child` minimum            |
| E098 | Child count above `@child` maximum            |
| E099 | Self-nesting exceeds `@child` max_depth       |
| E100 | Symbol value not in declared `symbol_set`      |
| E101 | Referenced `symbol_set` does not exist          |
| E102 | Duplicate `symbol_set` name                    |
| E103 | Duplicate symbol within a `symbol_set`         |

## Block & Table Containment

Use `@children` and `@parent` decorators on schemas to constrain which blocks and tables can nest inside which others.

### @children — restrict what a block may contain

```wcl
@children(["endpoint", "table:user_row"])
schema "service" {
    name: string
}

service "api" {
    name = "my api"
    endpoint health { path = "/health" }     // allowed
    table users : user_row { | "Alice" | }   // allowed
    // logger { level = "info" }             // ERROR E095
}
```

Use `@children([])` to create a leaf block that cannot contain any children.

### @parent — restrict where a block may appear

```wcl
@parent(["service", "_root"])
schema "endpoint" {
    path: string
}
```

The special name `"_root"` refers to the document's top level. Use a schema named `"_root"` with `@children` to constrain what appears at the top level:

```wcl
@children(["service", "config"])
schema "_root" {}
```

### Table containment

Define virtual schemas named `"table"` or `"table:X"` to constrain table placement:

```wcl
@parent(["data"])
schema "table:user_row" {}

@parent(["_root"])
schema "table" {}
```

See [Built-in Decorators](decorators-builtin.md#childrenkinds) for full details.

## Applying Schemas to Tables

You can apply a schema to a table using the colon syntax or the `@schema` decorator:

```wcl
schema "user_row" {
    name : string
    age  : u32
}

# Colon syntax
table users : user_row {
    | "Alice" | 30 |
}

# Decorator syntax
@schema("user_row")
table contacts {
    | "Bob" | 25 |
}

# With CSV import
table imported : user_row = import_table("users.csv")
```

When a schema is applied, inline column declarations are not allowed (E092).
