# Blocks

A block is the primary structural unit of a WCL file. Blocks group attributes and nested blocks under a named type, and can carry optional identity and metadata.

## Full Syntax

```
[decorators] [partial] type [inline-id] [inline-args...] { body }
```

Every component except `type` and `{ body }` is optional.

## Components

### Block Type

The block type is a plain identifier that names the kind of configuration node. Types are user-defined unless they are one of the reserved names listed below.

```wcl
server { }
database { }
endpoint { }
my_custom_type { }
```

### Inline ID

An inline ID uniquely identifies a block within its scope. It may contain letters, digits, and **hyphens** (unlike attribute names, which forbid hyphens):

```wcl
server web-1 { }
database primary-db { }
endpoint get-users { }
```

Inline IDs must be unique per block type within their scope. The only exception is `partial` blocks, which share an ID with the block they extend (see [Partial Declarations](./partials.md)).

### Inline Arguments

Inline arguments are positional values placed between the inline ID (or block type) and the opening `{`. They accept any primary expression: integers, floats, strings, booleans, `null`, and lists.

```wcl
server web 8080 "prod" { }
endpoint api true 3 { }
service worker [1, 2, 3] { }
```

Without a schema, inline args (including the block's inline identifier) are collected into a synthetic `_args` list attribute:

```wcl
server web 8080 "prod" {
  host = "localhost"
}
// Evaluates to: { _args: [web, 8080, "prod"], host: "localhost" }
```

With a schema, fields decorated with `@inline(N)` map positional args to named attributes. The inline identifier occupies index 0, and additional arguments follow:

```wcl
schema "server" {
    id: identifier @inline(0)
    port: int @inline(1)
    env: string @inline(2)
    host: string
}

server web 8080 "prod" {
    host = "localhost"
}
// Evaluates to: { id: web, port: 8080, env: "prod", host: "localhost" }
```

Any args not mapped by `@inline` remain in `_args`.

### Decorators

Decorators are annotations that modify or validate a block. They are placed before the block type, one per line, each starting with `@`:

```wcl
@deprecated
@env("production")
server legacy { }

@required
server primary { }
```

See [Decorators](./decorators.md) for the full decorator system.

### The `partial` Keyword

The `partial` keyword marks a block as a partial declaration, meaning it will be merged into another block with the same type and ID. This allows spreading a block's definition across multiple files:

```wcl
partial server web-1 {
  host = "0.0.0.0"
}

partial server web-1 {
  port = 8080
}
```

See [Partial Declarations](./partials.md) for merge semantics.

### Body

The body is a `{ }`-delimited sequence of attributes, nested blocks, `let` bindings, `for` loops, and `if`/`else` expressions.

## Reserved Block Types

The following block types have special semantics and are handled by the WCL pipeline:

| Type               | Purpose                                               |
|--------------------|-------------------------------------------------------|
| `schema`           | Defines a schema for validating user blocks           |
| `decorator_schema` | Defines the parameter schema for a decorator          |
| `table`            | Tabular data with typed columns                       |
| `validation`       | Inline validation assertions                          |
| `macro`            | Defines a reusable macro (function or attribute form) |

## Examples

### Minimal Block

```wcl
server { }
```

### Block with Attributes

```wcl
server {
  host = "0.0.0.0"
  port = 8080
}
```

### Block with Inline ID

```wcl
server web-1 {
  host = "0.0.0.0"
  port = 8080
}
```

### Block with Inline Arguments

```wcl
server 8080 "production" {
  host = "prod.example.com"
}
```

### Block with Inline ID and Arguments

```wcl
server web-1 8080 "production" {
  host = "prod.example.com"
}
```

### Decorated Block

```wcl
@env("production")
server primary {
  host = "prod.example.com"
  port = 443
}
```

### Nested Blocks

```wcl
application my-app {
  name    = "my-app"
  version = "1.0.0"

  server {
    host = "0.0.0.0"
    port = 8080

    tls {
      cert = "/etc/certs/server.crt"
      key  = "/etc/certs/server.key"
    }
  }

  database primary {
    host = "db.internal"
    port = 5432
  }
}
```

### Multiple Sibling Blocks of the Same Type

```wcl
server web-1 {
  host = "10.0.0.1"
  port = 8080
}

server web-2 {
  host = "10.0.0.2"
  port = 8080
}

server web-3 {
  host = "10.0.0.3"
  port = 8080
}
```

Blocks without IDs in the same scope are not required to be unique by the evaluator, but schemas may impose additional uniqueness constraints.
