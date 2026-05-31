# Primitive Shapes

Primitive shapes are what everything is broken down into for rendering. Higher-level blocks (flowchart nodes, charts, cards, …) all lower to these, so targeting a new backend only means re-implementing the primitives.

Every shape shares a few common fields, summarised here; the per-shape tables below reflect the full field set of each shape:

| Shared field | Meaning |
| --- | --- |
| `id` | Name used to connect the shape (`a -> b`) and to anchor others to it. |
| `class` | Style classes — text and SVG paint (`fill`, `stroke`, …) via the `class` system. |
| `anchor_left` / `anchor_right` / `anchor_top` / `anchor_bottom` | Fractional anchors (0–1) that pin an edge of the shape to the parent box. |
| `connect_points` | Which sides (`:left`/`:right`/`:top`/`:bottom`) edges attach to. |

The box-like shapes — `rect`, `circle`, and `container` — additionally accept an icon badge: `icon` (a `pack.name`), `icon_size`, `icon_pos` (`:center` / `:top_left` / …), and `icon_class`.

## rect

![diagram](../_wdoc/wdoc_primitives-diagram-1.svg)

An axis-aligned rectangle — the workhorse box.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | `f64` | no | Top-left x corner, in canvas pixels. |
| `y` | `f64` | no | Top-left y corner, in canvas pixels. |
| `width` | `f64` | no | Box width. |
| `height` | `f64` | no | Box height. |
| `fill` | `utf8` | no | Fill colour. |
| `stroke` | `utf8` | no | Outline colour. |
| `id` | `identifier` | no | Name used to connect the shape (`a -> b`) and to anchor others to it. |
| `class` | `list<utf8>` | no | Style classes — text and SVG paint via the `class` system. |
| `anchor_left` | `f64` | no | Fractional anchor (0–1) pinning the left edge to the parent box. |
| `anchor_right` | `f64` | no | Fractional anchor (0–1) pinning the right edge to the parent box. |
| `anchor_top` | `f64` | no | Fractional anchor (0–1) pinning the top edge to the parent box. |
| `anchor_bottom` | `f64` | no | Fractional anchor (0–1) pinning the bottom edge to the parent box. |
| `connect_points` | `list<AnchorSide>` | no | Which sides (`:left`/`:right`/`:top`/`:bottom`) edges attach to. |
| `icon` | `utf8` | no | Icon-badge icon (a `pack.name`). |
| `icon_size` | `f64` | no | Icon-badge size. |
| `icon_pos` | `IconPos` | no | Icon-badge position (`:center` / `:top_left` / …). |
| `icon_class` | `list<utf8>` | no | Icon-badge style classes. |

```wcl
diagram { width = 200  height = 90
  rect { x = 20.0  y = 15.0  width = 120.0  height = 60.0  fill = "#cce"  stroke = "#333" }
}
```

## circle

![diagram](../_wdoc/wdoc_primitives-diagram-2.svg)

A circle, positioned by its centre.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `cx` | `f64` | no | Centre x point. |
| `cy` | `f64` | no | Centre y point. |
| `r` | `f64` | no | Radius. |
| `fill` | `utf8` | no | Fill colour. |
| `stroke` | `utf8` | no | Outline colour. |
| `id` | `identifier` | no | Name used to connect the shape (`a -> b`) and to anchor others to it. |
| `class` | `list<utf8>` | no | Style classes — text and SVG paint via the `class` system. |
| `anchor_left` | `f64` | no | Fractional anchor (0–1) pinning the left edge to the parent box. |
| `anchor_right` | `f64` | no | Fractional anchor (0–1) pinning the right edge to the parent box. |
| `anchor_top` | `f64` | no | Fractional anchor (0–1) pinning the top edge to the parent box. |
| `anchor_bottom` | `f64` | no | Fractional anchor (0–1) pinning the bottom edge to the parent box. |
| `connect_points` | `list<AnchorSide>` | no | Which sides (`:left`/`:right`/`:top`/`:bottom`) edges attach to. |
| `icon` | `utf8` | no | Icon-badge icon (a `pack.name`). |
| `icon_size` | `f64` | no | Icon-badge size. |
| `icon_pos` | `IconPos` | no | Icon-badge position (`:center` / `:top_left` / …). |
| `icon_class` | `list<utf8>` | no | Icon-badge style classes. |

```wcl
diagram { width = 200  height = 100
  circle { cx = 100.0  cy = 50.0  r = 36.0  fill = "#8fbcbb"  stroke = "#333" }
}
```

## line

![diagram](../_wdoc/wdoc_primitives-diagram-3.svg)

A straight line segment between two points.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `x1` | `f64` | no | Start-point x. |
| `y1` | `f64` | no | Start-point y. |
| `x2` | `f64` | no | End-point x. |
| `y2` | `f64` | no | End-point y. |
| `stroke` | `utf8` | no | Line colour. |
| `id` | `identifier` | no | Name used to connect the shape (`a -> b`) and to anchor others to it. |
| `class` | `list<utf8>` | no | Style classes — text and SVG paint via the `class` system. |
| `anchor_left` | `f64` | no | Fractional anchor (0–1) pinning the left edge to the parent box. |
| `anchor_right` | `f64` | no | Fractional anchor (0–1) pinning the right edge to the parent box. |
| `anchor_top` | `f64` | no | Fractional anchor (0–1) pinning the top edge to the parent box. |
| `anchor_bottom` | `f64` | no | Fractional anchor (0–1) pinning the bottom edge to the parent box. |
| `connect_points` | `list<AnchorSide>` | no | Which sides (`:left`/`:right`/`:top`/`:bottom`) edges attach to. |

```wcl
diagram { width = 200  height = 80
  line { x1 = 20.0  y1 = 60.0  x2 = 180.0  y2 = 20.0  stroke = "#bf616a" }
}
```

## label

![diagram](../_wdoc/wdoc_primitives-diagram-4.svg)

An SVG text label. Named `label` (not `text`) to avoid clashing with the paragraph block.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `content` | `utf8` | yes | The text, given as the inline label: `label "halfway" { … }`. |
| `x` | `f64` | no | Anchor x position. |
| `y` | `f64` | no | Anchor y position. |
| `font_size` | `f64` | no | Explicit font size; when omitted the text auto-fits its region. |
| `fit_width` | `f64` | no | Width of the region the text is auto-sized to fit (when `font_size` is unset). |
| `fit_height` | `f64` | no | Height of the region the text is auto-sized to fit (when `font_size` is unset). |
| `fill` | `utf8` | no | Text colour. |
| `id` | `identifier` | no | Name used to connect the shape (`a -> b`) and to anchor others to it. |
| `class` | `list<utf8>` | no | Style classes — text and SVG paint via the `class` system. |
| `anchor_left` | `f64` | no | Fractional anchor (0–1) pinning the left edge to the parent box. |
| `anchor_right` | `f64` | no | Fractional anchor (0–1) pinning the right edge to the parent box. |
| `anchor_top` | `f64` | no | Fractional anchor (0–1) pinning the top edge to the parent box. |
| `anchor_bottom` | `f64` | no | Fractional anchor (0–1) pinning the bottom edge to the parent box. |
| `connect_points` | `list<AnchorSide>` | no | Which sides (`:left`/`:right`/`:top`/`:bottom`) edges attach to. |

```wcl
diagram { width = 200  height = 60
  label "Hello" { x = 100.0  y = 36.0  font_size = 22.0  fill = "#5e81ac" }
}
```

## polygon

![diagram](../_wdoc/wdoc_primitives-diagram-5.svg)

An arbitrary closed shape from a list of points.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `points` | `utf8` | yes | Space-separated `x,y` pairs, e.g. `"180,10 230,40 180,70"`. |
| `fill` | `utf8` | no | Fill colour. |
| `stroke` | `utf8` | no | Outline colour. |
| `id` | `identifier` | no | Name used to connect the shape (`a -> b`) and to anchor others to it. |
| `class` | `list<utf8>` | no | Style classes — text and SVG paint via the `class` system. |
| `anchor_left` | `f64` | no | Fractional anchor (0–1) pinning the left edge to the parent box. |
| `anchor_right` | `f64` | no | Fractional anchor (0–1) pinning the right edge to the parent box. |
| `anchor_top` | `f64` | no | Fractional anchor (0–1) pinning the top edge to the parent box. |
| `anchor_bottom` | `f64` | no | Fractional anchor (0–1) pinning the bottom edge to the parent box. |
| `connect_points` | `list<AnchorSide>` | no | Which sides (`:left`/`:right`/`:top`/`:bottom`) edges attach to. |

```wcl
diagram { width = 200  height = 100
  polygon { points = "40,80 100,15 160,80"  fill = "#ebcb8b"  stroke = "#333" }
}
```

## container

![diagram](../_wdoc/wdoc_primitives-diagram-6.svg)

A grouping box that holds and lays out child shapes (`@children`). Optional chrome makes the group visible; a `layout` arranges the children automatically.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `id` | `identifier` | no | Name used to connect the shape (`a -> b`) and to anchor others to it. |
| `class` | `list<utf8>` | no | Style classes — text and SVG paint via the `class` system. |
| `stroke` | `utf8` | no | Optional chrome — outline colour of the background rect that makes the group visible. |
| `fill` | `utf8` | no | Optional chrome — fill colour of the background rect that makes the group visible. |
| `padding` | `f64` | no | Inset between the chrome and the child shapes. |
| `width` | `f64` | no | Declared interior width (when no layout/anchor sizes it). |
| `height` | `f64` | no | Declared interior height (when no layout/anchor sizes it). |
| `layout` | `symbol` | no | Layout mode: `:free` (default, manual) / `:grid` / `:layered` / `:force`. |
| `columns` | `i64` | no | Number of columns for `:grid` layout. |
| `cell_width` | `f64` | no | Grid cell width for `:grid` layout. |
| `cell_height` | `f64` | no | Grid cell height for `:grid` layout. |
| `gap` | `f64` | no | Gap between cells in `:grid` layout. |
| `direction` | `symbol` | no | Flow direction for `:layered`: `:top_to_bottom` (default) / `:left_to_right`. |
| `layer_gap` | `f64` | no | Spacing between ranks (layers) in `:layered` layout. |
| `node_gap` | `f64` | no | Spacing between nodes within a rank in `:layered` layout. |
| `iterations` | `i64` | no | `:force` relaxation steps (default 300). |
| `repulsion` | `f64` | no | `:force` node repulsion strength (default 9000). |
| `link_distance` | `f64` | no | `:force` ideal edge-to-edge length (default 60). |
| `gravity` | `f64` | no | `:force` centering pull (default 0.05). |
| `seed` | `i64` | no | `:force` random seed for reproducible layouts (default 1). |
| `anchor_left` | `f64` | no | Fractional anchor (0–1) pinning the left edge to the parent box. |
| `anchor_right` | `f64` | no | Fractional anchor (0–1) pinning the right edge to the parent box. |
| `anchor_top` | `f64` | no | Fractional anchor (0–1) pinning the top edge to the parent box. |
| `anchor_bottom` | `f64` | no | Fractional anchor (0–1) pinning the bottom edge to the parent box. |
| `connect_points` | `list<AnchorSide>` | no | Which sides (`:left`/`:right`/`:top`/`:bottom`) edges attach to. |
| `icon` | `utf8` | no | Icon-badge icon (a `pack.name`). |
| `icon_size` | `f64` | no | Icon-badge size. |
| `icon_pos` | `IconPos` | no | Icon-badge position (`:center` / `:top_left` / …). |
| `icon_class` | `list<utf8>` | no | Icon-badge style classes. |
| `edges` | `list<Edge>` | yes | Edges connecting child shapes (`a -> b`). |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `children` | `SvgBlock` | yes | The child shapes laid out by the container. |

```wcl
diagram { width = 240  height = 130
  container {
    anchor_left = 10.0  anchor_top = 10.0
    fill = "#eef"  stroke = "#88a"  padding = 10.0
    layout = :grid  columns = 2  cell_width = 90.0  cell_height = 44.0  gap = 10.0
    rect { fill = "#88c0d0" }
    rect { fill = "#a3be8c" }
    rect { fill = "#ebcb8b" }
    rect { fill = "#b48ead" }
  }
}
```

## image

![diagram](../_wdoc/wdoc_primitives-diagram-7.svg)

A raster image placed as an SVG `<image>`. The source (the inline label) is a doc-relative path — copied into `_wdoc/` — or a URL / `data:` URI passed through unchanged. (The preview above uses an inline `data:` URI so it renders here.)

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `source` | `utf8` | yes | Image source (the inline label): a doc-relative path, a URL, or a `data:` URI. |
| `alt` | `utf8` | no | Alt text for the `<img>` (page form). |
| `width` | `f64` | no | Display width; the natural size is used when omitted. |
| `height` | `f64` | no | Display height; the natural size is used when omitted. |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Optional style classes. |
| `x` | `f64` | no | Placement x when used inside a diagram (ignored on a page). |
| `y` | `f64` | no | Placement y when used inside a diagram (ignored on a page). |
| `scale` | `f64` | no | Extra size multiplier (diagram form). |
| `anchor_left` | `f64` | no | Diagram anchor insets (left/right/top/bottom), like any shape. |
| `connect_points` | `list<AnchorSide>` | no | Diagram edge-attach sides, like any shape. |

```wcl
diagram { width = 200  height = 90
  image "logo.png" { x = 50.0  y = 15.0  width = 100.0  height = 60.0 }
}
```

See [Images](../references/wdoc_images.md) for the page-level `<img>` form and asset handling.

## card

![diagram](../_wdoc/wdoc_primitives-diagram-8.svg)

A box whose `body` is arbitrary wdoc content (paragraphs, callouts, lists, even nested diagrams), wrapped in an SVG `<foreignObject>` so it scales with the diagram. Timelines accept cards too, pinned to a date with `on`.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | `f64` | no | Top-left x placement in the diagram (or use anchors). |
| `y` | `f64` | no | Top-left y placement in the diagram (or use anchors). |
| `width` | `f64` | no | Card box width (default 160). |
| `height` | `f64` | no | Card box height (default 90). |
| `anchor_left` | `f64` | no | Diagram anchor insets (left/right/top/bottom), like any shape. |
| `on` | `utf8` | no | ISO date the card is pinned to (used only when it's a `timeline` child). |
| `side` | `symbol` | no | Timeline side: `:near` / `:far` / `:auto` (used only as a `timeline` child). |
| `title` | `utf8` | no | Optional plain-text heading. |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Optional style classes. |
| `connect_points` | `list<AnchorSide>` | no | Diagram edge-attach sides, like any shape. |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `body` | `WdocBlock` | yes | The card's rich content (paragraphs, lists, callouts, nested diagrams…). |

```wcl
diagram { width = 260  height = 110
  card { x = 20.0  y = 15.0  width = 220.0  height = 80.0
    title = "Note"
    p "Rich **text** inside a diagram."
  }
}
```
