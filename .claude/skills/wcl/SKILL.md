---
name: wcl
description: >-
  Read, write or debug a `.wcl` file, a wdoc page or site, or a `wcl` CLI
  invocation. Covers WCL syntax, types, schemas, decorators and connections;
  wdoc's block vocabulary for documents, diagrams, charts and decks; every
  builtin; every CLI flag and exit code; and declaring new block kinds. Use
  before guessing a builtin name or a flag — both are closed lists.
---

# WCL and wdoc

<overview>
**WCL** is a typed configuration and schema language. A `.wcl` file carries the data and the
schema that describes it. The schema gives you types with widths and encodings, unions,
interfaces, and decorators that declare which blocks and fields are legal. A field holds an
expression or a function, not only a literal. A document model gathers nested blocks into
typed lists. `wcl check` validates a file against its own schema. `wcl get` resolves a dotted
path out of it.

**wdoc** is a document generator. Its vocabulary is ordinary WCL blocks. A document writes
`import <wdoc.wcl>` and gains `site`, `page`, `h1`, `code`, `table`, `diagram` and the rest.
`wcl wdoc build` renders it to a static website. `wcl wdoc build --type markdown` renders it to a folder of
`.md` files, and `wcl wdoc build --type pdf` to a paginated PDF. wdoc is not a second language — it is a
schema written in WCL, and `wdoc` is a subcommand of the `wcl` CLI.
</overview>

<variables>
- `${CLAUDE_SKILL_DIR}`: Path to this skill's directory.
- `LANGUAGE`: `${CLAUDE_SKILL_DIR}/references/language/` — the WCL language chapters, listed below.
- `WDOC`: `${CLAUDE_SKILL_DIR}/references/wdoc/` — the wdoc block chapters, listed below.
</variables>

## How to use this skill

**This file is a router. It holds no reference material.** Find the one or two entries below
that match the question. Open those files. Read nothing else. Each reference carries its own
content: it will not send you to a sibling to understand the chapter you are on. Where one does
point outward — `cargo doc` for exact Rust signatures, an upstream index for icon names — it is
naming a lookup that would go stale if copied in, and the chapter still answers without it.

Read [`references/intro.md`](references/intro.md) first if you have not used WCL before. It
covers what WCL is, how it differs from JSON and YAML, and the install. It then walks a first
document through `wcl check`, `wcl get` and `wcl wdoc build`.

## The language — `LANGUAGE`

- [`lang_documents.md`](references/language/lang_documents.md) — a `.wcl` file as a document: fields, blocks, labels, nesting, tables, `self` and `parent`, `let` items.
- [`lang_values.md`](references/language/lang_values.md) — numbers and their suffixes, literal units, booleans, symbols, strings, interpolation, heredocs, `none`.
- [`lang_collections.md`](references/language/lang_collections.md) — list and tensor literals, record literals, field access, bare-record coercion to a union variant.
- [`lang_types.md`](references/language/lang_types.md) — type aliases, unions, interfaces, optionals, symbol sets, reference types, the type constraints.
- [`lang_expressions.md`](references/language/lang_expressions.md) — member access, indexing, calls, operator precedence, numeric promotion, the `??` operator.
- [`lang_control_flow.md`](references/language/lang_control_flow.md) — `if` / `else`, `match`, `if let`, block expressions, `let` bindings, `try` / `catch`.
- [`lang_functions.md`](references/language/lang_functions.md) — function literals, `fn` items, function types, higher-order functions, closures and capture.
- [`lang_namespaces.md`](references/language/lang_namespaces.md) — `namespace` and `use`, qualified kind names, disk and system imports, how they resolve.
- [`lang_schemas.md`](references/language/lang_schemas.md) — the schema decorators that declare a document shape, and how document schemas compose per namespace.
- [`lang_decorators.md`](references/language/lang_decorators.md) — what a decorator is, the authored and host-declared vocabularies, `@declares_kind`, reading decorators back.
- [`lang_connections.md`](references/language/lang_connections.md) — `connection` declarations, connection statements, `@connections`, reference integrity.
- [`lang_evaluation.md`](references/language/lang_evaluation.md) — the evaluate-and-edit split, lazy fields, cycle detection, JSON serialization, the error model.
- [`lang_builtins.md`](references/language/lang_builtins.md) — every builtin by category, with its signature, parameters, return value and an example.
- [`lang_cli.md`](references/language/lang_cli.md) — every `wcl` subcommand, its flags and its exit codes.

## wdoc — `WDOC`

Documents and presentation:

- [`wdoc_sites.md`](references/wdoc/wdoc_sites.md) — the entry document, the `site` block, `page` blocks, `toc` and `menu`, the output tree.
- [`wdoc_templates.md`](references/wdoc/wdoc_templates.md) — the three template kinds, the template context, the template parts, the `el` constructor family.
- [`wdoc_websites.md`](references/wdoc/wdoc_websites.md) — the website template, named slots and the slot contract, head assets, the assets folder.
- [`wdoc_presentations.md`](references/wdoc/wdoc_presentations.md) — `deck`, `section`, `slide`, `fragment` and `notes`, and how a deck builds on each target.
- [`wdoc_styling.md`](references/wdoc/wdoc_styling.md) — `theme` and `palette`, the built-in themes, the style rule vocabulary, scoped palettes.
- [`wdoc_visibility.md`](references/wdoc/wdoc_visibility.md) — `@only` and `@except`, the sites, templates and backends axes, and waiving a native block on an uncovered target.
- [`wdoc_outputs.md`](references/wdoc/wdoc_outputs.md) — the build, serve, PDF and Markdown targets, and what each backend can and cannot render.

Content blocks:

- [`wdoc_formatting.md`](references/wdoc/wdoc_formatting.md) — headings, `p`, `text` and `span`, `column`, the inline pattern vocabulary.
- [`wdoc_code.md`](references/wdoc/wdoc_code.md) — the `code` block, its language, filename and caption, and the chrome each backend draws.
- [`wdoc_callouts.md`](references/wdoc/wdoc_callouts.md) — the `callout` kinds, `footnote` and `footnotes`, the `chapter_header` meta line.
- [`wdoc_tables.md`](references/wdoc/wdoc_tables.md) — `list` and `li`, `table` in both authored forms, and the CSS workarounds standing in for the absent alignment and caption fields.
- [`wdoc_media.md`](references/wdoc/wdoc_media.md) — `image` sources and sizing, why there is no crop field, the `video` block, the `file` block, path resolution.
- [`wdoc_icons.md`](references/wdoc/wdoc_icons.md) — the `icon` block, the bundled packs and their licensing, custom icon sets, and how to find an icon name.
- [`wdoc_math.md`](references/wdoc/wdoc_math.md) — the `math` block, inline and display equations, the supported LaTeX subset.
- [`wdoc_data_views.md`](references/wdoc/wdoc_data_views.md) — components and slots, repeaters, partials, the `body` and `project` pair, schema-reflected tables.
- [`wdoc_demo.md`](references/wdoc/wdoc_demo.md) — the `demo` block: source and live preview side by side, and how it degrades.

Diagrams and drawn blocks:

- [`wdoc_diagrams.md`](references/wdoc/wdoc_diagrams.md) — the `diagram` block, the primitive and composite shapes, shape styling, the layout modes.
- [`wdoc_diagram_connections.md`](references/wdoc/wdoc_diagram_connections.md) — edges between shapes, ports and anchors, the routing modes, arrowheads, edge labels.
- [`wdoc_flowcharts.md`](references/wdoc/wdoc_flowcharts.md) — the flowchart node kinds, the layered auto-layout, composing swim-lanes from primitives.
- [`wdoc_sequence_state.md`](references/wdoc/wdoc_sequence_state.md) — `sequence_diagram` and `state_diagram`, and the one geometry lowering that serves every backend.
- [`wdoc_charts.md`](references/wdoc/wdoc_charts.md) — the bar, line and pie charts: series shapes, axes, labels, legends, colour.
- [`wdoc_timelines.md`](references/wdoc/wdoc_timelines.md) — `timeline` calendar math, ranges and milestones, and `dopesheet` tracks, keys and frames.
- [`wdoc_tree.md`](references/wdoc/wdoc_tree.md) — `tree` and `tree_node`, `node_table` and `node_row`, the `card` shape.
- [`wdoc_tilemaps.md`](references/wdoc/wdoc_tilemaps.md) — `tileset`, `tile` and `tilemap` sprite grids, and the `map` block with its layers and pins.
- [`wdoc_terminals.md`](references/wdoc/wdoc_terminals.md) — `terminal` and `term_text` grids, the terminal primitive interface, the TUI widget family.
- [`wdoc_wireframe.md`](references/wdoc/wdoc_wireframe.md) — the wireframe widget family in four groups, and why a widget is a diagram shape rather than a page block.

Extending:

- [`wdoc_extending.md`](references/wdoc/wdoc_extending.md) — the lowering mechanism, the three lowering interfaces, the semantic content IR, native blocks.

## Routing hints

- A syntax or type question about a `.wcl` file → `LANGUAGE`, whatever the file is for.
- A question about what a page renders as → `WDOC`.
- "Which builtin does X?" → `lang_builtins.md`.
- "Which flag does X?" → `lang_cli.md`.
- A block that draws → the diagram group, not `wdoc_formatting.md`.
- Declaring a **new** block kind of your own → `wdoc_extending.md`, then `lang_schemas.md`.

<boundaries>
<always>
- Give every top-level field a schema. A value with no `@document` type in scope is an error, not a default
- Run `wcl check` on a `.wcl` file you wrote, and `wcl wdoc build` on a page, before reporting it done
- Name the output target when a wdoc block behaves differently across html, markdown and pdf — `wdoc_outputs.md` says which do
</always>

<ask>
- Which output targets a wdoc document must render on, when the request names a block that not every backend draws
- Whether a new block kind is wanted, before writing a lowering: composing the existing vocabulary is usually the smaller change
</ask>

<never>
- Guess a builtin, a decorator, a block kind or a CLI flag. Each is a closed list, and the reference for it names every member
- Reach for a JSON, YAML or Markdown idiom on the assumption WCL has an equivalent
- Keep reading reference files once the question is answered
- Edit `docs/reference/` to match this skill, or this skill to match `docs/reference/`. The two trees are independent and both answer to `crates/`
</never>
</boundaries>
