# Built-in Decorators

WCL provides a set of built-in decorators for schema validation, documentation, macro transforms, and configuration semantics.

## Reference Table

| Decorator              | Targets                    | Arguments                        | Description                                               |
|------------------------|----------------------------|----------------------------------|-----------------------------------------------------------|
| `@optional`            | schema fields              | none                             | Field is not required                                     |
| `@required`            | schema fields              | none                             | Field must be present (default for schema fields)         |
| `@default(value)`      | schema fields              | `value`: any                     | Default value when field is absent                        |
| `@sensitive`           | attributes                 | `redact_in_logs`: bool = `true`  | Marks value as sensitive; redacted in log output          |
| `@deprecated`          | blocks, attributes         | `message`: string, `since`: string (optional) | Warns when this item is used                 |
| `@validate(...)`       | attributes, schema fields  | `min`, `max`, `pattern`, `one_of`, `custom_msg` | Value constraints                          |
| `@doc(text)`           | any                        | `text`: string                   | Inline documentation for the decorated item               |
| `@example { }`         | decorator schemas, schemas | block body                       | Embedded usage example                                    |
| `@allow(rule)`         | let bindings, attributes   | `rule`: string                   | Suppresses a specific warning                             |
| `@id_pattern(glob)`    | schemas                    | `glob`: string                   | Enforces naming convention on block IDs                   |
| `@ref(schema)`         | schema identifier fields   | `schema`: string                 | Requires value to reference an existing block of that type|
| `@partial_requires`    | partial blocks             | `fields`: list of strings        | Declares expected merge dependencies                      |
| `@merge_order(n)`      | partial blocks             | `n`: int                         | Explicit ordering for partial merges                      |
| `@open`                | schemas                    | none                             | Allows extra attributes not declared in the schema        |
| `@children(kinds)`     | schemas                    | `kinds`: list of strings         | Restricts which child blocks/tables may appear inside     |
| `@parent(kinds)`       | schemas                    | `kinds`: list of strings         | Restricts which parent blocks may contain this block/table|

---

## @optional

Marks a schema field as not required. If the field is absent from a block, no error is raised.

```wcl
schema "service" {
    debug_port: int    @optional
    log_level:  string @optional
}
```

## @required

Marks a schema field as required. This is the default for all schema fields, but can be written explicitly for clarity.

```wcl
schema "service" {
    port:   int    @required
    region: string @required
}
```

## @default(value)

Provides a fallback value when the field is absent from the block. A field with `@default` is implicitly optional.

```wcl
schema "service" {
    env:      string @default("production")
    replicas: int    @default(1)
    tls:      bool   @default(true)
}
```

The default value must be a valid WCL expression and must match the declared field type.

## @sensitive

Marks an attribute's value as sensitive. Tools and log output should redact this value.

```wcl
database "primary" {
    host     = "db.internal"
    password = "s3cr3t" @sensitive
    api_key  = "change-me" @sensitive(redact_in_logs = true)
}
```

The optional `redact_in_logs` argument defaults to `true`.

## @deprecated

Indicates that a block or attribute is deprecated. A warning is emitted when it is used.

```wcl
service "legacy-api" @deprecated(message = "Use service 'api-v2' instead", since = "3.0") {
    port = 8080
}
```

On an attribute:

```wcl
schema "service" {
    workers:  int @deprecated(message = "Use 'replicas' instead")
    replicas: int @default(1)
}
```

`since` is optional and accepts a version string.

## @validate(...)

Attaches value constraints to an attribute or schema field. Multiple constraint arguments can be combined.

```wcl
schema "endpoint" {
    port:    int    @validate(min = 1, max = 65535)
    env:     string @validate(one_of = ["development", "staging", "production"])
    slug:    string @validate(pattern = "^[a-z0-9-]+$")
    timeout: int    @validate(min = 1, max = 300, custom_msg = "timeout must be between 1 and 300 seconds")
}
```

| Argument     | Applies to  | Description                                     |
|--------------|-------------|-------------------------------------------------|
| `min`        | int, float  | Minimum value (inclusive)                       |
| `max`        | int, float  | Maximum value (inclusive)                       |
| `pattern`    | string      | Regular expression the value must fully match   |
| `one_of`     | string, int | Value must be one of the given options          |
| `custom_msg` | any         | Custom message emitted on constraint violation  |

## @doc(text)

Attaches a documentation string to any declaration. Used by tooling and the language server to provide hover documentation.

```wcl
schema "service" {
    port: int    @required @doc("The TCP port this service listens on.")
    env:  string @default("production") @doc("Deployment environment name.")
}

service "api" @doc("Main API service for the frontend.") {
    port = 8080
}
```

## @example { }

Embeds a usage example directly inside a `decorator_schema` or `schema` declaration. Used by documentation generators and IDE tooling.

```wcl
decorator_schema "rate_limit" {
    target    = [attribute]
    requests:  int
    window_ms: int @default(1000)

    @example {
        calls_per_second = 100 @rate_limit(requests = 100, window_ms = 1000)
    }
}
```

## @allow(rule)

Suppresses a specific warning on a `let` binding or attribute. Use this when a warning is expected and intentional.

```wcl
let _unused = compute_value() @allow("unused_binding")

service "api" {
    legacy_flag = true @allow("deprecated_field")
}
```

The `rule` argument is a string identifying the warning to suppress.

## @id_pattern(glob)

Enforces a naming convention on block IDs for a schema. Any block whose ID does not match the glob pattern produces a validation error (E077).

```wcl
schema "service" @id_pattern("svc-*") {
    port: int
}

service "svc-api" { port = 8080 }    // valid
service "api"     { port = 8080 }    // error E077: ID does not match "svc-*"
```

## @ref(schema)

Applied to an identifier field in a schema. Requires that the field's value matches the ID of an existing block of the named type.

```wcl
schema "deployment" {
    service_id: string @ref("service")
}

service "api" { port = 8080 }

deployment "d1" {
    service_id = "api"      // valid: service "api" exists
}

deployment "d2" {
    service_id = "missing"  // error E076: no service "missing" found
}
```

## @partial_requires(fields)

Declares that a partial block expects certain fields to be present after merging. This documents and enforces merge dependencies.

```wcl
partial service @partial_requires(["port", "region"]) {
    env      = "production"
    replicas = 1
}
```

If a block that includes this partial does not provide the listed fields either directly or through another partial, a validation error is raised.

## @merge_order(n)

Sets an explicit integer priority for partial merge ordering. Partials with lower `n` are merged first. Without this decorator, merge order follows declaration order.

```wcl
partial service @merge_order(1) {
    env = "production"
}

partial service @merge_order(2) {
    env = "staging"    // this wins because it merges later
}
```

## @children(kinds)

Restricts which child blocks and tables may appear inside blocks of a given schema. The argument is a list of allowed block kind names and table identifiers.

```wcl
@children(["endpoint", "middleware", "table:user_row"])
schema "service" {
    name: string
}

service "api" {
    endpoint health { path = "/health" }     // allowed
    middleware auth { priority = 1 }          // allowed
    table users : user_row { | "Alice" | }   // allowed (table:user_row)
    // logger { level = "info" }             // ERROR E095: not in children list
}
```

Special names in the children list:

| Entry | Meaning |
|-------|---------|
| `"table"` | Allows anonymous tables (no schema ref) |
| `"table:X"` | Allows tables with `schema_ref = X` |

An empty list `@children([])` forbids all child blocks and tables, making the schema a leaf:

```wcl
@children([])
schema "leaf_node" {
    value: string
}
```

You can also constrain what appears at the document root by defining a schema named `"_root"`:

```wcl
@children(["service", "config"])
schema "_root" {}

service main { port = 8080 }     // allowed
database primary { host = "db" } // ERROR E095: not in _root children list
```

## @parent(kinds)

Restricts where a block may appear. The argument is a list of allowed parent block kinds. Use `"_root"` to allow the block at the document root.

```wcl
@parent(["service", "_root"])
schema "endpoint" {
    path: string
}

service "api" {
    endpoint health { path = "/health" }   // allowed: parent is "service"
}

endpoint standalone { path = "/ping" }     // allowed: parent is _root
```

If a block appears inside a parent not in its `@parent` list, error E096 is emitted.

### Constraining table placement

To constrain where tables may appear, define a virtual schema with the `"table"` or `"table:X"` name:

```wcl
# Tables with schema_ref "user_row" may only appear inside "data" blocks
@parent(["data"])
schema "table:user_row" {}

# Anonymous tables may only appear at the root
@parent(["_root"])
schema "table" {}
```

### Combined constraints

Both `@children` and `@parent` are checked independently. If both are violated on the same item, both E095 and E096 are emitted. If neither decorator is present on a schema, nesting is unrestricted (backwards compatible).
