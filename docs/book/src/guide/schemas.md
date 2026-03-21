# Schemas

Schemas define the expected shape of blocks — their fields, types, and constraints. When a schema exists with the same name as a block type, WCL automatically validates every block of that type against it.

## Syntax

```wcl
schema "service" {
    port:     int    @required
    region:   string @required
    env:      string @default("production")
    tags:     list   @optional
    replicas: int    @validate(min = 1, max = 100)
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

| Type     | Description                        |
|----------|------------------------------------|
| `string` | A string value                     |
| `int`    | An integer value                   |
| `float`  | A floating-point value             |
| `bool`   | A boolean value                    |
| `list`   | A list of values                   |
| `map`    | A key-value map                    |
| `any`    | Accepts any value type             |

## Field Decorators

### @required

Fields are required by default. You may add `@required` explicitly for clarity:

```wcl
schema "database" {
    host: string @required
    port: int    @required
}
```

### @optional

Marks a field as not required. If the field is absent from the block, no error is raised:

```wcl
schema "service" {
    debug_port: int @optional
}
```

### @default(value)

Provides a default value used when the field is absent. Implies `@optional`:

```wcl
schema "service" {
    env:      string @default("production")
    replicas: int    @default(1)
}
```

### @validate(...)

Attaches constraints to a field's value:

```wcl
schema "service" {
    port:     int    @validate(min = 1, max = 65535)
    env:      string @validate(one_of = ["development", "staging", "production"])
    name:     string @validate(pattern = "^[a-z][a-z0-9-]*$")
    replicas: int    @validate(min = 1, max = 100, custom_msg = "replicas must be between 1 and 100")
}
```

Available constraint arguments:

| Argument     | Applies to       | Description                              |
|--------------|------------------|------------------------------------------|
| `min`        | int, float       | Minimum value (inclusive)                |
| `max`        | int, float       | Maximum value (inclusive)                |
| `pattern`    | string           | Regex pattern the value must match       |
| `one_of`     | string, int      | Value must be one of the listed options  |
| `custom_msg` | any              | Custom error message on violation        |

## Cross-References with @ref

Use `@ref("schema_name")` on an identifier field to require that the referenced value points to a valid block of the named type:

```wcl
schema "deployment" {
    service_id: string @ref("service")
    region_id:  string @ref("region")
}
```

When a `deployment` block has `service_id = "api"`, WCL verifies that a `service "api"` block exists in the document.

## ID Naming Conventions with @id_pattern

Use `@id_pattern("glob")` on a schema's identifier field to enforce naming conventions on block IDs:

```wcl
schema "service" @id_pattern("svc-*") {
    port: int
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
    port:   int
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
    port:   int
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
    port:     int    @required @validate(min = 1, max = 65535)
    region:   string @required @validate(one_of = ["us-east-1", "eu-west-1", "ap-south-1"])
    env:      string @default("production") @validate(one_of = ["development", "staging", "production"])
    tags:     list   @optional
    replicas: int    @default(1) @validate(min = 1, max = 100)
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
    age  : int
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
