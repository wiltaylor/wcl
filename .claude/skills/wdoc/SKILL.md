---
name: wdoc
description: "Reference and processes for wdoc. WCL's static-site and skill generator: declare pages and sites in WCL and render them to HTML, Markdown, a Claude skill folder, or PDF. Use when working with wdoc or answering questions about it."
allowed-tools: []
disable-model-invocation: false
metadata:
  wskill_schema_version: 1.3.0
---

# wdoc

<overview>

WCL's static-site and skill generator: declare pages and sites in WCL and render them to HTML, Markdown, a Claude skill folder, or PDF.

**Upstream version:** `0.24.1-alpha`. If the real upstream has moved past this, the skill may be stale — bump `topic.version` and re-verify (see the update workflow).

wdoc is WCL's static-site and skill generator. This skill captures its full reference as data — every block family, template, and render target — projected from one model.

</overview>

## Parameters

<variables>

- `${CLAUDE_SKILL_DIR}`: path to this skill's directory (its `scripts/`, `assets/`, and `references/` live here).

- `$ARGUMENTS`: The wdoc block or render concept to look up. How to determine: Take it from the user's request — e.g. the block kind, template, or render target they asked about. If empty, summarise the reference and ask what they need.

</variables>

<boundaries>

<always>

- Cite the exact reference page (a fact/concept page) when answering.
- Prefer the documented block fields over guesses.

</always>

<ask>

- Before running any command that edits files.

</ask>

<never>

- Invent blocks, fields, or template kinds that aren't in the reference — check the fact pages instead.

</never>

</boundaries>

## Reference

### Documents & Content

_The document model (pages, sites, render targets) and the content blocks that fill a page._

#### The document model

_How a document is structured and rendered._
- [wdoc Overview](references/concept_overview.md)
- [Sites](references/concept_sites.md)
- [Templates](references/concept_templates.md)
- [Pages](references/concept_pages.md)
- [Including sub-sites](references/concept_includes.md)
- [Block Visibility](references/concept_visibility.md)
- [Markdown output](references/concept_markdown.md)
- [Skill folders](references/concept_skills.md)
- [Built-in site templates](references/fact_template_kinds.md)
- [Built-in colour themes](references/fact_themes.md)

#### Content blocks

_What fills a page: prose, tables, lists, callouts, media._
- [Formatting](references/concept_formatting.md)
- [Columns](references/concept_columns.md)
- [table](references/fact_tables_block.md)
- [list / li](references/fact_lists_block.md)
- [callout](references/fact_callouts.md)
- [math](references/fact_math.md)
- [iconset / icon_def / icon](references/fact_icons.md)
- [image](references/fact_images.md)
- [video](references/fact_videos.md)

#### Data & styling

- [Data Views](references/concept_data_views.md)
- [Styling](references/concept_styling.md)

### Diagrams

_The diagram family: the SVG canvas, the auto-layout modes, and every higher-level shape — graphs, charts, maps, sprites, terminals and wireframes._

#### Canvas & layout

_The drawing surface, its primitive shapes, connections, and auto-layout._
- [diagram](references/fact_diagrams.md)
- [primitive shapes](references/fact_primitive_shapes.md)
- [composite shapes](references/fact_composite_shapes.md)
- [styling shapes with classes](references/fact_shape_styling.md)
- [Connections](references/concept_connections.md)
- [Diagram and container layout modes](references/fact_layout_modes.md)

#### Graphs & flow

_Turn a connection graph into a ranked, routed picture._
- [flowchart shapes](references/fact_flowcharts.md)
- [swim-lane flowcharts](references/fact_swimlanes.md)
- [sequence_diagram](references/fact_sequence_diagrams.md)
- [state_diagram](references/fact_state_diagrams.md)
- [tree](references/fact_tree.md)

#### Data visualisation

_Plot values directly from WCL data._
- [charts](references/fact_charts.md)
- [timeline](references/fact_timelines.md)

#### Grids & sprites

_Tiled, animated, or pinned content._
- [tilemaps](references/fact_tilemaps.md)
- [dopesheet](references/fact_dopesheets.md)
- [map](references/fact_maps.md)

#### Terminal & UI

_ANSI terminal grids and wireframe UI mockups._
- [terminal](references/fact_terminals.md)
- [Wireframes](references/fact_wireframe.md)

### Task runbooks

_Step-by-step procedures for rendering and reviewing wdoc output._
- [Render a site and live-preview it](references/process_build_serve.md)
- [Render a document into a Claude skill folder](references/process_render_skill.md)
- [Render a document to PDF or Markdown](references/process_render_pdf_markdown.md)
- [Render a slide deck](references/process_render_presentation.md)
- [Review a site with comments](references/process_review_comments.md)

- [Related skills](references/related_ref.md) — cross-references to other wskills

## Views

Beyond this skill, the wskill ships these views — build them with `just render` in the wskill folder:

- **book** (`wdoc/book/main.wcl`)
- **ai skill** (`wdoc/skill/main.wcl`)
- **presentation** — A wdoc tour — an overview deck. (`wdoc/presentation/main.wcl`)
- **training** — wdoc authoring tutorial — a hands-on lesson series. (`wdoc/training/main.wcl`)
