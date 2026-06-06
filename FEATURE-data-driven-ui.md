# Feature request: data-driven UI composition for wdoc (components → screens)

## Motivation

In the `wad` project, a storefront **design system** is authored as data
(`schema/design.wcl` + `data/design.wcl`) and rendered by `wdoc/pages.wcl`. We want one
**data-driven** model where:

1. A **UI component** is defined once as data (a tree of UI elements with props,
   coloured from the palette by token).
2. The same component data renders **inline** in the design-system page (the Components
   gallery) **and** is **composed into screens**.
3. A **screen** is a data block that picks a frame type (web / window / phone / tablet)
   and composes components/elements; the book **generates one page per screen block**.

Today this is impossible: a screen/component body can only be **hand-authored wdoc
blocks**, never built from data. We end up duplicating layouts and can't reference a
component from a screen.

## Desired authoring experience (target)

```wcl
ui_component "product_card" {
  panel {
    label  { text = "Wireless Headphones"  style = "heading" }
    label  { text = "$129"                 color = "brand_primary" }
    button { text = "Add to cart"          variant = :primary }
  }
}

screen "product_listing" {
  frame = :web                         # wf_browser
  url   = "shop.example.com/headphones"
  grid { columns = 3
    use "product_card"                 # reference the component by id
    use "product_card"
    use "product_card"
  }
}
```

…and the wdoc layer renders `product_card` in the Components gallery, renders each
`screen` into its frame, and emits one page per `screen` — all from the data above.

## What blocks this today (evidence)

1. **`@children` can't be value/expression-populated.** Container/frame widgets declare
   `@children(Widget)` but it is filled **only by syntactically-nested blocks** — the
   Rust renderer walks `block.blocks()`, and the field is "purely for schema validation"
   (`crates/wcl_wdoc/lib/wireframe.wcl:1-15`; `crates/wcl_wdoc/src/wireframe.rs`
   `child_widgets`). You cannot do `wf_browser { children = screen.body }`.

2. **`wdoc_repeater` can't re-emit heterogeneous block values.** It expands a **fixed
   template** per scalar element of `each` (`crates/wcl_wdoc/src/render/expand.rs`). It
   can't take a `list<UiNode>` and render each node as its appropriate (different) widget
   block.

3. **`lower` emits only leaf primitives.** A custom block's `lower` returns
   `list<SvgFundamental>` (Rect/Circle/Line/Label/Polygon —
   `crates/wcl_wdoc/lib/diagram-core.wcl:455`), recursed until only fundamentals remain.
   It **cannot return higher-level blocks** (a `wf_panel` containing a `wf_button`), so a
   data block can't programmatically expand into a composed widget tree without
   re-implementing all wireframe rendering as raw SVG.

4. **No render-by-reference.** `wdoc_component` instances are **authored**, selected by
   writing the component's name as a block — there is no "render the component whose id
   is `node.ref`" driven by a data value.

5. **Only page content renders.** Data blocks are inert unless a `page` pulls their
   fields into expressions; document-root repeaters can emit one **page** per data
   element, but each page body is still a fixed template (so per-screen unique bodies
   can't be generated) (`crates/wcl_wdoc/lib/core.wcl`,
   `crates/wcl_wdoc/lib/components.wcl`).

Net: WCL has the data side (unions, records, lists, `fn`, pattern-matching) and a
lowering pipeline, but **lowering bottoms out at SVG primitives and there is no way to
turn a data tree into a tree of higher-level blocks**, nor to assign/reference children
dynamically.

## Proposed features (any one of A–C unblocks it; they can combine)

### A. Block-producing expansion (`expand`) — most general

Let a custom `@block` declare `expand = fn(self) -> list<Block>` (or `-> list<SvgBlock>`
/ `list<WdocBlock>`) that returns **higher-level blocks** computed from its data; the
renderer splices the result in place of the block and lowers it recursively (exactly how
`lower` works today, but the return type is blocks, not only `SvgFundamental`). Then a
`ui_component` / `screen` block expands into a `wf_browser { wf_panel { … } }` tree built
from its data. Generalises the existing lowering pipeline; the recursion already exists.

### B. Value-assignable `@children`

Allow `children = <list-of-block-values>` on container/frame blocks, treated identically
to nested blocks. Requires **first-class block values** (constructing `wf_button{…}` as a
value and putting it in a `list`). Larger language change, but composable with functions.

### C. `wdoc_render(node)` + a UI-node union — most targeted

A built-in (or stdlib component) that interprets a tagged **UI-node** data union and
renders each node as the matching widget block, **recursively**, resolving component
refs and palette tokens. Smallest blast radius; purpose-built for UI/screens. Sketch:

```wcl
union UiNode {
  Panel  { title: utf8?  children: list<UiNode> }
  Row    { children: list<UiNode> }
  Grid   { columns: i64  children: list<UiNode> }
  Button { text: utf8  variant: symbol  icon: utf8? }
  Input  { label: utf8  value: utf8? }
  Label  { text: utf8  style: utf8?  color: utf8? }   # type/colour tokens
  Use    { component: utf8 }                           # render ui_component by id
}
# wdoc_render(nodes, ctx) walks the tree → wf_* widgets, maps tokens → palette hex,
# resolves Use → the named component's nodes.
```

### D. Render-by-reference + per-block page generator (needed by C's `Use`)

A way to render a `wdoc_component` chosen by a data value
(`wdoc_instance { component = node.ref }`), and a **document-root generator** that emits
one page per screen block whose body is `wdoc_render(screen.body)` rather than a fixed
template.

## Recommendation

Lead with **C (UI-node union + `wdoc_render`) built on A (block-producing `expand`)**:
A gives the engine (data → block tree, recursively), C gives the ergonomic, typed UI
vocabulary and component-by-id reuse. This makes both the design-system gallery and the
screens render from the same component data, and lets screen pages be generated per
block. B (first-class block values) is a nice long-term general capability but heavier
and not required.

## Impact on `wad` once available

- `ui_component.elements` → `content: list<UiNode>` (a real tree, not a flat list).
- New `screen` block: `frame: symbol`, `url/title`, `body: list<UiNode>` (with `Use`
  refs to components).
- `wdoc/pages.wcl`: the Components gallery calls `wdoc_render(component.content)`; a
  document-root generator emits `page screen_<id>` rendering the chosen frame +
  `wdoc_render(screen.body)`. Deletes the per-mode authored duplication.

## Acceptance criteria (examples the feature must satisfy)

1. A `ui_component` defined purely as data renders in the design-system page with no
   hand-authored per-component layout.
2. A `screen` block referencing components by id renders inside its chosen frame, and a
   page is generated per screen block (route `screen_<id>`), listed in the TOC.
3. Colours resolve from palette tokens; light/dark both work from one definition.
4. Adding a screen or component is **data-only** (edit `data/design.wcl`), no edits to
   `wdoc/*.wcl`.

## Interim (until the feature lands)

Screens stay **hand-authored** wdoc mockups: a `screens` page with a section per screen,
each a device frame composing `wf_*` widgets in both light and dark modes. This is the
workaround, not the goal.
