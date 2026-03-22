# Using WCL as a Rust Library

WCL can be embedded directly into Rust programs. The `wcl` facade crate provides the full parsing pipeline, evaluated values, queries, and serde deserialization.

## Adding the Dependency

Add `wcl` as a path dependency (or git dependency) in your `Cargo.toml`:

```toml
[dependencies]
wcl = { path = "../wcl" }
```

The `wcl` crate re-exports everything you need.

## Parsing a WCL String

Use `wcl::parse()` to run the full 11-phase pipeline and get a `Document`:

```rust
use wcl::{parse, ParseOptions};

let source = r#"
    server web-prod {
        host = "0.0.0.0"
        port = 8080
        debug = false
    }
"#;

let doc = parse(source, ParseOptions::default());

// Check for errors
if doc.has_errors() {
    for diag in doc.errors() {
        eprintln!("error: {}", diag.message);
    }
} else {
    println!("Document parsed successfully");
}
```

## Parsing a WCL File

Read the file yourself and pass the content to `parse()`. Set `root_dir` so that imports resolve correctly:

```rust
use wcl::{parse, ParseOptions};
use std::path::PathBuf;

let path = PathBuf::from("config/main.wcl");
let source = std::fs::read_to_string(&path).expect("read file");

let options = ParseOptions {
    root_dir: path.parent().unwrap().to_path_buf(),
    ..Default::default()
};

let doc = parse(&source, options);
assert!(!doc.has_errors());
```

## Accessing Evaluated Values

After parsing, `doc.values` contains an ordered map of all evaluated top-level attributes and blocks:

```rust
use wcl::{parse, ParseOptions, Value};

let doc = parse(r#"
    name = "my-app"
    port = 8080
    tags = ["web", "prod"]
"#, ParseOptions::default());

// Access scalar values
if let Some(Value::String(name)) = doc.values.get("name") {
    println!("name: {}", name);
}

if let Some(Value::Int(port)) = doc.values.get("port") {
    println!("port: {}", port);
}

// Access list values
if let Some(Value::List(tags)) = doc.values.get("tags") {
    for tag in tags {
        println!("tag: {}", tag);
    }
}
```

## Working with Blocks

Blocks are stored as `Value::BlockRef` in the values map. You can also use the convenience methods on `Document`:

```rust
use wcl::{parse, ParseOptions, Value};

let doc = parse(r#"
    server web-prod {
        host = "0.0.0.0"
        port = 8080
    }

    server web-staging {
        host = "staging.internal"
        port = 8081
    }
"#, ParseOptions::default());

// Get all blocks as resolved BlockRef values
let servers = doc.blocks_of_type_resolved("server");
for server in &servers {
    println!("server id={:?}", server.id);
    if let Some(Value::Int(port)) = server.get("port") {
        println!("  port: {}", port);
    }
    if let Some(Value::String(host)) = server.get("host") {
        println!("  host: {}", host);
    }
}

// Check decorators
let all_blocks = doc.blocks();
for block in &all_blocks {
    if block.has_decorator("deprecated") {
        let dec = block.decorator("deprecated").unwrap();
        println!("{} is deprecated: {:?}", block.kind, dec.args);
    }
}
```

## Working with Tables

Tables evaluate to `Value::List(Vec<Value::Map>)` — a list of row maps where each row maps column names to cell values:

```rust
use wcl::{parse, ParseOptions};
use wcl::eval::value::Value;

let doc = parse(r#"
    table users {
        name : string
        age  : int
        | "alice" | 25 |
        | "bob"   | 30 |
    }
"#, ParseOptions::default());

if let Some(Value::List(rows)) = doc.values.get("users") {
    for row in rows {
        if let Value::Map(cols) = row {
            println!("{}: {}", cols["name"], cols["age"]);
        }
    }
}
// Output:
// alice: 25
// bob: 30
```

Tables inside blocks appear in the `BlockRef.attributes` map:

```rust
if let Some(Value::BlockRef(br)) = doc.values.get("main") {
    if let Some(Value::List(rows)) = br.attributes.get("users") {
        println!("Table has {} rows", rows.len());
    }
}
```

## Running Queries

The `Document::query()` method accepts the same query syntax as the CLI:

```rust
use wcl::{parse, ParseOptions, Value};

let doc = parse(r#"
    server svc-api {
        port = 8080
        env = "prod"
    }

    server svc-admin {
        port = 9090
        env = "prod"
    }

    server svc-debug {
        port = 3000
        env = "dev"
    }
"#, ParseOptions::default());

// Select all server blocks
let all = doc.query("server").unwrap();

// Filter by attribute
let prod = doc.query(r#"server | .env == "prod""#).unwrap();

// Project a single attribute
let ports = doc.query("server | .port").unwrap();
if let Value::List(port_list) = ports {
    println!("ports: {:?}", port_list);
    // [Int(8080), Int(9090), Int(3000)]
}

// Filter and project
let prod_ports = doc.query(r#"server | .env == "prod" | .port"#).unwrap();

// Select by ID
let api = doc.query("server#svc-api").unwrap();
```

## Deserializing into Rust Structs

### With `from_str`

The simplest approach deserializes a WCL string directly into a Rust type via serde:

```rust
use wcl::from_str;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct AppConfig {
    name: String,
    port: i64,
    debug: bool,
}

let config: AppConfig = from_str(r#"
    name = "my-app"
    port = 8080
    debug = false
"#).expect("parse error");

println!("{:?}", config);
```

### Deserializing from `Value`

If you already have a parsed `Document`, you can deserialize individual values using `from_value`:

```rust
use wcl::{from_value, Value};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct ServerConfig {
    port: i64,
    host: String,
}

// Assume `val` is a Value::Map obtained from doc.values or a BlockRef
let mut map = indexmap::IndexMap::new();
map.insert("port".to_string(), Value::Int(8080));
map.insert("host".to_string(), Value::String("localhost".to_string()));

let config: ServerConfig = from_value(Value::Map(map)).unwrap();
println!("{:?}", config);
```

## Custom Functions

You can register custom Rust functions that are callable from WCL expressions. This lets host applications extend WCL with domain-specific logic:

```rust
use wcl::{parse, ParseOptions, FunctionRegistry, FunctionSignature, Value};
use std::sync::Arc;

let mut opts = ParseOptions::default();

// Register a custom function
opts.functions.functions.insert(
    "double".into(),
    Arc::new(|args: &[Value]| {
        match args.first() {
            Some(Value::Int(n)) => Ok(Value::Int(n * 2)),
            _ => Err("expected int".into()),
        }
    }),
);

// Optionally add a signature for LSP support (completions, signature help)
opts.functions.signatures.push(FunctionSignature {
    name: "double".into(),
    params: vec!["n: int".into()],
    return_type: "int".into(),
    doc: "Double a number".into(),
});

let doc = parse("result = double(21)", opts);
assert_eq!(doc.values.get("result"), Some(&Value::Int(42)));
```

You can also use `FunctionRegistry::register()` to add both the function and its signature at once:

```rust
use wcl::{FunctionRegistry, FunctionSignature, Value};
use std::sync::Arc;

let mut registry = FunctionRegistry::new();
registry.register(
    "greet",
    Arc::new(|args: &[Value]| {
        match args.first() {
            Some(Value::String(s)) => Ok(Value::String(format!("Hello, {}!", s))),
            _ => Err("expected string".into()),
        }
    }),
    FunctionSignature {
        name: "greet".into(),
        params: vec!["name: string".into()],
        return_type: "string".into(),
        doc: "Greet someone".into(),
    },
);
```

## Library Files

Create `.wcl` library files manually and place them in `~/.local/share/wcl/lib/`. Use `wcl::library::list_libraries()` to list installed libraries. See the [Libraries guide](../guide/libraries.md) for details.

## Parse Options

`ParseOptions` controls the pipeline behavior:

```rust
use wcl::{ParseOptions, ConflictMode};
use std::path::PathBuf;

let options = ParseOptions {
    // Root directory for import path jail checking
    root_dir: PathBuf::from("./config"),

    // Maximum depth for nested imports (default: 32)
    max_import_depth: 32,

    // Set to false for untrusted input to forbid all imports
    allow_imports: true,

    // How to handle duplicate attributes in partial merges
    // Strict = error on duplicates, LastWins = later value wins
    merge_conflict_mode: ConflictMode::Strict,

    // Maximum macro expansion depth (default: 64)
    max_macro_depth: 64,

    // Maximum for-loop nesting depth (default: 32)
    max_loop_depth: 32,

    // Maximum total iterations across all for loops (default: 10,000)
    max_iterations: 10_000,

    // Custom functions (builtins are always included)
    functions: FunctionRegistry::default(),
};
```

When processing untrusted WCL input (e.g., from an API), disable imports to prevent file system access:

```rust
use wcl::ParseOptions;

let options = ParseOptions {
    allow_imports: false,
    ..Default::default()
};
```

## Error Handling

The `Document` collects all diagnostics from every pipeline phase. Each `Diagnostic` includes a message, severity, source span, and optional error code:

```rust
use wcl::{parse, ParseOptions};

let doc = parse(r#"
    server web {
        port = "not_a_number"
    }

    schema "server" {
        port: int
    }
"#, ParseOptions::default());

for diag in &doc.diagnostics {
    let severity = if diag.is_error() { "ERROR" } else { "WARN" };
    let code = diag.code.as_deref().unwrap_or("----");
    eprintln!("[{}] {}: {}", severity, code, diag.message);
}
```

## Complete Example

Putting it all together -- parse a configuration file, validate it, query it, and extract values:

```rust
use wcl::{parse, ParseOptions, Value};
use std::path::PathBuf;

fn main() {
    let source = r#"
        schema "server" {
            port: int
            host: string @optional
        }

        server svc-api {
            port = 8080
            host = "api.internal"
        }

        server svc-admin {
            port = 9090
            host = "admin.internal"
        }
    "#;

    let doc = parse(source, ParseOptions::default());

    // 1. Check for errors
    if doc.has_errors() {
        for e in doc.errors() {
            eprintln!("{}", e.message);
        }
        std::process::exit(1);
    }

    // 2. Query for all server ports
    let ports = doc.query("server | .port").unwrap();
    println!("All ports: {}", ports);

    // 3. Iterate resolved blocks
    for server in doc.blocks_of_type_resolved("server") {
        let id = server.id.as_deref().unwrap_or("(no id)");
        let port = server.get("port").unwrap();
        let host = server.get("host").unwrap();
        println!("{}: {}:{}", id, host, port);
    }
}
```
