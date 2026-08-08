# WCL — Claude Code Instructions

This file is a **navigation map**, not a spec. For depth, follow the pointers in
[Where the docs live](#where-the-docs-live) rather than expecting it all inline.

## The one rule

> **Any change to WCL or wdoc updates both the book chapter and its mirrored skill
> reference.**

The docs exist twice, on purpose:

- `docs/reference/pages/<area>/<stem>.wcl` — the chapter a human reads on wcl.dev.
- `.claude/skills/wcl/references/<area>/<stem>.md` — the same material an agent reads.

The two trees have the **same 42 stems** (`intro`, `language/lang_*`, `wdoc/wdoc_*`).
A stem is one topic in both places. Write the chapter, then write the reference.

Nothing enforces this. #148 rejected the machinery — no report recipe, no CI gate — and
#153 removed `wcl-refcheck` on the same principle. This instruction and the
codebase-health review are the **whole** drift defence. If you change behaviour and
update only one copy, the other copy is now wrong and no build will tell you.

## Layout

- `crates/wcl_lang` — the language library: lexer, parser, AST, document view, lazy
  evaluator, schema validator, host-binding API.
- `crates/wcl` — the `wcl` CLI binary: `parse`, `check`, `eval` / `get`, `set`, `fmt`,
  `diff`, `init`, `repl`, `lsp`, and `wdoc build` / `serve` / `pdf` / `markdown`. The
  `wdoc serve` watch + axum dev server lives here (`src/serve.rs`), because it is
  CLI-only. Its watcher accumulates changed paths but rebuilds only on an explicit
  trigger — stdin Enter, or `POST /__wdoc_rebuild`. `init` (project scaffolding) lives in
  `src/scaffold/`. It embeds its own small WCL stdlib (`src/scaffold/lib/*.wcl` →
  `schema_registry()`) plus the built-in templates (`src/scaffold/templates/*.wcl`). A
  template opts in
  with `import <scaffold.wcl>` and reads answers through an `answer("name")` builtin. See
  the `wcl init` contract below.
- `crates/wcl_lsp` — the `tower-lsp` language server that `wcl lsp` drives.
- `crates/wcl_wdoc` — the library crate behind the `wcl wdoc` subcommands. It renders pages
  to HTML, PDF and Markdown. `build.rs` embeds its WCL stdlib (`lib/*.wcl`) into a
  `wcl_lang::Registry` (`schema_registry()`) and generates the content IR from
  `lib/content.wcl`. A document opts in with `import <wdoc.wcl>`. See the
  [wdoc feature map](#wcl_wdoc-feature-map).
- `crates/wcl_lang/fuzz` — `cargo-fuzz` targets (`parse` / `eval` / `format_round_trip` /
  `json_round_trip` / `set_edit_path`); run one with `just fuzz-run <target>` on nightly.
  See `crates/wcl_lang/fuzz/README.md`.
- `editors/vscode` — a VS Code extension stub that spawns `wcl lsp`
  (`editors/vscode/README.md`).
- `editors/tree-sitter-wcl` — a tree-sitter grammar stub
  (`editors/tree-sitter-wcl/README.md`).
- `examples/` — fixture files the tests use, including `imports/` for module loading,
  `errors/` for negative diagnostics, `warnings/`, `wdoc/`, and `wdoc_relocatable/` (the
  small book the relocatable-output tests build into a nested `--out`).
- `docs/` — wcl.dev, authored in WCL and built with `wcl wdoc`. Two entry documents:
  `landing/main.wcl` (the page at `/`) and `reference/main.wcl` (the 42-chapter book at
  `/reference/`). The book draws the sample media in `docs/assets/` as `../assets/…`.
- `.claude/skills/wcl` — the hand-written skill: a router `SKILL.md` plus the 42
  `references/` files that mirror the book. See [The one rule](#the-one-rule).
- `README.md` — the user-facing quickstart.

## Where the docs live

Prefer these over re-deriving behaviour from source:

- **The reference book** (`docs/reference/`) — the one manual for the language *and* wdoc.
  42 chapters: `main.wcl` plus `pages/intro.wcl`, `pages/language/lang_*.wcl` and
  `pages/wdoc/wdoc_*.wcl`. **A doc change belongs here** (and in its skill mirror). The
  tree is fixed by #156, and the whole skeleton landed at once (#157), because a `toc`
  entry or a link naming an unknown page is a build error. Chapters replaced their stub one
  issue at a time, so a remaining stub is unwritten, not missing — `lang_cli`,
  `wdoc_outputs` and `wdoc_visibility` are still stubs. Browse with `just docs-serve-ref`
  (`:8138`).
- **The skill** (`.claude/skills/wcl/`) — `SKILL.md` routes to one or two
  `references/**.md` files and holds no reference material itself. Each reference stands
  alone: it never sends the reader to a website, to this repo, or to another reference. The
  stubs mirror the book's stubs.
- **The landing page** (`docs/landing/`) — `main.wcl` plus
  `pages/{landing-parts,index}.wcl`: the one-page `website`-template site at `/`, built
  from the `lp_*` components it declares itself. Browse with `just docs-serve` (`:8137`).
  The link to the book is a hand-written `./reference/`, because the two sites are two
  documents and `BuildError::BadLink` cannot check across them.
- **Both sites together** — `just docs-build` wipes `docs/_site/`, renders the landing into
  it, then renders the book into `docs/_site/reference/`. It is a gate part, and it is what
  `.github/workflows/deploy-site.yml` runs step for step: the gate builds what the deploy
  ships.
- **Crate internals** — each crate's `src/lib.rs` carries `//!` module docs. Run
  `cargo doc --open` for the API surface.
- **Topic READMEs** — fuzz (`crates/wcl_lang/fuzz/README.md`), icon packs and licensing
  (`crates/wcl_wdoc/assets/icons/README.md`), editors (`editors/*/README.md`).

## What's implemented

A capability summary per crate. The reference chapters carry the detail.

- **`wcl_lang`** — a hand-written lexer and recursive-descent parser producing a `Source`
  AST plus a `SymbolIndex`. Strings and heredocs with escapes and interpolation (the
  `<<'TAG'` raw form is verbatim). A `Document` view with lazy, cached field evaluation and
  cycle detection. An expression evaluator: literals, member access, calls, fn literals,
  let-bindings, block expressions, arithmetic / comparison / logic with numeric promotion,
  `??` none-coalescing, and `try` / `catch`. `fn name(…)` items (indexed lets). Schema
  constraints: `type Name = TypeRef` aliases plus `@min` / `@max` / `@non_empty`. The
  schema system (`@document` / `@block` / `@child` / `@children` / `@inline` / `@default` /
  `@table` / `@decorator` / `@contextual` / `@declares_kind`), the type system (`type` /
  `interface` / `union` / `symbol_set` / `list` / `tensor` / `fn` / `&T` refs), host
  bindings (`Environment`, `from_fn`, `FromValue` / `IntoValue`), reflection builtins
  (`type_fields`, `decl_info`, `doc_comment`, …), and the builtin library (collections /
  tensors / strings / lists / math / control flow). Imports: quoted disk paths and
  angle-bracket system paths through a `Registry`. The edit path is `parse_for_edit` + AST
  mutation + `format::to_source`. JSON value serialization. → the language chapters
  (`docs/reference/pages/language/`) and `cargo doc`.
- **`wcl` CLI** — `parse` / `check` / `eval`+`get` / `set` / `fmt` / `diff` / `init` /
  `repl` / `lsp` / `wdoc {build,serve,pdf,markdown}`. `diff` is a WCL-aware entity and
  field document diff over the *evaluated* views. It emits a re-parseable WCL tree by
  default, or the flat change array with `--format json`. Either side may be a
  `<rev>:<path>` git specifier, whose imports resolve from that revision; `src/gitspec.rs`
  materialises the tree with `git archive | tar`. `init` scaffolds a project folder from a
  WCL template — a built-in name, a user template under
  `$XDG_DATA_HOME/wcl/templates/<name>/template.wcl`, or a template `.wcl` path or folder.
  Precedence is built-ins ⟶ user templates ⟶ disk path, and `--list` shows the built-ins.
  It collects property answers from `-D key=value`, an `--answers` `.wcl` / `.json` file,
  an interactive prompt, or the property `default`, then writes the `file` / `folder`
  blocks to disk. → the CLI chapter and `wcl --help`.
- **`wcl_lsp`** — diagnostics, formatting, document symbols, workspace symbol search,
  go-to-definition and find-references (cross-file), hover, completion, signature help,
  semantic tokens (including `${…}` slots), schema-violation code actions, incremental sync
  (`ropey`), and a TCP listener. It resolves cross-file lookups through a root document,
  and open buffers shadow disk through an overlay `FileLoader`. → `cargo doc` on `wcl_lsp`.

## Implementation contracts

Non-obvious rules a contributor must respect. They are not in the chapters.

- **`let` items are invisible to the document model.** A `let name = expr` at file or block
  scope is a composition helper. Sibling and descendant expressions resolve it by name. It
  never appears in `Document::fields` / `blocks`, in `get`, in JSON, or in schema
  validation, and the symbol index does not hold it. This is a different thing from the
  expression-level `let … ;` inside a `{ }` block.
- **Bare record literals coerce by shape.** A `{ name: value, … }` literal becomes the
  matching `Value::Variant` when the declared type is a union or a `list<union>`. The work
  happens in `variant_dispatch::coerce_value_to_type`, called from `Field::value`,
  `build_variant` and `invoke_fn_value`. The explicit `Union::Variant { … }` form overrides
  the shape match. No match raises `VariantNoMatch`.
- **`@document` schemas compose per namespace.** The effective document schema for a
  namespace is the *merge* of every `@document` type visible there
  (`Document::doc_schemas_for_ns` → `DocSchemas` in `crates/wcl_lang/src/doc.rs`). A
  top-level field or block is legal if any member declares it, and a root-authored
  declaration wins the type check. Origin comes from `TypeDecl::is_imported`, set from the
  source's import path. `MultipleDocumentSchemas` fires only on a **second root-authored**
  `@document` in a namespace; imported (library) ones merge silently. That is why a user can
  `import <wdoc.wcl>` and still declare a root `@document` to add top-level tags. Both the
  strict path (`schema_errors`) and the lazy path
  (`Field::schema_membership_error` / `declared_type_ref` in `doc/views.rs`) must consult
  the merge.
  **Corollary: gather-field names share the merged space.** A user `@document` field named
  like a wdoc one (`pages`, `components`, `templates`, `themes`, …) resolves ambiguously and
  breaks template iteration in silence — `each = components` fails as "unresolved
  reference", and only at build time. Give a user gather a name of its own.
- **`@declares_kind` makes an *instance* declare a block kind, and the language derives that
  kind's schema.** A `@block` type carrying
  `@declares_kind(name = 0, params = "slots", body = "body")` — wdoc's `WdocComponent`, in
  `crates/wcl_wdoc/lib/components.wcl` — says its instances declare kinds.
  `Document::block_schema` falls back to a schema derived from the declarer's param blocks.
  Legacy untyped params stay `@schemaless utf8`; a typed `slot name: Type` preserves `Type`
  and is checked normally. A type marked `@block_slot` is a nested-block hole, not a scalar
  field; the host scopes and checks its bare fill at build time. Defaults and `?` make
  scalar fields optional, the remaining scalar names go into the derived
  `@block(kind, required_fields = [...])`, and `@contextual` marks the instance because the
  host expands its body. Three consequences to respect:
  1. The derived storage is a lazily-built `OnceLock` arena on `Document`
     (`declared_kinds`), which also serves `kind_declarer`. A `deriving` thread set guards
     it against the re-entrancy of deriving while evaluating.
  2. A derived schema is deliberately **absent from `type_decls()`**. It is not a
     declaration, so anything that introspects by walking declarations will not find it and
     must go through `block_schema` / `derived_block_schema` / `TypeDecl::is_derived`.
  3. The collision check and the derivation itself look kinds up **without** the fallback
     (`find_schema`), or a declared kind collides with itself.
- **A wdoc block declares exactly one rendering, and `@native` is the other half of
  `lower`** (`crates/wcl_wdoc/src/native.rs`). A type that extends a lowering interface
  (`ContentBlock` / `SvgBlock` / `TermPrimitive`, transitively) carries **either** a `lower`
  field **or** `@native`. Both, or neither, fails the build. `native_errors` and
  `reserved_kind_errors` run through the one `build::contract_errors` gate, which
  `build`, `pdf` and `markdown` all call (`serve` reaches it through `build`). `lower` is
  therefore declared *optional* on the interfaces: the check, not the type system, makes it
  exactly one. That is what killed the 57 stub `lower`s that returned `[]` while Rust
  intercepted the kind. `@native(backends = [:html, :pdf, :markdown])` names the targets
  that implement the kind, and a bare `@native` means all three. It is cross-checked **both
  ways** against `NATIVE_DISPATCH`, the one Rust table of natively-dispatched kinds: a
  target claimed but not implemented, or implemented but not claimed, is an error rather
  than the next stub. Rendering a native block on an uncovered target is a build error
  (`refuse_uncovered`), waived per instance with `@except(backends = […])` — capability says
  *can't*, intent says *don't want to*. **Two targets must cover it**, because two are
  involved: the backend actually rendering (each dispatch passes its own — `render_block`
  always passes `Html`, since a `card` body is HTML in whichever target embeds the SVG) and
  the output the build is producing (`patterns.backend()`). `markdown_source` is a renderer
  question and `file` an output one, and a `file` inside a card must not reach a PDF just
  because the card body renders as HTML. In the ordinary case the two are the same backend
  and it is one check. Adding a Rust arm for a kind means adding its registry row **and**
  its `@native` declaration; adding a backend to a kind means widening both.
- **`wcl init` evaluates the template twice** (`src/scaffold/mod.rs`). Pass 1 opens the
  document with an `answer` builtin that returns `none`, and reads only the `property`
  blocks — lazy evaluation never forces the `file` / `folder` bodies. After the answers
  resolve, pass 2 re-opens the document with `answer("name")` bound: a captured-closure
  builtin over the answers map, through `from_fn` + `Environment::add_builtin`. Heredoc
  `${answer(...)}` slots then substitute, so file content **must** use the interpolating
  heredoc form `$<<TAG` — a plain `<<TAG` is literal. Instance fields use `=`
  (`prompt = "…"`), not the `:` of `type` declarations. Answer `.wcl` files are read by
  evaluating their top-level field expressions off the AST (`parse_for_edit` + `eval_expr`),
  which bypasses the strict `@document` membership check a bare `key = value` file would
  otherwise trip. A `file` / `folder` block accepts a `when: bool?` gate, so a template
  generates a part only when an answer asks for it.

## `wcl_wdoc` feature map

**Mechanism:** an unrecognised block kind in a diagram or page dispatches to a WCL
`<kind>_lower` fn returning `list<Content | Svg | Html | TermFundamental>`. The renderer
recurses (depth-limited) until only leaves remain — a fundamental, or a node of the semantic
content IR. That IR node terminates the recursion on every backend, not only in HTML. This
is how a user-declared `@block(...) extends SvgBlock` (and the rest) plugs in. Page-level
blocks that draw SVG (`sequence_diagram` / `state_diagram`) lower to `Content::Drawing`; the
IR carries their typed `Svg` shapes, and `render/svg/standalone.rs` fits the same viewBox for
every backend. A block that cannot lower says so with **`@native`** instead — terminal, card,
node_table, tree, timeline, tilemap, dopesheet, map, wireframe widgets, icons, table, list,
and every structural wrapper — because its output is not expressible in WCL (calendar math,
ANSI grids, external-image crops, measured widget layout, valid nested list HTML). See the
`@native` contract above; there are no stub `lower`s left. `code` and `math` are **not**
native: they lower to `Content::Code` / `Content::Math`, and Rust computes the
backend-specific highlighting and typesetting from that fixed payload. The wireframe `wf_*`
family `extends SvgBlock`, so a widget is a diagram shape — placed by `x` / `y`, connectable
by edges — not a page block. Everything deeper is in the chapter or the source.

The **Chapter** column names a stem that exists twice: `docs/reference/pages/**/<stem>.wcl`
and `.claude/skills/wcl/references/**/<stem>.md`. Update both.

| Feature | Rust | WCL stdlib | Chapter |
|---|---|---|---|
| core / pages / sites (`h1`..`h6` / `p` / `text` lower to `Content::Heading` / `Content::Paragraph`) | `src/render/`, `render/content_html.rs`, `pdf/content.rs`, `markdown/content.rs` | `core.wcl`, `headings.wcl`, `p.wcl`, `text.wcl` | `wdoc_sites` |
| output interfaces and placement (`ContentBlock` → `Content`, `SvgBlock` → `Svg`, `TermPrimitive` → `TermFundamental`. Page / diagram / terminal placement belongs to each slot's `@children(...)` accepts-type, not to a parallel interface hierarchy) | `src/render/lower.rs`, `src/terminal/widgets.rs` | `core.wcl`, `terminal.wcl`, `tui.wcl` | `wdoc_extending` |
| semantic content IR (the closed, target-neutral `Content` union — one variant per document concept, no generic container, no raw-HTML escape. The Rust enum, its supporting records and symbol vocabularies, and their `TryFrom<Value>` are **generated** at build time from the WCL declaration, which walks every type reachable from `union Content`; an unmappable field type fails the build rather than becoming a hole. **Every backend matches it exhaustively** — `render/content_html.rs`, `pdf/content.rs`, `markdown/content.rs`, no catch-all arm anywhere — so a variant added to the union is a compile error in three places rather than silence in three outputs. That IS the mechanism, not a convention. The classification of every stdlib `ContentBlock` into fixed-payload producers and documented natives lives beside the union in `lib/content.wcl`. Two consequences to respect: a `Content::Heading` renders as a real `<hN class="heading-N">` — the class is a style hook DERIVED from the number, and `render/postprocess.rs`'s page-wide anchor and marker pass matches that shape — and a `text`'s `span` children concatenate into one `Content::Paragraph`, so a per-span `id` / `class` no longer renders anywhere. A lowered value is classified by its **union tag** (`content::as_content`), never its variant name, because the IR and the HTML vocabulary both declare `Paragraph`, `Table` and `Math`. The custom-variant recursion runs in all three walkers, so a user block whose `lower` returns another custom variant reaches every backend.) | `build/content_ir.rs` (the emitter, run from `build.rs` → `$OUT_DIR/content_ir.rs`), `src/content.rs` (the hand-written half: `ContentError`, the `Value` readers, `as_content`, and the typed-`Svg` → `Value` bridge a `Drawing` crosses), the `Lowered` seam in `render/lower.rs` (`lower_block`), the three walkers | `content.wcl` | `wdoc_extending` |
| template page metadata (`page_metadata(c)` indexes each shared site TOC once into reading order, giving O(1) page positions, neighbours and active paths. Page-local heading labels, ids and numbers derive from authored handles, and `render/postprocess.rs` stamps the same shared heading sequence into the emitted HTML. The builtin never lowers a page body.) | `src/page_metadata.rs`, `render/{html,postprocess}.rs` | `templates.wcl` | `wdoc_templates` |
| the `el` constructor family (`el(tag, cls, kids)` / `ela(…, attrs, …)` / `eli(…, id, …)` plus the leaves `raw` / `css_style` / `inl` / `icon` / `para`) — each is exactly its `Html` record with the field names dropped. Three element constructors, because a WCL parameter list is fixed at declaration: no defaults, no named arguments, and `?` in a param list is a parse error, so an optional is annotated as required and the `none` flows through. `ela` and `eli` share an arity, so the one positional mistake WCL catches cannot separate them — all three doc copies state that rather than design around it. Scoped to the HTML **element** vocabulary: `Svg` and the content IR keep the named-field literal, because they are field-shaped and WCL checks argument arity but never argument *types*, so transposing two of a shape's interchangeable `f64`s renders silently wrong where the record raises a shape mismatch. The long form stays legal and is the escape hatch for what the family does not name. | — (pure WCL; `tests/el.rs` pins each constructor against its long form) | `el.wcl` | `wdoc_templates` |
| inline patterns / formatting | `src/inline.rs` | `inline.wcl`, `inline-patterns.wcl` | `wdoc_formatting` |
| tables / lists | `src/render/` | `table.wcl`, `list.wcl` | `wdoc_tables` |
| code highlight (a WCL `ContentBlock` lowering to `Content::Code`, not a native or special-cased block. Each backend draws its own chrome — HTML a code-card, Markdown a fence under the filename, PDF a caption over coloured runs — and `highlight.rs` is only the leaf that colours) | `src/highlight.rs`, `render/content_html.rs`, `pdf/content.rs`, `markdown/content.rs` | `code.wcl`, `highlight.wcl` | `wdoc_code` |
| callouts, chapter headers and footnotes (they lower to `Content::Callout` / `Content::ChapterHeader` / `Content::Footnotes`, so the accent, icon, kicker, reading time, updated date, version, section title and note markers all reach every backend. The accent / icon / alert-keyword mapping is each backend's, keyed on the `CalloutKind` symbol — no backend matches a class name. The meta line's separator is the one shared `content::chapter_meta_line`. A footnote's `marker` is its declaration id, the key that `fn-<marker>` / `fnref-<marker>` anchor on, which `render/postprocess.rs`'s `[^id]` rewrite looks for and Markdown emits as a GFM footnote label) | `render/content_html.rs`, `pdf/content.rs`, `markdown/content.rs` | `callout.wcl`, `chapter_header.wcl`, `footnotes.wcl` | `wdoc_callouts` |
| components / repeaters / partials, and addressable bodies (`body` attaches renderable content to a data record as a property; a `project` elsewhere renders it in place) | `src/render/`, `render/expand.rs` | `components.wcl`, `project.wcl` | `wdoc_data_views` |
| `type_table` — schema-reflected property tables, so a documented type cannot drift from its declaration | — (pure WCL over `wcl_lang`'s `type_fields` / `decl_info` reflection builtins) | `typedoc.wcl` | `wdoc_data_views` |
| images, videos and file assets (a `video` renders first as a click-to-play facade that `_wdoc/wdoc-video.js` swaps for the real `<video>` / `<iframe>`; a `file` copies an asset into the output and links it) | `src/image.rs`, `src/video.rs`, `src/file.rs` | `image.wcl`, `video.wcl`, `file.wcl` | `wdoc_media` |
| icons | `src/icons.rs` (+ `build.rs`) | `icons.wcl` | `wdoc_icons` |
| diagrams, auto-layout and routing (layered, force and radial solvers; `text.rs` holds the heuristic text metrics the label renderer and shape sizing share) | `src/{layered,force,radial,routing,text}.rs`, `src/render/svg/` | `diagram-core.wcl`, `flowchart.wcl` | `wdoc_diagrams`, `wdoc_flowcharts`, `wdoc_diagram_connections` |
| sequence and state diagrams (one WCL geometry lowering each, computing the geometry once and returning the page-level `Content::Drawing { shapes }` bridge; there is no second backend-only geometry function) | `src/render/svg/standalone.rs` fits the typed drawing | `sequence.wcl`, `statechart.wcl` | `wdoc_sequence_state` |
| charts | — (pure WCL) | `charts.wcl` | `wdoc_charts` |
| cards, node tables (DB / class, per-row ports) and trees (indented file-tree) | `src/card.rs`, `src/node_table.rs`, `src/tree.rs` | `card.wcl`, `node_table.wcl`, `tree.wcl` | `wdoc_tree` |
| timelines and dopesheets | `src/timeline.rs`, `src/dopesheet.rs` | `timeline.wcl`, `dopesheet.wcl` | `wdoc_timelines` |
| terminals / TUI | `src/terminal/` | `terminal.wcl`, `tui.wcl` | `wdoc_terminals` |
| tilemaps and maps | `src/tileset.rs`, `src/map.rs` | `tilemap.wcl`, `map.wcl` | `wdoc_tilemaps` |
| wireframe | `src/wireframe.rs` | `wireframe.wcl` | `wdoc_wireframe` |
| math (a WCL `ContentBlock` lowering to `Content::Math`, not a native or special-cased block; `math.rs` is the LaTeX-to-SVG leaf the backend readings use) | `src/math.rs`, `render/content_html.rs`, `pdf/content.rs`, `markdown/content.rs` | `math.wcl` | `wdoc_math` |
| themes, styling and book typography (`render/css.rs` lowers the typed CSS vocabulary; `site_theme_css` emits the `--wdoc-*` variables and the per-subtree scoped palettes; the three book faces are embedded and written to `_wdoc/`) | `src/render/{theme,css}.rs` | `theme.wcl`, `theme-rules.wcl`, `css-classes.wcl`, `fonts.wcl` | `wdoc_styling` |
| templates and presentation (`deck` / `section` / `slide` / `fragment` / `notes`) | `src/render/html.rs`, `src/build.rs` | `templates.wcl`, `presentation.wcl` | `wdoc_templates`, `wdoc_presentations` |
| book sidebar footer buttons (`sidebar_footer { button … }` → `TemplateCtx.footer`) | `src/render/html.rs` (`read_sidebar_footer`, `FooterButtonNode`, the `render_template` footer value with its icon resolved through `patterns.icons()`), `src/build.rs` (`footer_nodes`, `footer_missing_page`) | `templates.wcl` (`wdoc_part_sidebar_footer`) | `wdoc_templates` |
| website template (named slots + `<head>` assets) | `src/build.rs` (slot-contract validation and routing, `head_extra`, the `assets` folder copy), `src/render/html.rs` (`Rendered{body,head}`, slot handles, `render_page` head_extra), `src/render/lower.rs` (`Head` / `Blocks` fundamentals) | `website.wcl` | `wdoc_websites` |
| block visibility (`@only` / `@except`; the `backends` axis names three targets — `:html` / `:pdf` / `:markdown`) | `src/visibility.rs`, `Backend` in `src/inline.rs` | `visibility.wcl` | `wdoc_visibility` |
| native blocks and declared backend coverage (`@native` / `@native(backends = […])`, the exactly-one-of check, the registry cross-check, and the uncovered-target build error) | `src/native.rs` | `core.wcl` | `wdoc_extending`, `wdoc_visibility` |
| PDF backend | `src/pdf/` | — | `wdoc_outputs` |
| Markdown backend | `src/markdown/` | — | `wdoc_outputs` |
| markdown_source (preview a page's generated Markdown in a `code markdown` block; HTML-only by construction — it taps the Markdown emitter's `body_to_markdown` seam from inside the HTML build, so another target refuses it rather than rendering nothing) | `src/render/html.rs` (`render_markdown_source`), `src/markdown/emit.rs` (`body_to_markdown`) | `markdown_source.wcl` | `wdoc_extending` |
| demo (a live light/dark preview of its children, then the example source below it. HTML renders the children **twice**, once per pane, flipping the build's UI theme mode each time, because SVG content bakes the resolved palette in Rust and one render cannot adapt to both panes; registry writes are keyed by source, so the second pass is idempotent. `diagram = true` adds `wdoc-preview-diagram`, which centres and fits the previews — the row stays a two-column grid, and only the 48rem media query stacks it, for every demo. Markdown degrades to the source plus one static render. PDF drops the example listing entirely and collects the children in place.) | `src/demo.rs`, dispatched from `src/render/html.rs`, `src/markdown/emit.rs` and `src/pdf/collect.rs`; scoped palettes in `src/render/theme.rs` | `demo.wcl` | `wdoc_demo` |
| reading a document tree at a git revision (`wcl diff`'s `<rev>:<path>` side; `materialize_rev` extracts the whole tree into a temp dir with `git archive | tar`, so imports and relative paths resolve like a real checkout) | `src/git.rs` (+ `crates/wcl/src/gitspec.rs`) | — | `lang_cli` |

Stdlib entry points: `lib/wdoc.wcl` → `lib/prelude.wcl` pulls in every part. The split is
purely for navigability; name resolution is order-independent across imports.

## Intentionally deferred

Deliberate, comment-documented gaps (not bugs), tracked so the list stays honest:

- **Richer value-type introspection in interface checking** —
  `crates/wcl_lang/src/doc/eval.rs` `check_value_implements_iface` structurally introspects
  variant-with-record and bare-record values. Closures, lists, tensors and scalars get a
  pass-through until the language carries runtime type tags for them.
- **Type arguments on `TypeRef::Named` are syntax only** — `content<SvgBlock>` parses
  (`crates/wcl_lang/src/parser/types.rs` `parse_type_args`), prints, and is readable through
  `TypeRef::type_args()`, but nothing checks arity, nothing substitutes, and there is no
  `type Foo<T>` declaration form. A named type resolves by `path` alone, so
  `content<Nonsense>` parses and only fails later, if at all. This is enough for slot
  derivation, which emits both a typed field and a `@children(X)` decorator and lets the
  decorator do the child-kind checking. Full generics are a separate effort.
- **A `text` block's `span` children lose their `id` / `class`** — the IR has no inline-run
  concept, so a `text` concatenates its spans into one `Content::Paragraph`. Outside HTML
  this changed nothing, because the Markdown and PDF walkers already flattened a `text` to
  its child text. In the book, a `span "…" { class = ["accent"] }` stopped painting. The
  fields stay declared so existing documents keep validating; carrying them back needs an
  inline-run node in the IR, which is its own decision.
- **Legacy declared-kind params remain permissive** — an old untyped `wdoc_slot` has no
  value type to preserve, so `derive_kind_schema` emits `@schemaless utf8` for
  compatibility. A new `slot name: Type` scalar param preserves and checks `Type`, and a
  `@block_slot` type is a host-checked nested content hole rather than a derived scalar
  field.
- **`if let` still requires its `else`** — a plain `if` may omit the branch, and the untaken
  side is `none` (`crates/wcl_lang/src/parser/expr.rs` `parse_if_expr`). `if let` does not.
  `eval_if_let` would happily return `none` on a no-match, so this is scope, not a semantic
  obstacle.

New deferred items get tracked alongside the slice that introduces them.

## Verification

A task is **not done** until the merge bar passes:

```bash
just ci::check
```

That is the **`ci` just module** (`.just/ci/mod.just`) — the whole gate. It runs five parts:

```
fmt-check   workspace-lint   workspace-test   docs-build   docs-relative-urls
```

`docs-relative-urls` depends on `docs-build` and greps the sites it just wrote for a
root-absolute `href="/…"` / `src="/…"`. A build tree is relocatable — the book is deployed
under `/reference/`, a directory the build was never told about — and an absolute URL
breaks that. The Rust tests resolve every local target over a fixture book
(`crates/wcl_wdoc/tests/build.rs`, `crates/wcl/tests/wdoc.rs`, sharing the walk in
`crates/wcl_wdoc/tests/support/relocatable.rs`); this part stays a grep, and only exists to
catch an absolute link typed into a real page. Because it depends on `docs-build`,
`.github/workflows/ci.yml` runs the two as one step — separate `just` invocations don't
share a dependency graph, so naming both would render the sites twice.

`just ci::<part>` runs one part, and that is exactly what each step of
`.github/workflows/ci.yml` invokes, so the workflow and the local gate cannot drift. While
iterating, run the parts that fail most on their own:

```bash
just workspace-test    # unit + integration tests across the workspace
just workspace-lint    # clippy --workspace --all-targets -- -D warnings
just fmt-check         # cargo fmt --all -- --check
```

A module cannot see its parent's recipes, so the gate's constituents live in
`.just/shared.just`, imported by **both** the root justfile and the module. Each recipe is
defined once, so `just workspace-test` and `just ci::workspace-test` are the same recipe.
One deliberate exception: `just ci::fuzz-sweep` is a part but is not in `check`. It needs
nightly plus cargo-fuzz, and it runs as its own CI job.

Run benches with `just workspace-bench` when you change the parser hot path.

## Conventions

- Hand-written lexer and recursive-descent parser. No nom, no parser generators.
- Diagnostics use `miette` + `thiserror`. Every parse error carries a `Span` and a
  `NamedSource`, so the CLI can render snippets.
- Keep the dependency list minimal. Add a new crate only when it earns its place.
- **Don't mention `file://` in docs or code comments.** To explain that an asset resolves
  only over a server, say "when served / hosted" and "not opened directly from disk". Never
  write the `file://` scheme.
- Documentation changes obey [The one rule](#the-one-rule): the book chapter and its skill
  reference move together.
