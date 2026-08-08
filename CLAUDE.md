# WCL — Claude Code Instructions

This file is a **navigation map**, not a spec. For depth, follow the pointers in
[Where the docs live](#where-the-docs-live) rather than expecting it all inline.

## Layout

- `crates/wcl_lang` — language library: lexer, parser, AST, document view, lazy evaluator, schema validator, host-binding API
- `crates/wcl` — `wcl` CLI binary (`parse`, `check`, `eval` / `get`, `set`, `fmt`, `diff`, `init`, `repl`, `lsp`, `wdoc build` / `serve` / `pdf` / `markdown` / `skill`). The `wdoc serve` watch+axum dev server lives here (`src/serve.rs`), since it's CLI-only (the watcher accumulates changed paths but rebuilds only on an explicit trigger — stdin Enter or `POST /__wdoc_rebuild`). `init` (project scaffolding) lives in `src/scaffold/` and embeds its own small WCL stdlib (`lib/scaffold/*.wcl` → `schema_registry()`) plus built-in templates (`templates/*.wcl`); a template opts in with `import <scaffold.wcl>` and references answered properties via an `answer("name")` builtin. See the `wcl init` implementation contract below.
- `crates/wcl_lsp` — `tower-lsp` language server driving `wcl lsp`
- `crates/wcl_wdoc` — library crate driving the `wcl wdoc` subcommands; renders pages to HTML/PDF/Markdown. Embeds its WCL stdlib (`lib/*.wcl`) into a `wcl_lang::Registry` via `build.rs` (`schema_registry()`); a document opts in with `import <wdoc.wcl>`. See the [wdoc feature map](#wcl_wdoc-feature-map).
- `crates/wcl_lang/fuzz` — `cargo-fuzz` targets (`parse` / `eval` / `format_round_trip` / …); run via `just fuzz-run <target>` on nightly. See `crates/wcl_lang/fuzz/README.md`.
- `editors/vscode` — VS Code extension stub that spawns `wcl lsp` (`editors/vscode/README.md`)
- `editors/tree-sitter-wcl` — tree-sitter grammar stub (`editors/tree-sitter-wcl/README.md`)
- `examples/` — fixture files used by tests (incl. `imports/` for module loading, `errors/` for negative diagnostics)
- `docs/` — wcl.dev, authored in WCL and built with `wcl wdoc`: two entry documents, `landing/main.wcl` (the page at `/`) and `reference/main.wcl` (the 42-chapter book at `/reference/`, which draws the sample media in `docs/assets/` as `../assets/…`). See [Where the docs live](#where-the-docs-live).
- `.wad/` — the reference **WAD** (Wil's Architecture Document): a typed architecture data model of WCL itself (`wad.wcl` + `schema/` + per-view `data/` + `wdoc/book/` templates + `scripts/` extractors), rendered by `just wad-build` / `just wad-serve`. The canonical WAD schema is the `WAD_SCHEMA_BASE_WCL` heredoc in `crates/wcl/src/scaffold/templates/wad.wcl` (`just wad-schema-sync` / `wad-schema-check`, CI-gated); `wcl init wad` scaffolds new WADs (schema 0.6.0 makes `component.kind` the `ComponentKind` vocabulary over 0.5.0's `boundary` blocks + optional `boundary` on system/external_system, which the Systems view and the context diagram both read); `wcl wad spec --from <rev>` derives change-spec skeletons from a diff (`crates/wcl/src/wad.rs`). Extractors under `.wad/scripts/` (uv single-file Python) own `data/generated/` — committed, never hand-edited (`just wad-extract`). How-to lived in the `wad` wskill, which left with `docs/wskills/` at the switchover (#206); the installed copies under `.claude/agents/*.md` are what remains until the WAD itself goes (#207).
- `README.md` — user-facing quickstart

## Where the docs live

Prefer these over re-deriving behaviour from source:

- **The reference book** (`docs/reference/`) — the one manual for the language *and* wdoc, 42 chapters (`main.wcl` plus `pages/intro.wcl`, `pages/language/lang_*.wcl`, `pages/wdoc/wdoc_*.wcl`). **This is where a doc change belongs.** The tree is fixed by #156 and the whole skeleton landed at once (#157), because a `toc` entry or a link naming an unknown page is a build error; chapters replaced their stub one issue at a time, so a remaining stub is unwritten, not missing. Browse with `just docs-serve-ref` (`:8138`).
- **The landing page** (`docs/landing/`) — `main.wcl` plus `pages/{landing-parts,index}.wcl`: the one-page `website`-template site at `/`, its sections built from the `lp_*` components it declares itself. Browse with `just docs-serve` (`:8137`). The link to the book is a hand-written `./reference/` — the two sites are two documents, so `BuildError::BadLink` cannot check across them.
- **Both sites together** — `just docs-build` wipes `docs/_site/` and renders the landing to it, then the book to `docs/_site/reference/`. It is a gate part and it is what `.github/workflows/deploy-site.yml` runs, step for step: the gate builds what the deploy ships.
- **Crate internals** — each crate's `src/lib.rs` carries `//!` module docs; run `cargo doc --open` for the API surface.
- **Topic READMEs** — fuzz (`crates/wcl_lang/fuzz/README.md`), icon packs + licensing (`crates/wcl_wdoc/assets/icons/README.md`), editors (`editors/*/README.md`).
- **Auto-memory notes** (under `~/.claude/projects/-home-wil-dev-WCL/memory/`) — WCL authoring gotchas, wdoc theming, the wdoc PDF backend.

## What's implemented

Capability summary per crate; the reference pages above carry the detail.

- **`wcl_lang`** — hand-written lexer + recursive-descent parser → `Source` AST + `SymbolIndex`; strings/heredocs with escapes + interpolation (`<<'TAG'` raw form is verbatim). `Document` view with lazy, cached field eval + cycle detection. Expression evaluator (literals, member access, calls, fn literals, let-bindings, block exprs, arithmetic/comparison/logic with numeric promotion, `??` none-coalescing, `try`/`catch`). `fn name(…)` items (indexed lets). Schema constraints: `type Name = TypeRef` aliases + `@min`/`@max`/`@non_empty`. Schema system (`@document`/`@block`/`@child`/`@children`/`@inline`/`@default`/`@table`/`@decorator`/`@contextual`/`@declares_kind`), type system (`type`/`interface`/`union`/`symbol_set`/`list`/`tensor`/`fn`/`&T` refs), host bindings (`Environment`, `from_fn`, `FromValue`/`IntoValue`), and builtins (collections / tensors / strings / lists / math / control flow). Imports: quoted disk + angle-bracket system via `Registry`. Edit path: `parse_for_edit` + AST mutation + `format::to_source`. JSON value serialization. → see the reference book (`docs/reference/pages/language/`) and `cargo doc`.
- **`wcl` CLI** — `parse` / `check` / `eval`+`get` / `set` / `fmt` / `diff` (WCL-aware entity/field document diff over the *evaluated* views — emits a re-parseable WCL tree by default, `--format json` for the flat change array; either side may be a `<rev>:<path>` git specifier whose imports resolve from that revision, materialised via `git archive | tar` — see `src/gitspec.rs`) / `init` (scaffold a project folder from a WCL template — a built-in name, a user template under `$XDG_DATA_HOME/wcl/templates/<name>/template.wcl`, or a template `.wcl` path / folder; built-ins ⟶ user templates ⟶ disk-path precedence, all surfaced by `--list`; collects property answers from `-D key=value`, an `--answers` `.wcl`/`.json` file, an interactive prompt, or the property `default`, then writes the `file`/`folder` blocks to disk; see `src/scaffold/`) / `repl` / `lsp` / `wdoc {build,serve,pdf,markdown,skill}` (`serve` is the plain watch/rebuild/live-reload dev server). → see the reference book (`docs/reference/pages/`) and `wcl --help`.
- **`wcl_lsp`** — diagnostics, formatting, document symbols, workspace symbol search, go-to-def + find-references (cross-file), hover, completion, signature help, semantic tokens (incl. `${…}` slots), schema-violation code actions, incremental sync (`ropey`), TCP listener. Resolves cross-file lookups through a root document; open buffers shadow disk via an overlay `FileLoader`. → see `cargo doc` on `wcl_lsp`.

**Implementation contracts** (non-obvious rules a contributor must respect, not in docs/):

- `let` **items** (`let name = expr` at file/block scope) are composition helpers: resolvable by name in sibling/descendant expressions but **invisible** to `Document::fields`/`blocks`, `get`, JSON, and schema validation (not in the symbol index). Distinct from the expression-level `let … ;` inside `{ }` blocks.
- **Bare record literals** (`{ name: value, … }`) coerce to the matching `Value::Variant` **by shape** when the declared type is a union or `list<union>` (`variant_dispatch::coerce_value_to_type`, run from `Field::value`, `build_variant`, and `invoke_fn_value`). The explicit `Union::Variant { … }` form overrides; no match ⇒ `VariantNoMatch`.
- **`@document` schemas compose per namespace.** The effective document schema for a namespace is the *merge* of every `@document` type visible there (`Document::doc_schemas_for_ns` → `DocSchemas` in `crates/wcl_lang/src/doc.rs`): a top-level field/block is legal if any member declares it, root-authored declarations preferred for type checks. Origin comes from `TypeDecl::is_imported` (set from the source's import path). `MultipleDocumentSchemas` fires only on a **second root-authored** `@document` in a namespace — imported (library) ones merge silently. This is why a user can `import <wdoc.wcl>` and still declare their own root `@document` to add top-level tags. Both the strict path (`schema_errors`) and the lazy path (`Field::schema_membership_error`/`declared_type_ref` in `doc/views.rs`) must consult the merge. **Corollary: gather-field names share the merged space** — a user `@document` field named like a wdoc one (`components`, `pages`, `sites`, `bodies`, …) resolves ambiguously and silently breaks template iteration (`each = components` fails as "unresolved reference" only at build time); the WAD schema gathers component instances as `sw_components` for exactly this reason.
- **`@declares_kind` makes an *instance* declare a block kind, and the language derives that kind's schema.** A `@block` type carrying `@declares_kind(name = 0, params = "slots", body = "body")` (wdoc's `WdocComponent`, in `crates/wcl_wdoc/lib/components.wcl`) says its instances declare kinds; `Document::block_schema` falls back to a schema derived from the declarer's param blocks. Legacy untyped params remain `@schemaless utf8`; a typed `slot name: Type` preserves `Type` and is checked normally. A type marked `@block_slot` denotes a nested-block hole instead of a scalar field; its bare fill is scoped and checked by the host at build time. Defaults/`?` make scalar fields optional, remaining scalar names are listed in the derived `@block(kind, required_fields = [...])`, and `@contextual` marks the instance because its body expands through the host. Three consequences a contributor must respect: (1) the derived storage is a lazily-built `OnceLock` arena on `Document` (`declared_kinds`, which also serves `kind_declarer` — the old `component_index`), guarded against the re-entrancy of deriving-while-evaluating by the `deriving` thread set; (2) a derived schema is deliberately **absent from `type_decls()`** — it isn't a declaration, so anything introspecting by walking declarations won't find it and must go through `block_schema` / `derived_block_schema` / `TypeDecl::is_derived`; (3) the collision check and the derivation itself look kinds up **without** the fallback (`find_schema`), or a declared kind collides with itself.
- **A wdoc block declares exactly one rendering, and `@native` is the other half of `lower`** (`crates/wcl_wdoc/src/native.rs`). A type extending a lowering interface (`ContentBlock` / `SvgBlock` / `TermPrimitive`, transitively) carries **either** a `lower` field **or** `@native`; both, or neither, fails the build (`native_errors`, reached with `reserved_kind_errors` through the one `build::contract_errors` gate all four entry points call). `lower` is therefore declared *optional* on the interfaces — the check, not the type system, is what makes it exactly one, which is what killed the 57 stub `lower`s that returned `[]` while Rust intercepted the kind. `@native(backends = [:html, :markdown, :skill])` names the targets that implement the kind (bare `@native` = all four) and is cross-checked **both ways** against `NATIVE_DISPATCH`, the one Rust table of natively-dispatched kinds: a target claimed but not implemented, or implemented but not claimed, is an error rather than the next stub. **Not in the registry, and now empty**: a block with a real `lower` that a backend nonetheless special-cased — a hand-copy of a lowering the backend couldn't follow, not a native implementation. There were two, `callout` and `code` (Markdown), and the semantic content IR took both; there is nothing of that shape left to exempt. Rendering a native block on an uncovered target is a build error (`refuse_uncovered`), waived per instance with `@except(backends = […])`: capability says *can't*, intent says *don't want to*. **Two targets must cover it**, because two are involved: the backend actually rendering (each dispatch passes its own — `render_block` always passes `Html`, since a `card` body is HTML in whichever target embeds the SVG) and the output the build is producing (`patterns.backend()`). `markdown_source` is a renderer question, `file` an output one, and a `file` inside a card must not reach a PDF just because the card body renders as HTML; in the ordinary case the two are the same backend and it is one check. Adding a Rust arm for a kind means adding its registry row **and** its `@native` declaration; adding a backend to a kind means widening both.
- **`wcl init` evaluates the template twice** (`src/scaffold/mod.rs`). Pass 1 opens the document with an `answer` builtin that returns `none` and reads only the `property` blocks (lazy eval never forces the `file`/`folder` bodies). After answers are resolved, pass 2 re-opens with `answer("name")` bound (a captured-closure builtin over the answers map, via `from_fn` + `Environment::add_builtin`) so heredoc `${answer(...)}` slots substitute; the file content **must** use the interpolating heredoc form `$<<TAG` (plain `<<TAG` is literal). Instance fields use `=` (`prompt = "…"`), not the `:` of `type` declarations. Answer `.wcl` files are read by evaluating their top-level field expressions off the AST (`parse_for_edit` + `eval_expr`), bypassing the strict `@document` membership check a bare `key = value` file would otherwise trip. Scaffold `file`/`folder` blocks accept a `when: bool?` gate, so a template generates a part only when an answer asks for it; `include` blocks / `included_sites` accept a `prefix` output override, so two includes over one folder target distinct subdirs.

## `wcl_wdoc` feature map

**Mechanism:** an unrecognised block kind in a diagram/page dispatches to a WCL
`<kind>_lower` fn returning `list<Content | Svg | Html | TermFundamental>`,
which the renderer recurses (depth-limited) until only leaves remain — a fundamental, or a
node of the semantic content IR, which terminates the recursion on every backend rather
than only in HTML. This is how user-declarable `@block(...) extends SvgBlock` (etc.) shapes
plug in. Page-level blocks that draw SVG (`sequence_diagram` / `state_diagram`) lower to
`Content::Drawing`; the IR carries their typed `Svg` shapes and
`render/svg/standalone.rs` fits the same viewBox for every backend. A block that can't lower says so with
**`@native`** instead (terminal, card, node_table, tree, timeline, tilemap, dopesheet, map,
wireframe widgets, icons, table, list, and every structural wrapper), because its output
(calendar math, ANSI grids, external-image crops, measured widget layout, valid nested list
HTML) isn't expressible in WCL — see the `@native` contract below; there are no stub `lower`s
any more. `code` and `math` are **not** native: they lower to `Content::Code` /
`Content::Math`, and Rust computes the backend-specific highlighting/typesetting from that
fixed payload. The wireframe `wf_*` family `extends SvgBlock`, so a widget is a diagram shape
(placed by `x`/`y`, connectable by edges), not a page block. Everything deeper → the doc page
or the source.

| Feature | Rust | WCL stdlib | Doc page |
|---|---|---|---|
| core / pages / sites (`h1`..`h6` / `p` / `text` lower to `Content::Heading` / `Content::Paragraph`) | `src/render/`, `render/content_html.rs`, `pdf/content.rs`, `markdown/content.rs` | `core.wcl`, `headings.wcl`, `p.wcl`, `text.wcl` | `pages.wcl`, `sites.wcl`, `primitives.wcl` |
| output interfaces and placement (`ContentBlock` → `Content`, `SvgBlock` → `Svg`, `TermPrimitive` → `TermFundamental`; page/diagram/terminal placement belongs to each slot's `@children(...)` accepts-type, not to a parallel interface hierarchy) | generic lowering dispatch in `src/render/lower.rs` and `src/terminal/widgets.rs` | `core.wcl`, `terminal.wcl`, `tui.wcl` | `primitives.wcl`, `diagrams.wcl`, `terminals.wcl` |
| template page metadata (`page_metadata(c)` indexes each shared site TOC once into reading order, O(1) page positions/neighbours and active paths; page-local heading labels/ids/numbers derive from authored handles, while `render/postprocess.rs` stamps the same shared heading sequence into emitted HTML. The builtin never lowers a page body.) | `src/page_metadata.rs`, `render/{html,postprocess}.rs` | `templates.wcl` | `sites.wcl` |
| the `el` constructor family (`el(tag, cls, kids)` / `ela(…, attrs, …)` / `eli(…, id, …)` + the leaves `raw` / `inl` / `icon` / `para`) — each exactly its `Html` record with the field names dropped, three element constructors because a WCL parameter list is fixed at declaration (no defaults, no named arguments, and `?` in a param list is a parse error, so an optional is annotated as required and the `none` flows through). `ela` and `eli` share an arity, so the one positional mistake WCL catches can't separate them — that is stated in all three doc copies rather than designed around. Scoped to the HTML **element** vocabulary: `Svg` and the content IR keep the named-field literal, because they are field-shaped and WCL checks argument arity but never argument *types*, so transposing two of a shape's interchangeable `f64`s renders silently wrong where the record raises a shape mismatch. The long form stays legal and is the escape hatch for what the family doesn't name (id + attrs together, a `Paragraph` with an id, `Head`, `Table`, `Highlighted`, `Math`) | — (pure-WCL; `tests/el.rs` pins each constructor against its long form) | `el.wcl` | `sites.wcl` |
| semantic content IR (the closed, target-neutral `Content` union — one variant per document concept, no generic container, no raw-HTML escape. The Rust enum, its supporting records/symbol vocabularies and their `TryFrom<Value>` are **generated** at build time from the WCL declaration, which walks every type reachable from `union Content`; an unmappable field type fails the build rather than becoming a hole. **Every backend matches it exhaustively** — `render/content_html.rs`, `pdf/content.rs`, `markdown/content.rs` (which the skill target shares), no catch-all arm anywhere — so a variant added to the union is a compile error in three places rather than silence in three outputs; that IS the mechanism, not a convention. The complete 35-`ContentBlock` classification lives beside the union: 16 fixed-payload producers (`callout`, `chapter_header`, `code`, `footnotes`, `h1`..`h6`, `math`, `p`, `text`, `sequence_diagram`, `state_diagram`, `video`) and 19 documented natives that require an authored subtree or renderer-only state. Two consequences to respect: a `Content::Heading` renders as a real `<hN class="heading-N">` — the class is a style hook DERIVED from the number, and `render/postprocess.rs`'s page-wide anchor/marker pass matches that shape — and a `text`'s `span` children concatenate into one `Content::Paragraph`, so a per-span `id` / `class` no longer renders anywhere (the IR carries prose, not styled runs; the fields stay declared and an inline-run concept is separate work). A lowered value is classified by its **union tag** (`content::as_content`), never its variant name — the IR and the HTML vocabulary both declare `Paragraph`, `Table` and `Math` — and the custom-variant recursion that used to live only in the HTML renderer now runs in all three walkers, so a user block whose `lower` returns another custom variant reaches every backend. See `spec/wdoc-substrate/02-blocks.md` §2.1–§2.4, §2.9 on the branch that holds it.) | `build/content_ir.rs` (the emitter, run from `build.rs` → `$OUT_DIR/content_ir.rs`), `src/content.rs` (the hand-written half: `ContentError`, the `Value` readers, `as_content`, and the typed-`Svg` → `Value` bridge a `Drawing` crosses), the `Lowered` seam in `render/lower.rs` (`lower_block`), the three walkers | `content.wcl` | — |
| inline patterns / formatting | `src/inline.rs` | `inline.wcl`, `inline-patterns.wcl` | `formatting.wcl` |
| tables / lists | `src/render/` | `table.wcl`, `list.wcl` | `tables.wcl` |
| code highlight (a WCL `ContentBlock` lowering to `Content::Code`, not a native/special-cased block; each backend draws its own chrome — HTML the code-card, Markdown a fence under the filename, PDF a caption over coloured runs — and `highlight.rs` is only the leaf that colours) | `src/highlight.rs`, `render/content_html.rs`, `pdf/content.rs`, `markdown/content.rs` | `code.wcl` | `formatting.wcl` |
| callouts (lower to `Content::Callout`; the accent/icon/alert-keyword mapping is each backend's, keyed on the `CalloutKind` symbol — no backend matches a class name any more) | `render/content_html.rs`, `pdf/content.rs`, `markdown/content.rs` | `callout.wcl` | `formatting.wcl` |
| chapter headers / footnotes (lower to `Content::ChapterHeader` / `Content::Footnotes`, so the kicker, reading time, updated date, version, the section title and the note markers reach every backend; the meta line's separator is the one shared `content::chapter_meta_line`. A footnote's `marker` is its declaration id — the key `fn-<marker>` / `fnref-<marker>` anchor on, which is what `render/postprocess.rs`'s `[^id]` rewrite looks for, and a GFM footnote label in Markdown) | `render/content_html.rs`, `pdf/content.rs`, `markdown/content.rs` | `chapter_header.wcl`, `footnotes.wcl` | — |
| components / repeaters / partials | `src/render/` | `components.wcl` | `data-views.wcl` |
| images | `src/image.rs` | `image.wcl` | `images.wcl` |
| icons | `src/icons.rs` (+`build.rs`) | `icons.wcl` | `icons.wcl` |
| diagrams + layout/routing | `src/{layered,force,routing}.rs`, `src/render/` | `diagram-core.wcl`, `flowchart.wcl` | `diagrams.wcl`, `flowcharts.wcl`, `connections.wcl` |
| sequence diagrams (their real WCL lowering computes the geometry once and returns the page-level `Content::Drawing { shapes }` bridge; there is no second backend-only geometry function) | `src/render/svg/standalone.rs` fits the typed drawing | `sequence.wcl` | `sequence-diagrams.wcl` |
| state diagrams (same `Content::Drawing` bridge; one WCL geometry lowering) | `src/render/svg/standalone.rs` fits the typed drawing | `statechart.wcl` | `state-diagrams.wcl` |
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
| math (a WCL `ContentBlock` lowering to `Content::Math`, not a native/special-cased block; `math.rs` is the LaTeX-to-SVG leaf used by the backend readings) | `src/math.rs`, `render/content_html.rs`, `pdf/content.rs`, `markdown/content.rs` | `math.wcl` | `math.wcl` |
| themes / styling | `src/render/` | `theme.wcl`, `css-classes.wcl` | `styling.wcl` |
| templates / presentation | `src/render/` | `templates.wcl`, `presentation.wcl` | `sites.wcl`, `pages.wcl` |
| book sidebar footer buttons (`sidebar_footer { button … }` → `TemplateCtx.footer`) | `src/render/html.rs` (`read_sidebar_footer`, `FooterButtonNode`, `render_template` footer value w/ icon resolved via `patterns.icons()`), `src/build.rs` (thread `footer_nodes`, `footer_missing_page`) | `templates.wcl` (`wdoc_part_sidebar_footer`) | `sites.wcl` |
| website template (named slots + `<head>` assets) | `src/build.rs` (slot-contract validation/routing, `head_extra`, `assets` folder copy), `src/render/html.rs` (`Rendered{body,head}`, slot handles, `render_page` head_extra), `src/render/lower.rs` (`Head` / `Blocks` fundamentals) | `website.wcl` | `websites.wcl` |
| block visibility (`@only`/`@except`; the `backends` axis names four targets — `:html` / `:pdf` / `:markdown` / `:skill`, the last added so a skill build can be scoped apart from a Markdown one) | `src/visibility.rs`, `Backend` in `src/inline.rs` | `visibility.wcl` | `visibility.wcl` |
| native blocks + declared backend coverage (`@native` / `@native(backends = […])`, the exactly-one-of check, the registry cross-check, and the uncovered-target build error) | `src/native.rs` | `core.wcl` | `primitives.wcl`, `visibility.wcl` |
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
- **Type arguments on `TypeRef::Named` are syntax only** — `content<SvgBlock>` parses (`crates/wcl_lang/src/parser/types.rs` `parse_type_args`), prints, and is readable via `TypeRef::type_args()`, but nothing checks arity, nothing substitutes, and there is no `type Foo<T>` declaration form. A named type resolves by `path` alone, so `content<Nonsense>` parses and only fails later (if at all). This is enough for slot derivation, which emits both a typed field and a `@children(X)` decorator and lets the decorator do the child-kind checking. Full generics are a separate effort.
- **A `text` block's `span` children lose their `id` / `class`** — the IR has no inline-run concept, so a `text` concatenates its spans into one `Content::Paragraph`. Outside HTML this changed nothing (the Markdown and PDF walkers already flattened a `text` to its child text), but in the book a `span "…" { class = ["accent"] }` stopped painting. The fields stay declared so existing documents keep validating; carrying them back needs an inline-run node in the IR, which is its own decision.
- **Legacy declared-kind params remain permissive** — an old untyped `wdoc_slot` has no value type to preserve, so `derive_kind_schema` emits `@schemaless utf8` for compatibility. New `slot name: Type` scalar params preserve and check `Type`; `@block_slot` types are host-checked nested content holes rather than derived scalar fields.
- **`if let` still requires its `else`** — a plain `if` may omit the branch (the untaken side is `none`, `crates/wcl_lang/src/parser/expr.rs` `parse_if_expr`), but `if let` does not; `eval_if_let` would happily return `none` on a no-match, so this is scope, not a semantic obstacle.

New deferred items get tracked alongside the slice that introduces them.

## Verification

A task is **not done** until the merge bar passes:

```bash
just ci::check
```

That's the **`ci` just module** (`.just/ci/mod.just`) — the whole gate. `just ci::<part>`
runs one part, which is exactly what each step of `.github/workflows/ci.yml` invokes, so
the workflow and the local gate can't drift. While iterating, the three parts that fail
most are worth running alone:

```bash
just workspace-test    # unit + integration tests across the workspace
just workspace-lint    # clippy --workspace -- -D warnings
just fmt-check         # cargo fmt --all -- --check
```

A module can't see its parent's recipes, so the gate's constituents live in
`.just/shared.just`, imported by **both** the root justfile and the module (each defined
once; `just workspace-test` and `just ci::workspace-test` are the same recipe). Two
deliberate exceptions: `just ci::fuzz-sweep` is a part but not in `check` (it needs nightly
+ cargo-fuzz, and runs as its own CI job), and `wad-extract-check` is the one part that
isn't side-effect-free — it regenerates the tracked files under `.wad/data/generated/`,
which is how it judges their freshness.

Run benches with `just workspace-bench` when changing the parser hot path.

## Conventions

- Hand-written lexer + recursive-descent parser. No nom, no parser generators.
- Diagnostics use `miette` + `thiserror`. Every parse error carries a `Span` and a `NamedSource` so the CLI can render snippets.
- Keep the dependency list minimal. Add a new crate only when it earns its place.
- **Don't mention `file://` in docs or code comments.** When explaining that an
  asset resolves only over a server (not a direct local-file open), say "when
  served / hosted" and "not opened directly from disk" — never write the
  `file://` scheme.
