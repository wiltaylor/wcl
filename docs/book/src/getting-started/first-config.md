# Your First Configuration File

This page walks through creating a WCL file, evaluating it, adding a schema, and validating it.

## Create a File

Create a file called `hello.wcl` with a simple server block:

```wcl
server web {
    host = "localhost"
    port = 3000
    workers = 2
}
```

A block in WCL has a type (`server`), an optional ID (`web`), and a body containing attribute assignments.

## Evaluate It

The `eval` command runs the WCL pipeline and outputs the result as JSON:

```bash
wcl eval hello.wcl
```

Output:

```json
{
  "server": {
    "web": {
      "host": "localhost",
      "port": 3000,
      "workers": 2
    }
  }
}
```

WCL evaluates expressions, resolves references, and expands macros before producing output. For a simple file like this, the output mirrors the input structure.

## Add a Schema

Extend `hello.wcl` to include a schema:

```wcl
server web {
    host = "localhost"
    port = 3000
    workers = 2
}

schema "server" {
    host    : string
    port    : int
    workers : int
}
```

The `schema` block declares the expected type for each attribute. WCL matches schemas to blocks automatically by block type name — a schema named `"server"` validates all `server` blocks.

## Validate It

```bash
wcl validate hello.wcl
```

If the configuration is valid, the command exits with code 0 and no output. If there is a type mismatch or a missing required field, you will see a diagnostic:

```
error[E071]: type mismatch for field `port`: expected int, got string
  --> hello.wcl:3:12
   |
 3 |     port = "3000"
   |            ^^^^^^ expected int
```

## JSON Output with a Schema

Validation does not change the JSON output — `wcl eval hello.wcl` produces the same JSON structure as before. The schema is purely a validation constraint.

## Key Concepts

- **Blocks**: the primary unit of configuration. A block has a type, an optional ID, and a set of attributes.
- **Attributes**: key-value pairs inside a block. Values can be literals, expressions, function calls, or references to other values.
- **Schemas**: declare the expected type (and optionally constraints) for each attribute in a block. Schemas are matched to blocks automatically by block type name.
- **Decorators**: `@name` or `@name(args)` annotations on blocks or attributes. Built-in decorators include `@sensitive`, `@optional`, `@partial`, and more.

From here, explore the [CLI Quickstart](./cli-quickstart.md) to learn about the other commands available, or jump into the language reference to learn about expressions, macros, and the query engine.
