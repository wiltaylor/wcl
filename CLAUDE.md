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
- python, wasm, go, dotnet, jvm, ruby, zig — usually NO changes needed (they consume evaluated JSON, not AST)
- **EXCEPTION**: When adding new `Value` variants, `bindings/wasm/src/js/convert.rs` (`value_to_js`) has an exhaustive match on `Value` that MUST be updated
- Only update if adding new FFI exports or changing the evaluated output shape

### Claude Code skill (`.claude/skills/wcl/`)
When the language, CLI, wdoc format, or bindings change, update the matching reference file. See `.claude/skills/wcl/reference/sync.md` for the full source → reference map. Quick map:

- `crates/wcl_lang/src/schema/decorator.rs` → `reference/schemas-and-decorators.md`
- `crates/wcl_lang/src/eval/functions.rs` → `reference/builtin-functions.md`
- `crates/wcl_lang/src/lang/ast.rs` / parser → `reference/syntax.md`
- `docs/appendix-error-codes.wcl` → `reference/error-codes.md`
- `crates/wcl/src/cli/*.rs` → `reference/cli.md`
- `crates/wcl_wdoc/src/wdoc.wcl` → `reference/wdoc.md` and/or `reference/wdoc-drawings.md`
- `crates/wcl_wdoc/src/shapes.rs` / `graph_layout.rs` → `reference/wdoc-drawings.md`
- `bindings/{python,wasm,go,dotnet,jvm,ruby,zig}/**` → `reference/bindings/<lang>.md`
- `crates/wcl_ffi/**` → `reference/bindings/c.md`
- `crates/wcl_lang/src/lib.rs` public API → `reference/bindings/rust.md`

Keep `SKILL.md` short — add new reference files rather than growing the entry point.
