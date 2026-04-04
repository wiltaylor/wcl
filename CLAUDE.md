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

### Language library `wcl_lang` (`crates/wcl_lang/src/`)
- `lang/ast.rs` — AST types
- `lang/parser/mod.rs` — parser
- `schema/schema.rs` — schema resolution + validation
- `schema/decorator.rs` — built-in decorator registry (register new decorators here)
- `schema/tests/integration.rs` — integration tests
- All `Schema { ... }` construction sites need new fields (grep `Schema {`)

### LSP `wcl_lsp` (`crates/wcl_lsp/src/`)
- `fmt_impl.rs` — LSP formatter
- `semantic_tokens.rs` — semantic token collection
- `ast_utils.rs` — AST navigation for LSP
- `symbols.rs` — document symbol provider

### CLI `wcl` (`crates/wcl/src/`)
- `cli/fmt.rs` — CLI formatter

### Tree-sitter grammar (`extras/tree-sitter-wcl/`)
- `grammar.js` — add/update grammar rules
- Run `npx tree-sitter generate` to regenerate `src/parser.c`, `grammar.json`, `node-types.json`
- `queries/highlights.scm` — add new keywords/nodes to highlighting
- `test/corpus/*.txt` — add test cases for new syntax
- Run `npx tree-sitter test` (or `just test tree-sitter-wcl`) — all must pass

### VS Code extension (`editors/vscode/`)
- `syntaxes/wcl.tmLanguage.json` — add new keywords to TextMate regex
- No test suite exists for the extension

### Documentation (`docs/`) — MUST be updated for any language/syntax change
- `docs/guide-schemas.wcl` — schema features and examples
- `docs/guide-decorators-builtin.wcl` — built-in decorator reference table + sections
- `docs/appendix-error-codes.wcl` — add new error codes
- `docs/appendix-ebnf.wcl` — update EBNF grammar rules

### Bindings (`bindings/`)
- python, wasm, go, dotnet — usually NO changes needed (they consume evaluated JSON, not AST)
- **EXCEPTION**: When adding new `Value` variants, `bindings/wasm/src/js/convert.rs` (`value_to_js`) has an exhaustive match on `Value` that MUST be updated
- Only update if adding new FFI exports or changing the evaluated output shape
