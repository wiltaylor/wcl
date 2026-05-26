# WCL — Claude Code Instructions

This branch (`rewrite`) is a clean restart of WCL. The previous implementation lives on `main` — do not consult it for current behaviour.

## Layout

- `crates/wcl_lang` — language library: lexer, parser, AST, document view, lazy evaluator, schema validator, host-binding API
- `crates/wcl` — `wcl` CLI binary (`wcl parse`, `wcl check`, `wcl eval` / `wcl get`, `wcl set`, `wcl fmt`, `wcl repl`, `wcl lsp`, `wcl wdoc build`, `wcl wdoc serve`)
- `crates/wcl_lsp` — `tower-lsp` language server driving `wcl lsp`
- `crates/wcl_wdoc` — library crate driving the `wcl wdoc` subcommands: `wcl_wdoc::build(file, out_dir)` renders pages to HTML, `wcl_wdoc::serve(file, out, addr)` runs the watch+axum dev server. Ships a bundled `wdoc.wcl` schema with fundamental blocks (`page`, `text` + `span`, `column`, `diagram` + shapes `rect` / `circle` / `line` / `label` / `polygon` / `container`), the layout + anchor system on container and shapes, and `class` for styling. **User-declarable custom shapes**: any `@block("kind") type ...` paired with a top-level `kind_lower = fn(...) -> list<SvgFundamental>` (or `list<HtmlFundamental>`) plugs into the renderer — when an unrecognised block kind appears in a diagram or page, the renderer calls its `<kind>_lower` function and recursively lowers the returned variants until only fundamentals remain (depth-limited, error-marker on overflow). The stdlib uses exactly this mechanism: `h1`..`h6` lower to `Paragraph` variants, and `process` / `decision` / `terminator` lower to SVG primitives. **Syntax-highlighted code blocks**: `@block("code") { @inline(0) language source }` renders via `syntect` (pure-Rust `fancy-regex` backend) plus `two-face`'s curated extra grammars (Rust, Python, TS, TOML, Dockerfile, …) — emits `<pre class="code-block"><code class="language-X">` with `<span class="tok-…">` tokens; an unknown language tag falls back to plain text. A bundled `wcl.sublime-syntax` grammar makes the site self-highlighting; the bundled `code-theme.css` ships token colours that get injected into every page's `<style>` and can be overridden by user `class` blocks. **Tables**: `@block("table")` authored with WCL's native pipe-row syntax (`rows: | a | b |`, first row = header); cells render through the inline-pattern engine; lowers to an `HtmlFundamental::Table`. **Page templates**: a `site` block sets a `default_template` (+ `title`); a page overrides it via its own `template = :name` field. A `@block("template")` carries `render = fn(TemplateCtx) -> list<HtmlFundamental>`; the renderer builds the `TemplateCtx` (the page's rendered body as the `content` part, the site title, and the list of every page as `PageRef`s) and the returned fundamentals become the `<body>` (`render_page` still owns `<head>`/CSS). Templates compose structure with two general fundamentals — `Element { tag id? class? attrs? children }` (recursive) and `Raw { html }` (verbatim) — and build their own nav/menus from `ctx.pages`; there's no built-in menu. "Parts" are just functions returning `list<HtmlFundamental>` that a template calls and embeds. Two stdlib templates ship: `webpage` (Hugo-style header + top nav) and `book` (mdbook-style fixed left chapter sidebar with current-chapter highlight + reading column; the sidebar is a nested, ordered table of contents declared via a `toc { chapter "Title" { page = name?  …nested chapters } }` block inside `site` — recursive `chapter` entries nest to any depth, `page`-less chapters are grouping headings, and a `chapter` pointing at an unknown page is a build error); their default styling is injected via `SITE_CSS` / `BOOK_CSS`. No `site`/`template` ⇒ pages render bare (pre-template behaviour). `serve` renders into a tempdir (or `--out`) by calling `build()`, watches the source dir via `notify` and rebuilds on every `.wcl` change; axum routes `/` and `/<name>[.html]` are static-file reads off that directory. Build errors are logged to stderr; the last successful render keeps serving.
- `crates/wcl_lang/fuzz` — `cargo-fuzz` targets (parse + eval); run via `just fuzz-run <target>` on nightly
- `editors/vscode` — minimal VS Code extension stub that spawns `wcl lsp` for `.wcl` files
- `editors/tree-sitter-wcl` — tree-sitter grammar stub for editors that consume them
- `examples/` — fixture files used by tests (incl. `imports/` for module loading and `errors/` for negative diagnostics)
- `README.md` — user-facing quickstart

## What's implemented

- Hand-written lexer + recursive-descent parser → `Source` AST + `SymbolIndex`
- `Document` view layer with lazy, cached field evaluation and cycle detection
- Expression evaluator: literals, identifiers, member access, calls, function literals, let-bindings, block expressions, arithmetic / comparison / logical operators with implicit numeric promotion (`1 + 2.0`, `1u32 == 1i64`)
- `let` items: `let name = expr` at the file (global) scope or inside any block. Composition helpers (values or functions) that resolve by name in sibling/descendant expressions but are **not** document data — invisible to `Document::fields`/`blocks`, `get`, JSON, and schema validation (not registered in the symbol index). Lexically scoped (inner shadows outer), lazily evaluated, cycle-detected. Distinct from the expression-level `let … ;` inside `{ }` block expressions; item form takes no terminator.
- Schema system: `@document`, `@schemaless`, `@block`, `@child`, `@children`, `@inline`, `@default`, `@table`, `@decorator`; structural validation via `Document::schema_errors()` / `SchemaViolationKind`
- Type system: `type`, `interface` (with `extends`), `union` (record / typeref / unit variants), `symbol_set`, builtin numeric + string variants, `list<T>`, `tensor<T, [...]>`, `fn(...) -> T`, named refs, `&T` reference fields with scope-aware lookup
- Host bindings: `Environment` registers synthetic types + builtin functions via `from_fn` (`FromValue` / `IntoValue` traits); each `BuiltinFn` can carry a printable signature
- Host-callable functions: `Document::call_function(name, &[Value])` looks up a top-level binding and invokes it; `Document::call_value(&FnValue, &[Value])` runs an already-resolved function value. Both use a fresh evaluation context rooted at the document. Pairs with expression-form variant construction (`Union::Variant { field: expr, ... }`) so hosts can ship interpreters over user-supplied lowering functions. Local-bound records/variants participate in member-access (`p.x` works for a record-typed parameter).
- Builtins: collections (`map`/`filter`/`fold`/`len`/`sum`/`range`/`head`/`tail`/`flatten`/`zip`), tensors (`tensor`/`tensor_data`/`tensor_shape`/`tensor_reshape`), strings (`split`/`join`/`replace`/`contains`/`starts_with`/`ends_with`/`to_upper`/`to_lower`/`trim`), lists (`list_contains`/`reverse`/`sort`/`unique`/`index_of`/`take`/`drop`), control flow (`error`/`panic`/`assert`/`format`/`concat`)
- Imports: eager top-level `import`; lazy `import` inside blocks
- Edit path: `parse_for_edit` + AST mutation + `format::to_source` round-trip, driving `wcl set` and `wcl fmt`. `format::FormatConfig` exposes indent / trailing-comma / blank-line cap; `wcl fmt --indent` / `--no-trailing-comma` surface them at the CLI.
- Interactive REPL (`wcl repl [<file>]`) — plain stdin loop with multiline continuation; tagged parse vs eval errors; `:quit` exits
- JSON value serialization — custom one-way `Serialize` on `Value` emits idiomatic JSON (scalars as primitives, lists as arrays, records as objects, variants as `"Name"` / `{"Name": payload}`); `wcl get --json` uses it. `TypeRef`/`Span`/`BuiltinType` still round-trip via derive.
- LSP server (`wcl lsp` / `wcl lsp --tcp ADDR` / `wcl lsp --log <path>`): diagnostics, formatting, document symbols, go-to-definition + cross-file, find-references + cross-file, hover, completion (trigger-driven and identifier-position with locals + builtins), semantic tokens incl. inside `${...}` interpolation slots, code actions (quick-fixes for unknown-field and disallowed-child schema violations), incremental text sync via `ropey`, multi-connection TCP listener. **Root document**: on `initialize`, the server picks up `initializationOptions.root` (or falls back to `<workspace>/main.wcl`) and resolves every cross-file lookup through it. Open editor buffers shadow disk contents via an overlay [`FileLoader`] (`wcl_lang::overlay_loader`), so unsaved edits in any imported file participate in the root parse.
- LSP integration tests (`crates/wcl_lsp/tests/server.rs`) drive `Backend`'s `LanguageServer` trait directly through `tower-lsp`
- Fuzz harness in `crates/wcl_lang/fuzz/` with `parse`, `eval`, and `format_round_trip` targets, seeded from `examples/`. CI runs a 30 s `parse` smoke on every push.
- Editor stubs in `editors/` — VS Code extension that spawns `wcl lsp`, tree-sitter grammar for highlighting

## Intentionally deferred

Nothing on a current list. New deferred items get tracked alongside the slice that introduces them.

## Verification

A task is **not done** until all of these pass:

```bash
just workspace-test    # unit + integration tests across the workspace
just workspace-lint    # clippy --workspace -- -D warnings
cargo fmt --all -- --check
```

Run benches with `just workspace-bench` when changing the parser hot path.

## Conventions

- Hand-written lexer + recursive-descent parser. No nom, no parser generators.
- Diagnostics use `miette` + `thiserror`. Every parse error carries a `Span` and a `NamedSource` so the CLI can render snippets.
- Keep the dependency list minimal. Add a new crate only when it earns its place.
