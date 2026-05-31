# WCL Language Reference

WCL is a typed configuration language built around two ideas: **fields** (`name = value`) and **blocks** (`kind "label" { … }`). On top of that base sit a real type system, first-class functions, pattern matching, and a schema layer that validates a document's structure.

> [!NOTE]
> **How this book is organised**
> Three sections. Language walks every feature of the language, one feature per chapter. Reference is the builtin function index and CLI reference. wdoc covers the documentation generator's blocks — this site is built with it.

## Sections

| Section | Covers |
| --- | --- |
| **Language** | Every language feature — literals, fields, types, unions, functions, pattern matching, schema, imports |
| **Reference** | Builtin function index and full CLI reference |
| **wdoc** | The documentation generator: sites, pages, callouts, code, diagrams, charts, terminals, timelines, icons |

## The mental model

A document is a tree of fields and blocks. A field binds a name to a value; a block is a named, labelled group of fields and nested blocks. Here is a small document:

```wcl
name    = "alpha"
count   = 3
enabled = true

service "web" {
  port = 8080
  metadata {
    region = "us-east-1"
  }
}
```

Values carry types (`count` is an `i64`, `enabled` a `bool`). Blocks are validated against a schema declared with decorators. Beyond plain data, WCL has `let` bindings, functions, `if`/`match`, and modules via `import`.
