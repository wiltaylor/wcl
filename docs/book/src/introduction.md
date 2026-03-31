# Introduction

WCL (Wil's Configuration Language) is a statically-typed, block-structured configuration language designed for expressive, validated, and maintainable configuration. It combines schemas, decorators, macros, data tables, a query engine, partial declarations, and a full Language Server Protocol (LSP) implementation into a single cohesive system.

## What is WCL?

WCL is a configuration language built around named blocks of typed key-value attributes. Unlike general-purpose data formats, WCL is evaluated: it supports expressions, environment variable references, built-in functions, and cross-block references. Schemas enforce the shape of your data at validation time, and the entire pipeline from parsing to schema validation is designed to produce clear, actionable diagnostics.

Core features:

- **Block-structured**: configuration is organized into named, typed blocks
- **Static typing with schemas**: declare the expected shape of any block type and validate against it
- **Decorators**: attach metadata or behavior to blocks and attributes with `@decorator` syntax
- **Macros**: define reusable configuration fragments with function-style and attribute-style macros
- **Data tables**: declare tabular data inline and query it
- **Partial declarations**: split a block definition across multiple files or sites with `@partial`
- **Query engine**: select and filter blocks using a path-based query syntax
- **Full LSP**: diagnostics, hover, go-to-definition, completions, formatting, and more

## Why WCL Instead of JSON, YAML, TOML, or HCL?

| Feature | JSON | YAML | TOML | HCL | WCL |
|---|---|---|---|---|---|
| Comments | No | Yes | Yes | Yes | Yes (doc comments too) |
| Expressions | No | No | No | Yes | Yes |
| Schemas / validation | No | No | No | Partial | Yes (first-class) |
| Macros / reuse | No | Anchors | No | Modules | Yes |
| Decorators | No | No | No | No | Yes |
| Data tables | No | No | No | No | Yes |
| Query engine | No | No | No | No | Yes |
| LSP | No | Limited | Limited | Yes | Yes |
| Static types | No | No | No | No | Yes |

JSON and TOML are simple but offer no reuse or validation. YAML is expressive but notorious for surprising parse behavior. HCL is powerful but tightly coupled to HashiCorp tooling. WCL is designed as a standalone, tool-agnostic configuration layer with a full validation and evaluation pipeline.

## Quick Example

The following defines a production web server configuration with an enforcing schema:

```wcl
/// Production web server configuration
server web-prod {
    host = "0.0.0.0"
    port = 8080
    workers = max(4, 2)

    @sensitive
    api_key = "sk-secret-key"
}

schema "server" {
    host: string @optional
    port: i64
    workers: i64
    api_key: string
}
```

Key things to notice:

- `server web-prod { ... }` is a block of type `server` with ID `web-prod`
- The `schema "server"` block automatically validates every `server` block by matching the name
- `workers = max(4, 2)` is an evaluated expression using a built-in function
- `@sensitive` is a decorator that can be handled by tooling (e.g., to redact the value from output)
- Schema fields use colon syntax (`port: i64`) to declare expected types

## Where to Go Next

Ready to start using WCL? Head to [Getting Started](./getting-started/installation.md) to install the CLI and write your first configuration file.
