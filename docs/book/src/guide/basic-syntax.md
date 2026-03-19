# Basic Syntax

WCL (Wil's Configuration Language) is a block-structured configuration language. Every WCL file is composed of two kinds of top-level declarations: **attributes** and **blocks**. Both may be nested to arbitrary depth inside block bodies.

## Attributes

An attribute binds a name to a value using `=`:

```wcl
name = "acme-app"
port = 8080
debug = false
```

The left-hand side must be a valid identifier (letters, digits, and underscores — no hyphens). The right-hand side is any expression. See [Attributes](./attributes.md) for the full set of value types.

## Blocks

A block groups related attributes and nested blocks under a *type* name:

```wcl
server {
  host = "0.0.0.0"
  port = 8080
}
```

Blocks may carry an inline ID (written with hyphens), string labels, or both:

```wcl
server #web-1 "production" {
  host = "0.0.0.0"
  port = 443
}
```

See [Blocks](./blocks.md) for the complete syntax including decorators, partials, and reserved block types.

## Nesting

Block bodies can contain other blocks, creating a tree:

```wcl
application {
  name    = "my-service"
  version = "1.0.0"

  database {
    host = "db.internal"
    port = 5432

    pool {
      min = 2
      max = 10
    }
  }

  server {
    host = "0.0.0.0"
    port = 8080
  }
}
```

There is no practical limit on nesting depth.

## A Complete Minimal File

```wcl
// Service configuration

service #api {
  name    = "api-gateway"
  version = "2.1.0"
  enabled = true

  listen {
    host = "0.0.0.0"
    port = 8080
  }
}
```

## Comments

WCL supports line comments (`// ...`), block comments (`/* ... */`), and doc comments (`/// ...`). See [Comments](./comments.md) for details.

## What's Next

- [Attributes](./attributes.md) — value types, expressions, and duplicate rules
- [Blocks](./blocks.md) — IDs, labels, decorators, partials, reserved types
- [Data Types](./data-types.md) — the full primitive and composite type system
- [Expressions](./expressions.md) — operators, precedence, function calls, lambdas
