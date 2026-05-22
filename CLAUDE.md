# WCL — Claude Code Instructions

This branch (`rewrite`) is a clean restart of WCL. The previous implementation lives on `main` — do not consult it for current behaviour.

## Layout

- `crates/wcl_lang` — language library: lexer, parser, AST, document view, lazy evaluator, schema validator, host-binding API
- `crates/wcl` — `wcl` CLI binary (`wcl parse`, `wcl check`, `wcl eval` / `wcl get`, `wcl set`, `wcl fmt`)
- `examples/` — fixture files used by tests (incl. `imports/` for module loading and `errors/` for negative diagnostics)

## What's implemented

- Hand-written lexer + recursive-descent parser → `Source` AST + `SymbolIndex`
- `Document` view layer with lazy, cached field evaluation and cycle detection
- Expression evaluator: literals, identifiers, member access, calls, function literals, let-bindings, block expressions, arithmetic / comparison / logical operators (no implicit coercions)
- Schema system: `@document`, `@schemaless`, `@block`, `@child`, `@children`, `@inline`, `@default`, `@table`, `@decorator`; structural validation via `Document::schema_errors()` / `SchemaViolationKind`
- Type system: `type`, `interface` (with `extends`), `union` (record / typeref / unit variants), `symbol_set`, builtin numeric + string variants, `list<T>`, `tensor<T, [...]>`, `fn(...) -> T`, named refs, `&T` reference fields with scope-aware lookup
- Host bindings: `Environment` registers synthetic types + builtin functions via `from_fn` (`FromValue` / `IntoValue` traits)
- Imports: eager top-level `import`; lazy `import` inside blocks
- Edit path: `parse_for_edit` + AST mutation + `format::to_source` round-trip, driving `wcl set` and `wcl fmt`

## Intentionally deferred

- LSP server
- Interactive REPL (the one-shot `wcl eval <file> <path>` exists; full REPL doesn't)
- Serde / binary serialization of `Document`
- Fuzz harness

Don't add any of these without an explicit ask.

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
