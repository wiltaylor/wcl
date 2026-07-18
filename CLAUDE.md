# WCL — Claude Code Instructions

This branch (`rewrite`) is a clean restart of WCL. The previous implementation lives on
`main` — do not consult it for current behaviour.

This file is a **navigation map**, not a spec. For depth, follow the pointers in
[Where the docs live](#where-the-docs-live) rather than expecting it all inline.

## Layout

- `crates/wcl_lang` — language library: lexer, parser, AST, document view, lazy evaluator, schema validator, host-binding API
- `crates/wcl` — `wcl` CLI binary (`parse`, `check`, `eval` / `get`, `set`, `fmt`, `diff`, `init`, `answer`, `repl`, `lsp`, `editor`, `wdoc build` / `serve` / `pdf` / `markdown` / `skill` / `comments`). The `wdoc serve` watch+axum dev server lives here (`src/serve.rs`), since it's CLI-only (the watcher accumulates changed paths but rebuilds only on an explicit trigger — stdin Enter or `POST /__wdoc_rebuild`). `init` (project scaffolding) lives in `src/scaffold/` and embeds its own small WCL stdlib (`lib/scaffold/*.wcl` → `schema_registry()`) plus built-in templates (`templates/*.wcl`); a template opts in with `import <scaffold.wcl>` and references answered properties via an `answer("name")` builtin. See the `wcl init` implementation contract below. `editor` (`src/editor/`) serves the browser editor for the current directory — see the editor entry below and the [`editor-ui/`](#layout) bullet.
- `crates/wcl_lsp` — `tower-lsp` language server driving `wcl lsp`
- `crates/wcl_wdoc` — library crate driving the `wcl wdoc` subcommands; renders pages to HTML/PDF/Markdown. Embeds its WCL stdlib (`lib/*.wcl`) into a `wcl_lang::Registry` via `build.rs` (`schema_registry()`); a document opts in with `import <wdoc.wcl>`. See the [wdoc feature map](#wcl_wdoc-feature-map).
- `crates/wcl_lang/fuzz` — `cargo-fuzz` targets (`parse` / `eval` / `format_round_trip` / …); run via `just fuzz-run <target>` on nightly. See `crates/wcl_lang/fuzz/README.md`.
- `editor-ui/` — the `wcl editor` frontend: a SolidJS app on the Forge design system (`@forge/{ui,code,tokens}` consumed as git subdir deps from `github:wiltaylor/forge`), built with vite + pnpm. `crates/wcl/build.rs` rebuilds `editor-ui/dist` when the sources are newer (pnpm required; `WCL_EDITOR_UI_SKIP=1` embeds a placeholder instead) and `src/editor/assets.rs` embeds it via rust-embed (debug builds read the folder from disk — frontend iteration needs no cargo rebuild). Dev loop: `wcl editor` + `pnpm dev` (vite on :5174 proxying `/api`, incl. the LSP WebSocket). `pnpm-lock.yaml` is deliberately not committed (pnpm git-subdir-dep bug); bumping forge means updating the pinned codeload URLs in `package.json`'s `onlyBuiltDependencies`.
- `editors/vscode` — VS Code extension stub that spawns `wcl lsp` (`editors/vscode/README.md`)
- `editors/tree-sitter-wcl` — tree-sitter grammar stub (`editors/tree-sitter-wcl/README.md`)
- `examples/` — fixture files used by tests (incl. `imports/` for module loading, `errors/` for negative diagnostics)
- `docs/` — the user-facing documentation site, authored in WCL and built with `wcl wdoc`
- `.wad/` — the reference **WAD** (Wil's Architecture Document): a typed architecture data model of WCL itself (`wad.wcl` + `schema/` + per-view `data/` + `wdoc/book/` templates + `scripts/` extractors), rendered by `just wad-build` / `just wad-serve`. The canonical WAD schema is the `WAD_SCHEMA_BASE_WCL` heredoc in `crates/wcl/src/scaffold/templates/wad.wcl` (`just wad-schema-sync` / `wad-schema-check`, CI-gated); `wcl init wad` scaffolds new WADs; `wcl wad spec --from <rev>` derives change-spec skeletons from a diff (`crates/wcl/src/wad.rs`). Extractors under `.wad/scripts/` (uv single-file Python) own `data/generated/` — committed, never hand-edited (`just wad-extract`). How-to lives in the `wad` wskill (`docs/wskills/wad/`) — one mode-routed lifecycle skill (`wad plan | issue | build | doc`) that also absorbed the former wplan/wissue/wbuild skills; the seven `.claude/agents/*.md` files are generated from its `agent` blocks via `just skills-install`.
- `README.md` — user-facing quickstart

## Where the docs live

Prefer these over re-deriving behaviour from source:

- **Language / CLI / builtins reference** — the `wcl` wskill (`docs/wskills/wcl/`), a self-contained WCL model projected into a book (included in the site under `/wskills/wcl/`) and a skill. The `wdoc` block reference stays as a book at `docs/pages/reference/wdoc/*.wcl` (charts, diagrams, timelines, terminals, math, …). The landing (`docs/pages/wcl/index.wcl`) lists the wskills registry-style via `included_sites(...)`. Browse with `just docs-serve`; render to `docs/_site/` with `just docs-build`.
- **Crate internals** — each crate's `src/lib.rs` carries `//!` module docs; run `cargo doc --open` for the API surface.
- **Topic READMEs** — fuzz (`crates/wcl_lang/fuzz/README.md`), icon packs + licensing (`crates/wcl_wdoc/assets/icons/README.md`), editors (`editors/*/README.md`).
- **Auto-memory notes** (under `~/.claude/projects/-home-wil-dev-WCL/memory/`) — WCL authoring gotchas, wdoc theming, the wdoc PDF backend.

## What's implemented

Capability summary per crate; the reference pages above carry the detail.

- **`wcl_lang`** — hand-written lexer + recursive-descent parser → `Source` AST + `SymbolIndex`; strings/heredocs with escapes + interpolation (`<<'TAG'` raw form is verbatim). `Document` view with lazy, cached field eval + cycle detection. Expression evaluator (literals, member access, calls, fn literals, let-bindings, block exprs, arithmetic/comparison/logic with numeric promotion, `??` none-coalescing, `try`/`catch`). `fn name(…)` items (indexed lets). Schema constraints: `type Name = TypeRef` aliases + `@min`/`@max`/`@non_empty`. Schema system (`@document`/`@block`/`@child`/`@children`/`@inline`/`@default`/`@table`/`@decorator`), type system (`type`/`interface`/`union`/`symbol_set`/`list`/`tensor`/`fn`/`&T` refs), host bindings (`Environment`, `from_fn`, `FromValue`/`IntoValue`), and builtins (collections / tensors / strings / lists / math / control flow). Imports: quoted disk + angle-bracket system via `Registry`. Edit path: `parse_for_edit` + AST mutation + `format::to_source`. JSON value serialization. → see the `wcl` wskill (`docs/wskills/wcl/`) and `cargo doc`.
- **`wcl` CLI** — `parse` / `check` / `eval`+`get` / `set` / `fmt` / `diff` (WCL-aware entity/field document diff over the *evaluated* views — emits a re-parseable WCL tree by default, `--format json` for the flat change array; either side may be a `<rev>:<path>` git specifier whose imports resolve from that revision, materialised via `git archive | tar` — see `src/gitspec.rs`) / `init` (scaffold a project folder from a WCL template — a built-in name, a user template under `$XDG_DATA_HOME/wcl/templates/<name>/template.wcl`, or a template `.wcl` path / folder; built-ins ⟶ user templates ⟶ disk-path precedence, all surfaced by `--list`; collects property answers from `-D key=value`, an `--answers` `.wcl`/`.json` file, an interactive prompt, or the property `default`, then writes the `file`/`folder` blocks to disk; see `src/scaffold/`) / `repl` / `lsp` / `wdoc {build,serve,pdf,markdown,skill,comments}` (review comments live in `wcl editor`'s preview pane and persist to `comments.wcl` sidecars — no document rebuild; `comments` lists/`resolve`s them. `serve` is the plain watch/rebuild/live-reload dev server — editing lives in `wcl editor`) / `answer` (guided answer mode over `@answerable` question blocks from `import <answer.wcl>` — `--list` JSON, `--id`/`--pick`/`--text`/`--skip` for scripts, or an interactive walk-through with arrow-key menus via `stty` raw mode falling back to numbered line input; see `src/answer.rs`) / `editor` (browser editor for the current directory, `src/editor/`: gitignore-aware file tree (`ignore` crate walk), tabbed CodeMirror editing of any text file (the embedded `editor-ui/` SPA), `.wcl` LSP — completion/hover/diagnostics — over a WebSocket↔duplex bridge to an in-process `wcl_lsp` session (`src/editor/lsp_bridge.rs`; the server rewrites `initialize` to inject `initializationOptions.root`), and a site-scoped wdoc preview pane: `GET /api/sites` discovers every `site`-declaring entry under the served tree (nested by `include` membership) for a topbar picker, and a manual Rebuild full-builds the selected site with unsaved buffers overlaid (`comment_mode` + `edit_mode` anchors on, no injected scripts; root = explicit arg, else `./main.wcl` — needed only for schema-validated saves, the LSP root, and the review handshake). The preview pane also hosts the **review-comment UI** (comment on page / pick a block / list-edit-resolve, pins re-placed per iframe navigation — `src/editor/comments.rs` + `editor-ui/src/{preview/frame.js,state/comments.js,components/CommentPanel.jsx}`, endpoints `/api/comments*` + `/api/review/*`) and the **edit_object jump** (an "Edit this …" button in the rendered page resolves its object via `POST /api/object/locate` and opens the declaring `.wcl` in a tab selected at the instance — see the wdoc feature map). Saves run the validating `commit` pipeline with etag conflict detection; non-root `.wcl` buffers get syntax-only LSP diagnostics (isolated schema validation would be all false positives)). → see the `wcl` wskill (`docs/wskills/wcl/`) and `wcl --help`.
- **`wcl_lsp`** — diagnostics, formatting, document symbols, workspace symbol search, go-to-def + find-references (cross-file), hover, completion, signature help, semantic tokens (incl. `${…}` slots), schema-violation code actions, incremental sync (`ropey`), TCP listener. Resolves cross-file lookups through a root document; open buffers shadow disk via an overlay `FileLoader`. → see `cargo doc` on `wcl_lsp`.

**Implementation contracts** (non-obvious rules a contributor must respect, not in docs/):

- `let` **items** (`let name = expr` at file/block scope) are composition helpers: resolvable by name in sibling/descendant expressions but **invisible** to `Document::fields`/`blocks`, `get`, JSON, and schema validation (not in the symbol index). Distinct from the expression-level `let … ;` inside `{ }` blocks.
- **Bare record literals** (`{ name: value, … }`) coerce to the matching `Value::Variant` **by shape** when the declared type is a union or `list<union>` (`variant_dispatch::coerce_value_to_type`, run from `Field::value`, `build_variant`, and `invoke_fn_value`). The explicit `Union::Variant { … }` form overrides; no match ⇒ `VariantNoMatch`.
- **`@document` schemas compose per namespace.** The effective document schema for a namespace is the *merge* of every `@document` type visible there (`Document::doc_schemas_for_ns` → `DocSchemas` in `crates/wcl_lang/src/doc.rs`): a top-level field/block is legal if any member declares it, root-authored declarations preferred for type checks. Origin comes from `TypeDecl::is_imported` (set from the source's import path). `MultipleDocumentSchemas` fires only on a **second root-authored** `@document` in a namespace — imported (library) ones merge silently. This is why a user can `import <wdoc.wcl>` and still declare their own root `@document` to add top-level tags. Both the strict path (`schema_errors`) and the lazy path (`Field::schema_membership_error`/`declared_type_ref` in `doc/views.rs`) must consult the merge. **Corollary: gather-field names share the merged space** — a user `@document` field named like a wdoc one (`components`, `pages`, `sites`, `bodies`, …) resolves ambiguously and silently breaks template iteration (`each = components` fails as "unresolved reference" only at build time); the WAD schema gathers component instances as `sw_components` for exactly this reason.
- **The wskill base schema's canonical copy is the scaffold heredoc** (`crates/wcl/src/scaffold/templates/wskill.wcl`, terminator `WSK_SCHEMA_BASE_WCL`); the live copies under `docs/wskills/*/schema/base.wcl` are regenerated with `just wskill-schema-sync` and CI fails on drift (`just wskill-schema-check`). Topic-owned vocabularies (entity kinds, artifact kinds) live in each wskill's hand-editable `schema/kinds.wcl`. Scaffold `file`/`folder` blocks accept a `when: bool?` gate (conditional generation — the wskill template's optional presentation/training views use it), and `include` blocks / `included_sites` accept a `prefix` output override so two includes over one folder (a member's book + its deck) target distinct subdirs.
- **`wcl init` evaluates the template twice** (`src/scaffold/mod.rs`). Pass 1 opens the document with an `answer` builtin that returns `none` and reads only the `property` blocks (lazy eval never forces the `file`/`folder` bodies). After answers are resolved, pass 2 re-opens with `answer("name")` bound (a captured-closure builtin over the answers map, via `from_fn` + `Environment::add_builtin`) so heredoc `${answer(...)}` slots substitute; the file content **must** use the interpolating heredoc form `$<<TAG` (plain `<<TAG` is literal). Instance fields use `=` (`prompt = "…"`), not the `:` of `type` declarations. Answer `.wcl` files are read by evaluating their top-level field expressions off the AST (`parse_for_edit` + `eval_expr`), bypassing the strict `@document` membership check a bare `key = value` file would otherwise trip.

## `wcl_wdoc` feature map

**Mechanism:** an unrecognised block kind in a diagram/page dispatches to a WCL
`<kind>_lower` fn returning `list<SvgFundamental | HtmlFundamental | TermFundamental>`,
which the renderer recurses (depth-limited) until only fundamentals remain — this is how
user-declarable `@block(...) extends SvgBlock` (etc.) shapes plug in. A page-level block
that draws SVG (`sequence_diagram` / `state_diagram`) must still satisfy `WdocBlock`
(whose `lower` returns HTML fundamentals), so its geometry lives in a second `lower_svg`
fn; `render_lowered_svg_block` (`src/render/svg/standalone.rs`) fits a viewBox over the
result and all three backends dispatch to it. A handful of blocks
(terminal, card, node_table, tree, timeline, tilemap, dopesheet, map, wireframe widgets, icons,
math, code, table, list) are instead **special-cased in Rust** with stub WCL `lower`s, because their
output (calendar math, ANSI grids, external-image crops, LaTeX, syntax highlighting,
measured widget layout, valid nested list HTML) isn't expressible in WCL. The wireframe
`wf_*` family `extends SvgBlock`, so a widget is a diagram shape (placed by `x`/`y`,
connectable by edges), not a page block. Everything deeper → the doc page or the source.

| Feature | Rust | WCL stdlib | Doc page |
|---|---|---|---|
| core / pages / sites | `src/render/` | `core.wcl`, `headings.wcl`, `p.wcl`, `text.wcl` | `pages.wcl`, `sites.wcl`, `primitives.wcl` |
| inline patterns / formatting | `src/inline.rs` | `inline.wcl`, `inline-patterns.wcl` | `formatting.wcl` |
| tables / lists | `src/render/` | `table.wcl`, `list.wcl` | `tables.wcl` |
| code highlight | `src/highlight.rs` | `code.wcl` | `formatting.wcl` |
| callouts | (WCL `lower`) | `callout.wcl` | `formatting.wcl` |
| components / repeaters / partials | `src/render/` | `components.wcl` | `data-views.wcl` |
| images | `src/image.rs` | `image.wcl` | `images.wcl` |
| icons | `src/icons.rs` (+`build.rs`) | `icons.wcl` | `icons.wcl` |
| diagrams + layout/routing | `src/{layered,force,routing}.rs`, `src/render/` | `diagram-core.wcl`, `flowchart.wcl` | `diagrams.wcl`, `flowcharts.wcl`, `connections.wcl` |
| sequence diagrams | `src/render/svg/standalone.rs` | `sequence.wcl` | `sequence-diagrams.wcl` |
| state diagrams | `src/render/svg/standalone.rs` | `statechart.wcl` | `state-diagrams.wcl` |
| charts | (pure-WCL) | `charts.wcl` | `charts.wcl` |
| cards | `src/card.rs` | `card.wcl` | `diagrams.wcl` |
| node tables (DB / class, per-row ports) | `src/node_table.rs` | `node_table.wcl` | `primitives.wcl` |
| trees (indented file-tree) | `src/tree.rs` | `tree.wcl` | `tree.wcl` |
| timelines | `src/timeline.rs` | `timeline.wcl` | `timelines.wcl` |
| terminals / TUI | `src/terminal/` | `terminal.wcl`, `tui.wcl` | `terminals.wcl` |
| tilemaps | `src/tileset.rs` | `tilemap.wcl` | `tilemaps.wcl` |
| dopesheets | `src/dopesheet.rs` | `dopesheet.wcl` | `dopesheets.wcl` |
| maps | `src/map.rs` | `map.wcl` | `maps.wcl` |
| wireframe | `src/wireframe.rs` | `wireframe.wcl` | `wireframe.wcl` |
| math (LaTeX) | `src/math.rs` | `math.wcl` | `math.wcl` |
| themes / styling | `src/render/` | `theme.wcl`, `css-classes.wcl` | `styling.wcl` |
| templates / presentation | `src/render/` | `templates.wcl`, `presentation.wcl` | `sites.wcl`, `pages.wcl` |
| book sidebar footer buttons (`sidebar_footer { button … }` → `TemplateCtx.footer`) | `src/render/html.rs` (`read_sidebar_footer`, `FooterButtonNode`, `render_template` footer value w/ icon resolved via `patterns.icons()`), `src/build.rs` (thread `footer_nodes`, `footer_missing_page`) | `templates.wcl` (`wdoc_part_sidebar_footer`) | `sites.wcl` |
| website template (named `region`s + `<head>` assets) | `src/build.rs` (regions partition, `head_extra`, `assets` folder copy), `src/render/html.rs` (`Rendered{body,head}`, `render_page` head_extra), `src/render/lower.rs` (`Head` fundamental) | `website.wcl` | `websites.wcl` |
| block visibility (`@only`/`@except`) | `src/visibility.rs` | `visibility.wcl` | `visibility.wcl` |
| review comments (`comments.wcl` sidecar beside each wskill / root doc — page+locator keyed, placed client-side, watcher-ignored so no rebuild; `wcl wdoc comments` + the `wcl editor` preview pane's comment UI) | core `src/comments.rs`; editor endpoints `crates/wcl/src/editor/comments.rs`, UI `editor-ui/src/{preview/frame.js,state/comments.js,components/CommentPanel.jsx}` | `comment.wcl` | `comments.wcl` |
| review handshake (`wcl wdoc review <file>` — the agent's blocking wait: registers as waiting, blocks until the reviewer clicks "Send to agent" in the `wcl editor` preview pane, then prints the comments like `comments`; calling it again re-shows the banner — the "agent finished, rebuild & review" loop. File-based markers under `<tmp>/wcl-wdoc-review/<hash-of-canonical-root>/` (`serve`/`agent`/`ready`, round-keyed) — no HTTP client, no port discovery; `wcl editor` writes the live marker on startup (root document required) and clears it on Ctrl-C. Endpoints `GET /api/review/status` (long-poll on the `agent` marker) + `POST /api/review/ready`) | `crates/wcl_wdoc/src/review.rs` (`Handshake` markers), `crates/wcl/src/main.rs` (`run_review`), editor endpoints + banner in `crates/wcl/src/editor/comments.rs` + `editor-ui/` | — | — |
| edit_object jump (the `wcl editor` preview pane) — the editor's preview builds with `edit_mode: true` (alongside `comment_mode`), so block anchoring stamps `data-wcl-span`/`data-wcl-file` (byte offsets into the declaring file, gated on `InlinePatterns::edit_mode()`) and the **`edit_object` block** (stdlib `edit_object.wcl`, special-cased in `render_block` → emitted **only when `patterns.edit_mode()`**, skipped by `anchor_block` so it isn't itself selectable; `[]` WCL `lower` ⇒ nothing in build/markdown/pdf) renders an "Edit this …" button carrying `data-wcl-edit-kind`/`-target`. Clicking it in the preview iframe posts `{entry, page_file, kind, target, files}` to `POST /api/object/locate`, which overlays the unsaved buffers (`open_doc_for_edit_with_overlay`), scopes to the page's owning sub-site via `wcl_wdoc::doc_entry_for_page` (so a wskill page resolves its own kinds), matches the instance's first label (fallback: the kind name; no `target` requires exactly one instance), and answers `{file: <repo-relative>, span: {start, end}}`; the SPA opens that file in a tab and selects/scrolls the span (UTF-8-byte → UTF-16 conversion + a CM view registry in `editor-ui/src/state/views.js`). Saves go through the validating `commit` pipeline (`crates/wcl/src/edit.rs` — the shared editing core: `commit`/`restore`/`content_etag`/`format_source`/`locate_object`); constraint errors roll back (baseline-diff of `schema_errors`). Wired into every concept/entity/fact/process via the wskill component templates (`docs/wskills/*/wdoc/component/` + the `wskill` scaffold). The `@wdoc.file(path, folder?)` decorator remains declared vocabulary (a placement hint for editor tooling that creates objects). | `crates/wcl/src/edit.rs` (`locate_object` + commit core), route in `crates/wcl/src/editor/mod.rs` (`handle_object_locate`), anchors/buttons in `src/render/html.rs` (`anchor_block`, `render_edit_object_button`); client `editor-ui/src/{preview/frame.js,state/views.js,components/PreviewPane.jsx}` | `file_placement.wcl`, `edit_object.wcl` | — |
| Answer mode — `wcl answer <file>`: a guided walk over pending `@answerable` question blocks (`import <answer.wcl>` declares the decorator mapping prompt/response/status roles + pending/resolved/skipped symbols onto the user's own question schema, plus a ready-made `option` child block). `--list` JSON / `--id` non-interactive / interactive arrow-key menus — raw mode by shelling to `stty`, since the workspace forbids `unsafe`; numbered `:skip`/`:later`/`:quit` line input otherwise; re-discovers between answers because each write reformats the file and shifts spans; every answer writes through the validating `commit` pipeline — one question per write, per-answer durability. Fixture: `examples/answer/plan.wcl`. | `crates/wcl/src/{answer.rs,answer_tui.rs}` | `answer.wcl` (top-level import, not in the wdoc prelude) | wskill concept `answer_mode` |
| PDF backend | `src/pdf/` | — | (memory: wdoc-pdf-backend) |
| Markdown backend | `src/markdown/` | — | `markdown.wcl` |
| markdown_source (preview a page's generated Markdown in a `code markdown` block; book-only — taps the Markdown emitter's `body_to_markdown` seam from the HTML build, with a `start_page`/`pages`/`reference` skill-link layout so a skill page reproduces byte-for-byte) | `src/render/html.rs` (`render_markdown_source`), `src/markdown/emit.rs` (`body_to_markdown`), `src/inline.rs` (`with_skill_layout`, `skill_pages`/`output_dir`) | `markdown_source.wcl` | — |
| demo (example source + live light/dark preview of its children, side by side; renders children once and reuses the HTML for both palette-scoped `.wdoc-theme-*` wrappers — see the per-subtree palettes emitted by `site_theme_css`; `diagram = true` stacks the wide previews; degrades to source + one static render in Markdown/PDF) | `src/demo.rs`, dispatched in `src/render/html.rs`, `src/markdown/emit.rs`, `src/pdf/collect.rs`; scoped palettes in `src/render/theme.rs` | `demo.wcl` | — |
| skill folders (`:ai_skill`) | `src/markdown/skill.rs` | (`skill` block in `templates.wcl`) | `skills.wcl` |
| file assets (`file` block) | `src/file.rs` | `file.wcl` | `skills.wcl` |

Stdlib entry points: `lib/wdoc.wcl` → `lib/prelude.wcl` pulls in every part (split purely
for navigability; name resolution is order-independent across imports).

## Intentionally deferred

Deliberate, comment-documented gaps (not bugs), tracked so the list stays honest:

- **Richer value-type introspection in interface checking** — `crates/wcl_lang/src/doc/eval.rs` `check_value_implements_iface` structurally introspects variant-with-record and bare-record values; closures, lists, tensors, and scalars get a pass-through until the language carries runtime type tags for them.

New deferred items get tracked alongside the slice that introduces them.

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
- **Don't mention `file://` in docs or code comments.** When explaining that an
  asset resolves only over a server (not a direct local-file open), say "when
  served / hosted" and "not opened directly from disk" — never write the
  `file://` scheme.
