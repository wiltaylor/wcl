# Rust Binding (`wcl_lang`)

Native Rust crate — no WASM, full fidelity. Source: `crates/wcl_lang/src/lib.rs`.

## Install

```toml
[dependencies]
wcl_lang = "0.0.0-local"  # workspace version
```

## Minimal Example

```rust
use wcl_lang::{parse, ParseOptions};

fn main() {
    let src = r#"
        server web {
            host = "0.0.0.0"
            port = 8080
        }
    "#;
    let doc = parse(src, ParseOptions::default());
    if doc.has_errors() {
        for err in doc.errors() {
            eprintln!("{}: {}", err.code.as_deref().unwrap_or("?"), err.message);
        }
        return;
    }
    for block in doc.blocks() {
        println!("{} {:?}", block.kind, block.id);
    }
}
```

## Core API

| Item | Purpose |
|------|---------|
| `parse(source: &str, options: ParseOptions) -> Document` | Entry point. `crates/wcl_lang/src/lib.rs:591` |
| `ParseOptions` | Fields: `root_dir`, `allow_imports`, `max_import_depth` (default 32), `max_macro_depth` (64), `max_loop_depth`, `max_iterations`, `variables`, `functions` (custom fns), `fs` (`Arc<dyn FileSystem>`), `lib_paths`, `no_default_lib_paths`. `lib.rs:49` |
| `Document::has_errors() -> bool` | `lib.rs:464` |
| `Document::errors() -> Vec<&Diagnostic>` | `lib.rs:469` |
| `Document::diagnostics` field | All diagnostics (errors, warnings) |
| `Document::blocks() -> Vec<BlockRef>` | Top-level resolved blocks. `lib.rs:444` |
| `Document::blocks_of_type_resolved(kind: &str)` | `lib.rs:449` |
| `Document::blocks_of_type(kind: &str) -> Vec<&ast::Block>` | Unresolved AST form. `lib.rs:136` |
| `Document::eval_expression(src: &str) -> Result<Value, String>` | `lib.rs:154` |
| `Document::query(query_str: &str) -> Result<Value, String>` | `lib.rs:192` |
| `Document::call_function(name: &str, args: &[Value]) -> Result<Value, String>` | Exported fn invocation. `lib.rs:491` |
| `Document::exported_function_names() -> Vec<&str>` | `lib.rs:474` |
| `Document::has_decorator(decorator_name: &str) -> bool` | `lib.rs:457` |

### Serde Integration

```rust
use wcl_lang::{from_str, to_string_pretty};

#[derive(serde::Deserialize)]
struct Server { host: String, port: u16 }

let srv: Server = from_str(r#"host = "0.0.0.0"; port = 8080"#)?;
let back = to_string_pretty(&srv)?;
```

Exports: `from_str`, `from_str_with_options`, `to_string`, `to_string_pretty`, `to_string_compact`. `lib.rs:1033–1070`.

### Derive Macros (`wcl_derive`)

- `WclDeserialize` — derive Serde-compatible deserialization.
- `WclSchema` — generate a WCL schema declaration from a Rust struct.

Field attribute: `#[wcl(args)]` marks the inline-args tuple (replaces the deprecated `labels`).

## Value Model

`wcl_lang::Value` enum (via re-export):
`String`, `Int(i64)`, `Float(f64)`, `Bool`, `Null`, `List(Vec<Value>)`, `Map(IndexMap<String, Value>)`, `Block(BlockRef)`, `Date`, `Duration`, `Symbol`.

`BlockRef`: `kind`, `id`, `qualified_id`, `attributes`, `children`, `decorators`.

## Custom Functions

```rust
use std::sync::Arc;
use wcl_lang::{parse, ParseOptions, Value};

let mut opts = ParseOptions::default();
opts.functions.insert(
    "upper_rev".to_string(),
    Arc::new(|args: &[Value]| -> Result<Value, String> {
        match &args[0] {
            Value::String(s) => Ok(Value::String(s.to_uppercase().chars().rev().collect())),
            _ => Err("expected string".into()),
        }
    }),
);

let doc = parse(r#"x = upper_rev("hi")"#, opts);
```

## Filesystem Injection

```rust
use std::sync::Arc;
use wcl_lang::fs::{FileSystem, InMemoryFs};  // via re-exports

let mut fs = InMemoryFs::new();
fs.insert("/virtual/a.wcl", "x = 1");
let mut opts = ParseOptions::default();
opts.fs = Some(Arc::new(fs));
```

`FileSystem` trait is `Send + Sync`; `ImportResolver` is generic over `FS: FileSystem + ?Sized`.

## Error Handling

- `Document` is always returned; check `has_errors()` / iterate `errors()`.
- Deep errors surface as `Diagnostic` with `code` (`E001` …), `severity`, `message`, `span`.
- `eval_expression` / `query` / `call_function` return `Result<Value, String>` for runtime errors.

## Gotchas

- Do **not** use the deprecated `labels` field on `Block` / `BlockRef` — it was replaced by `inline_args` (and `_args` in the evaluated value when no schema applies).
- `ParseOptions` has an `fs` field, not a `FileSystem` trait object argument — pass `Some(Arc::new(my_fs))`.
- Custom functions are `Arc<dyn Fn(&[Value]) -> Result<Value, String> + Send + Sync>` — clone the `Arc` if you need to share.
