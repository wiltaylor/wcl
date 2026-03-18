# WCL -- Wil's Configuration Language

A statically-typed, block-structured configuration language with first-class support for composition, validation, and tooling. WCL draws syntactic inspiration from HCL and extends it with schemas, decorators, macros, data tables, a query engine, partial declarations, and a full Language Server Protocol implementation.

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
    port: int
    workers: int
    api_key: string
}

let regions = ["us-east", "eu-west", "ap-south"]
for region in regions {
    server worker-${region} {
        host = "${region}.internal"
        port = 9000
    }
}
```

## Features

- **Block-structured syntax** -- Human-readable configuration with named, typed blocks and inline IDs
- **Static type system** -- `string`, `int`, `float`, `bool`, `null`, `identifier`, `list`, `map`, `set`, `ref`, `union`
- **Schemas and validation** -- Declare field types, constraints (`@validate(min, max, pattern, one_of)`), and cross-references (`@ref`)
- **Decorators** -- `@deprecated`, `@sensitive`, `@optional`, `@validate`, `@doc`, and user-defined decorator schemas
- **Macros** -- Function macros for reusable block templates, attribute macros for block transformations
- **Data tables** -- Typed tabular data with column declarations and row validation
- **Query engine** -- `service | .port > 8000 | has(@deprecated)` pipeline syntax for filtering and projecting
- **Partial declarations** -- Split block definitions across files and merge them
- **Import system** -- `import "./shared.wcl"` with jail-checked path resolution
- **Control flow** -- `for`/`if`/`else` for generating dynamic configuration
- **50+ built-in functions** -- String, math, collection, crypto, type conversion, and higher-order functions
- **Serde integration** -- `#[derive(WclDeserialize)]` for direct Rust struct deserialization
- **Full LSP server** -- Hover, go-to-definition, completions, semantic tokens, references, formatting, and more

## Architecture

WCL is implemented as a Rust workspace with 8 crates (~25,000 lines, 1,000+ tests):

| Crate | Purpose |
|-------|---------|
| `wcl_core` | Lexer (nom), parser, AST, spans, trivia, diagnostics |
| `wcl_eval` | Evaluator, scope arena, macros, imports, control flow, query engine |
| `wcl_schema` | Schema registry, type checking, constraints, decorator validation |
| `wcl_serde` | `Value` <-> Serde bridge for JSON/YAML/TOML interop |
| `wcl_derive` | `#[derive(WclDeserialize)]` proc macro |
| `wcl_lsp` | Language Server Protocol implementation (tower-lsp) |
| `wcl_cli` | `wcl` command-line tool |
| `wcl` | Facade crate re-exporting all public APIs |

### Evaluation Pipeline

```
Source text
  |
  v
1. Lex .............. nom tokenizer with error recovery
2. Parse ............ recursive descent -> AST with spans + trivia
3. Macro collection . register function + attribute macros
4. Import resolution  jail-checked file loading + AST merging
5. Macro expansion .. splice function macros, transform attribute macros
6. Control flow ..... expand for/if into concrete blocks
7. Partial merge .... combine partial declarations (strict/override)
8. Evaluation ....... scope construction, topological sort, expression eval
9. Decorator valid. . check decorator args against decorator schemas
10. Schema valid. ... type check, constraints, @ref resolution, @id_pattern
11. Document valid. . run `validation` block check/message rules
```

## Getting Started

### Install

```bash
cargo install --path wcl_cli
```

This installs the `wcl` binary to `~/.cargo/bin/`.

### Uninstall

```bash
cargo uninstall wcl_cli
```

### Build (development)

```bash
cargo build --workspace
```

### Run the CLI

```bash
# Evaluate a WCL file to JSON
wcl eval config.wcl

# Validate with optional external schema
wcl validate config.wcl --schema types.wcl

# Format in place
wcl fmt config.wcl --write

# Query blocks
wcl query config.wcl "server | .port > 8000"

# Convert formats
wcl convert data.json --to wcl
wcl convert config.wcl --to yaml

# Set/add/remove values programmatically
wcl set config.wcl "server#web.port" "9090"
wcl add config.wcl "server new-svc"
wcl remove config.wcl "server#old-svc"

# Start the language server
wcl lsp                          # stdio (default)
wcl lsp --tcp 127.0.0.1:9257    # TCP
```

### Use as a Library

```rust
use wcl::{parse, ParseOptions, Value};

let doc = parse(r#"
    server web {
        port = 8080
        host = "localhost"
    }
"#, ParseOptions::default());

assert!(!doc.has_errors());

// Access evaluated values
if let Some(Value::Map(server)) = doc.values.get("server") {
    println!("port = {:?}", server.get("port"));
}

// Run queries
let result = doc.query("server | .port").unwrap();
```

### Derive Macro

```rust
use wcl::{from_str, WclDeserialize};

#[derive(WclDeserialize)]
struct ServerConfig {
    #[wcl(id)]
    name: Option<String>,
    port: i64,
    host: String,
}

let config: ServerConfig = from_str(r#"
    port = 8080
    host = "localhost"
"#).unwrap();
```

## Language Server

The `wcl lsp` command starts a full-featured LSP server supporting:

| Feature | Description |
|---------|-------------|
| **Diagnostics** | Real-time errors and warnings from all 11 pipeline phases |
| **Hover** | Type info, evaluated values, doc comments, decorator args, macro signatures |
| **Go to Definition** | Jump to let bindings, blocks, macros, and imported files |
| **Document Symbols** | Outline view with blocks, attributes, let bindings, schemas, macros |
| **Completion** | Context-aware: keywords, variables, builtins, decorators, types, member access |
| **Semantic Tokens** | AST-aware highlighting: keywords, types, properties, functions, decorators |
| **Signature Help** | Parameter info for 50+ builtins and user-defined macros |
| **Find References** | Scope-aware reference finding with block-kind search |
| **Formatting** | Full document formatting via AST round-trip |

### Editor Setup

**Neovim:**
```lua
vim.lsp.start({
    name = "wcl",
    cmd = { "wcl", "lsp" },
    root_dir = vim.fs.dirname(vim.fs.find({ ".git" }, { upward = true })[1]),
    filetypes = { "wcl" },
})
```

**VS Code:**

A bundled extension lives in `editors/vscode/`. Install it with:

```bash
just install-vscode
```

Or manually:

```bash
cd editors/vscode && npm install
ln -sfn "$(pwd)" ~/.vscode/extensions/wil.wcl-0.1.0
```

Then restart VS Code.

This registers `.wcl` files, provides TextMate syntax highlighting, and starts the LSP server automatically. Requires `wcl` to be on your `$PATH` (`just install`).

**Helix** (`languages.toml`):
```toml
[[language]]
name = "wcl"
scope = "source.wcl"
file-types = ["wcl"]
language-servers = ["wcl-lsp"]

[language-server.wcl-lsp]
command = "wcl"
args = ["lsp"]
```

## Testing

```bash
# Run all tests
cargo test --workspace

# Run LSP tests only (84 tests)
cargo test -p wcl_lsp

# Lint
cargo clippy --workspace
```

## Language Quick Reference

```wcl
// Attributes
name = "value"
port = 8080
enabled = true
tags = ["web", "prod"]
metadata = { version = "1.0", owner = "team-a" }

// Blocks with inline IDs and labels
server web-prod "us-east" {
    port = 8080
}

// Let bindings and expressions
let base_port = 8000
let debug = true
let greeting = "Hello, ${upper(name)}!"

// Control flow
for i, svc in ["auth", "api", "web"] {
    service svc-${svc} {
        port = base_port + i
    }
}

if debug {
    log_level = "debug"
} else {
    log_level = "info"
}

// Schemas (matched to blocks by name automatically)
schema "service" {
    port: int @validate(min = 1024, max = 65535)
    host: string @validate(pattern = "^[a-z.-]+$")
    env: string @validate(one_of = ["dev", "staging", "prod"])
    db: ref("database")
}

// Decorators
@deprecated("use server_v2 instead")
service legacy {
    port = 8080
    host = "localhost"
    env = "prod"
}

// Macros
macro with_monitoring(port_offset = 100) {
    monitoring {
        port = port_offset
        enabled = true
    }
}

// Data tables
table users {
    name: string
    role: string
    | "alice" | "admin" |
    | "bob"   | "dev"   |
}

// Queries
let admins = query(table."users" | .role == "admin")

// Partial declarations (across files)
partial server web-prod {
    tls = true
}

// Validation blocks
validation "port range check" {
    let ports = query(server | .port)
    check = every(ports, p => p >= 1024 && p <= 65535)
    message = "All server ports must be in range 1024-65535"
}

// Imports
import "./shared/base.wcl"
```

## File Extension

`.wcl` -- MIME type: `text/x-wcl`

## License

See LICENSE file for details.
