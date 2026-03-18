# Comparison with Other Formats

This page compares WCL with JSON, YAML, TOML, and HCL across key features.

## Feature Matrix

| Feature | JSON | YAML | TOML | HCL | WCL |
|---------|------|------|------|-----|-----|
| **Comments** | No | Yes | Yes | Yes | Yes — line (`//`), block (`/* */`), doc (`///`); preserved by formatter |
| **String interpolation** | No | No | No | Yes | Yes — `"host: ${host}:${port}"` |
| **Type system** | Implicit (6 types) | Implicit (dynamic) | Explicit primitives | Limited | Full — primitives, composites, union, ref, any |
| **Schemas** | External (JSON Schema) | No | No | No | Built-in — first-class `schema` declarations with constraints |
| **Variables / let bindings** | No | Anchors only | No | Limited | Yes — `let name = expr` |
| **Expressions** | No | No | No | Limited (templates) | Yes — full arithmetic, logic, ternary, regex, lambdas |
| **Macros** | No | No | No | No | Yes — function macros and attribute macros |
| **Imports** | No | No | No | Handled by Terraform | Yes — `import "file.wcl"` built-in |
| **Query engine** | No (external: jq) | No | No | No | Yes — built-in `query(selector \| filters)` |
| **Tables** | Arrays of objects | Sequences | Array of tables | No | Yes — typed column declarations with row syntax |
| **Control flow** | No | No | No | `for_each` (Terraform) | Yes — `for` loops and `if`/`else` |
| **Partial declarations** | No | No | No | No | Yes — `partial` blocks merged across files |
| **Decorators** | No | No | No | No | Yes — `@decorator(args)` with schemas |
| **Validation blocks** | External | No | No | No | Built-in — `validation` blocks with `check` and `message` |
| **LSP support** | Via plugins | Via plugins | Via plugins | Via plugins | Yes — first-class `wcl lsp` with full feature set |
| **Formatting** | External (prettier) | External | External | External | Yes — `wcl fmt` built-in |
| **Bidirectional conversion** | — | Partial | Partial | No | Yes — `wcl convert` to/from JSON/YAML/TOML |

## Comments

**JSON** has no comment syntax at all. Tools like `jsonc` or `json5` add comments as extensions, but they are not part of the standard.

**YAML** supports `#` line comments.

**TOML** supports `#` line comments.

**HCL** supports `//`, `#`, and `/* */` comments.

**WCL** supports three comment forms: `//` line comments, `/* */` block comments (nestable), and `///` doc comments which are attached to the following declaration. All comment forms are preserved by `wcl fmt`.

## Schemas and Validation

**JSON Schema** is a separate specification applied externally. It is powerful but disconnected from the data file itself.

**YAML**, **TOML**, and **HCL** have no built-in schema mechanism.

**WCL** schemas are declared directly in WCL source and matched to blocks automatically by block type name. They support type constraints, required/optional fields, min/max, pattern matching, `one_of`, and cross-references between block types.

```wcl
schema "ServiceSchema" {
  @required
  name : string
  @min(1) @max(65535)
  port : int
  host : string
}

service svc-api {
  name = "api"
  port = 8080
  host = "localhost"
}
```

## Expressions and Variables

**JSON** and **TOML** are purely declarative — all values must be literals.

**YAML** supports anchors and aliases for value reuse, but no arithmetic or logic.

**HCL** (as used in Terraform) supports template expressions and some built-in functions.

**WCL** supports full expression evaluation: arithmetic, string interpolation, comparisons, logical operators, ternary expressions, list/map operations, regex matching, lambdas, and a built-in query engine.

## Partial Declarations

WCL's `partial` blocks are unique among configuration formats. A block can be declared in multiple files or multiple places in the same file with `partial`, and the pieces are merged before evaluation:

```wcl
// base.wcl
partial service svc-api {
  host = "localhost"
}

// overrides.wcl
import "base.wcl"

partial service svc-api {
  port = 9090
}

// Result: service svc-api { host = "localhost", port = 9090 }
```

This enables layered configuration composition without inheritance hierarchies or external merge tools.

## Decorators

WCL decorators attach metadata and behaviour to declarations. They are validated against `decorator_schema` definitions, making them typed and self-documenting:

```wcl
decorator_schema "tag" {
  target = [block, attribute]
  env    : string
  tier   : string
}

@tag(env = "prod", tier = "critical")
service svc-payments {
  port = 8443
}
```

No other mainstream configuration format has an equivalent mechanism.

## When to Use Each Format

| Format | Best suited for |
|--------|----------------|
| JSON | Machine-generated config, REST APIs, interoperability |
| YAML | Kubernetes manifests, CI pipelines, human-edited simple config |
| TOML | Application config files (`Cargo.toml`, `pyproject.toml`) |
| HCL | Terraform infrastructure definitions |
| WCL | Complex configuration with shared logic, schemas, cross-file composition, and tooling |
