# WCL — Claude Code Instructions

This branch (`rewrite`) is a clean restart of WCL. The previous implementation lives on `main` — do not consult it for current behaviour. The language is being rebuilt incrementally; the surface today is only HCL-like fields and blocks.

## Layout

- `crates/wcl_lang` — parser + AST library
- `crates/wcl` — `wcl` CLI binary (`wcl parse`, `wcl check`)
- `examples/` — fixture files used by tests

There is no LSP, no bindings, no schema system, no evaluator yet. Add them back only when explicitly asked.

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
