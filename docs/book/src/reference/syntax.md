# Syntax Reference

This page is a concise formal summary of WCL syntax. The complete EBNF grammar is in the [EBNF Grammar](../appendix/ebnf.md) appendix.

## Document Structure

A WCL document is a sequence of top-level items. Order matters for readability but not for evaluation (declarations are dependency-ordered automatically).

```wcl
import "other.wcl"

let base = 8000

service svc-api {
  port = base + 80
}
```

Top-level items:

- `import` declarations
- `export` declarations
- Attributes (`name = expr`)
- Blocks (`type [id] [inline-args...] { body }`)
- Tables
- Let bindings (`let name = expr`)
- Macro definitions
- Macro calls
- For loops
- Conditionals
- Validation blocks
- Schemas and decorator schemas
- Symbol set declarations
- Comments

## Attributes

```wcl
port     = 8080
host     = "localhost"
enabled  = true
ratio    = 1.5
nothing  = null
tag      = #prod
```

An attribute is an identifier followed by `=` and an expression.

## Blocks

```wcl
service svc-api {
  port = 8080
}

service svc-api 8080 "production" {
  port = 9090
}
```

Syntax: `[decorators] [partial] type [id] [inline-args...] { body }`

- `type` — bare identifier (the block kind)
- `id` — identifier literal (may contain hyphens); used for unique identification
- `inline-args` — zero or more positional expressions (integer, float, string, bool, null, list) mapped to named fields via `@inline(N)` in a schema, or collected into `_args`

## Let Bindings

```wcl
let max_conns = 100
let dsn = "postgres://localhost/${db_name}"
```

Let bindings are module-scoped. They are not included in the serialized output.

## Imports

```wcl
import "base.wcl"
import "schemas/service.wcl"
```

Imports must appear at the top of the file. The imported document's contents are merged into the current document before evaluation.

## Control Flow

### For Loops

```wcl
let ports = [8080, 8081, 8082]

for i, port in ports {
  service "svc-${i}" {
    port = port
  }
}
```

### Conditionals

```wcl
if env == "prod" {
  replicas = 3
} else if env == "staging" {
  replicas = 2
} else {
  replicas = 1
}
```

## Tables

```wcl
table routes {
  path:   string
  method: string
  handler: string

  | "/health" | "GET"  | "health_handler" |
  | "/users"  | "GET"  | "list_users"     |
  | "/users"  | "POST" | "create_user"    |
}
```

Tables declare typed columns followed by row data using `|`-delimited syntax.

## Schemas

```wcl
schema "ServiceSchema" {
  port    : i64
  host    : string
  @required
  name    : string
  @min(1) @max(65535)
  port    : i64
}
```

Schemas define the expected shape and constraints for blocks.

## Decorator Schemas

```wcl
decorator_schema "tag" {
  target = [block, attribute]
  env    : string
  tier   : string
}
```

Decorator schemas define the structure of custom decorators.

## Decorators

```wcl
@deprecated
@tag(env = "prod", tier = "critical")
@partial_requires(["port", "host"])
service svc-api {
  port = 8080
}
```

Decorators appear immediately before the item they annotate. They accept positional and named arguments.

## Macros

### Function Macro Definition

```wcl
macro service_defaults(port: i64, host: string = "localhost") {
  port    = port
  host    = host
  enabled = true
}
```

### Attribute Macro Definition

```wcl
macro @with_logging(level: string = "info") {
  inject {
    log_level = level
  }
}
```

### Macro Call

```wcl
service svc-api {
  service_defaults(8080)
}
```

## Expressions

| Category | Examples |
|----------|---------|
| Literals | `42`, `3.14`, `"hello"`, `true`, `false`, `null`, `#my-id` |
| Arithmetic | `a + b`, `a - b`, `a * b`, `a / b`, `a % b` |
| Comparison | `a == b`, `a != b`, `a < b`, `a >= b`, `a =~ "pattern"` |
| Logical | `a && b`, `a \|\| b`, `!a` |
| Ternary | `cond ? then_val : else_val` |
| String interpolation | `"host: ${host}:${port}"` |
| List | `[1, 2, 3]` |
| Map | `{ key: "value" }` |
| Field access | `obj.field` |
| Index | `list[0]`, `map["key"]` |
| Query | `query(service \| .port > 1024)` |
| Ref | `ref(svc-api)` |
| Lambda | `x => x * 2`, `(a, b) => a + b` |

## Comments

```wcl
// Line comment

/*
  Block comment
  (nestable)
*/

/// Doc comment — attached to the next item
service svc-api {
  port = 8080
}
```
