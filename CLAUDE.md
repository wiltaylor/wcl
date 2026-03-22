# WCL — Claude Code Instructions

## Verification

A task is **not done** until both of these pass:

```bash
just test::all    # full test suite: Rust workspace, tree-sitter, Go bindings
just pack::all    # packaging / distribution builds
```

Also run before considering work complete:
```bash
cargo clippy --workspace --exclude wcl_wasm
cargo fmt --check
```

## Change Checklist

When making changes to WCL (especially AST, parser, schema, or language features), update ALL of these:

### Core Rust crates
- `crates/wcl_core/src/ast.rs` — AST types
- `crates/wcl_core/src/parser/mod.rs` — parser
- `crates/wcl_schema/src/schema.rs` — schema resolution + validation
- `crates/wcl_schema/src/decorator.rs` — built-in decorator registry (register new decorators here)
- `crates/wcl_schema/tests/integration.rs` — integration tests
- `crates/wcl_cli/src/fmt.rs` — CLI formatter
- `crates/wcl_lsp/src/fmt_impl.rs` — LSP formatter
- `crates/wcl_lsp/src/semantic_tokens.rs` — semantic token collection
- `crates/wcl_lsp/src/ast_utils.rs` — AST navigation for LSP
- `crates/wcl_lsp/src/symbols.rs` — document symbol provider
- All `Schema { ... }` construction sites need new fields (grep `Schema {`)

### Tree-sitter grammar (`extras/tree-sitter-wcl/`)
- `grammar.js` — add/update grammar rules
- Run `npx tree-sitter generate` to regenerate `src/parser.c`, `grammar.json`, `node-types.json`
- `queries/highlights.scm` — add new keywords/nodes to highlighting
- `test/corpus/*.txt` — add test cases for new syntax
- Run `npx tree-sitter test` (or `just test tree-sitter-wcl`) — all must pass

### VS Code extension (`editors/vscode/`)
- `syntaxes/wcl.tmLanguage.json` — add new keywords to TextMate regex
- No test suite exists for the extension

### Documentation (`docs/book/src/`) — MUST be updated for any language/syntax change
- `guide/schemas.md` — schema features and examples
- `guide/decorators-builtin.md` — built-in decorator reference table + sections
- `appendix/error-codes.md` — add new error codes
- `appendix/ebnf.md` — update EBNF grammar rules

### Bindings (`bindings/`)
- python, wasm, go, dotnet — usually NO changes needed (they consume evaluated JSON, not AST)
- **EXCEPTION**: When adding new `Value` variants, `bindings/wasm/src/js/convert.rs` (`value_to_js`) has an exhaustive match on `Value` that MUST be updated
- Only update if adding new FFI exports or changing the evaluated output shape
