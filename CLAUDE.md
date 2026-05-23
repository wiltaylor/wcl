# WCL — Claude Code Instructions

This branch (`rewrite`) is a clean restart of WCL. The previous implementation lives on `main` — do not consult it for current behaviour.

## Layout

- `crates/wcl_lang` — language library: lexer, parser, AST, document view, lazy evaluator, schema validator, host-binding API
- `crates/wcl` — `wcl` CLI binary (`wcl parse`, `wcl check`, `wcl eval` / `wcl get`, `wcl set`, `wcl fmt`, `wcl repl`, `wcl lsp`)
- `crates/wcl_lsp` — `tower-lsp` language server driving `wcl lsp`
- `crates/wcl_lang/fuzz` — `cargo-fuzz` targets (parse + eval); run via `just fuzz <target>` on nightly
- `examples/` — fixture files used by tests (incl. `imports/` for module loading and `errors/` for negative diagnostics)

## What's implemented

- Hand-written lexer + recursive-descent parser → `Source` AST + `SymbolIndex`
- `Document` view layer with lazy, cached field evaluation and cycle detection
- Expression evaluator: literals, identifiers, member access, calls, function literals, let-bindings, block expressions, arithmetic / comparison / logical operators (no implicit coercions)
- Schema system: `@document`, `@schemaless`, `@block`, `@child`, `@children`, `@inline`, `@default`, `@table`, `@decorator`; structural validation via `Document::schema_errors()` / `SchemaViolationKind`
- Type system: `type`, `interface` (with `extends`), `union` (record / typeref / unit variants), `symbol_set`, builtin numeric + string variants, `list<T>`, `tensor<T, [...]>`, `fn(...) -> T`, named refs, `&T` reference fields with scope-aware lookup
- Host bindings: `Environment` registers synthetic types + builtin functions via `from_fn` (`FromValue` / `IntoValue` traits); each `BuiltinFn` can carry a printable signature
- Imports: eager top-level `import`; lazy `import` inside blocks
- Edit path: `parse_for_edit` + AST mutation + `format::to_source` round-trip, driving `wcl set` and `wcl fmt`
- Interactive REPL (`wcl repl [<file>]`) — plain stdin loop, evaluates ad-hoc expressions via `Document::eval_expr`
- JSON value serialization — `Value`/`TypeRef`/`Span` etc. derive `serde::{Serialize, Deserialize}`; `wcl get --json` emits resolved values as JSON. `Value::Function` is `#[serde(skip)]` (function bodies don't round-trip)
- LSP server (`wcl lsp` / `wcl lsp --tcp ADDR` / `wcl lsp --log <path>`): diagnostics, formatting, document symbols, go-to-definition + cross-file, find-references + cross-file, hover, completion (trigger-driven and identifier-position), semantic tokens incl. inside `${...}` interpolation slots, incremental text sync via `ropey`, multi-connection TCP listener
- Fuzz harness in `crates/wcl_lang/fuzz/` with `parse` and `eval` targets, seeded from `examples/`

## Intentionally deferred

Nothing on a current list. New deferred items get tracked alongside the slice that introduces them.

## Verification

A task is **not done** until all of these pass:

```bash
just test    # unit + integration tests across the workspace
just lint    # clippy --workspace -- -D warnings
cargo fmt --all -- --check
```

Run benches with `just bench` when changing the parser hot path.

## Conventions

- Hand-written lexer + recursive-descent parser. No nom, no parser generators.
- Diagnostics use `miette` + `thiserror`. Every parse error carries a `Span` and a `NamedSource` so the CLI can render snippets.
- Keep the dependency list minimal. Add a new crate only when it earns its place.
