# The diagram canvas

A `diagram` is wdoc's drawing surface. It declares a pixel canvas, holds shapes, and renders to
inline SVG. Every backend gets that same SVG, so a diagram survives the HTML, PDF and Markdown
targets without a second authoring form.

```wcl
diagram {
  width = 320  height = 140
  rect { id = a  x = 20.0   y = 40.0  width = 90.0  height = 50.0  class = ["wdoc-process"] }
  rect { id = b  x = 210.0  y = 40.0  width = 90.0  height = 50.0  class = ["wdoc-process"] }
  a -> b
}
```

A `diagram` is a page block. Write it in a page body beside `p` and `code`. Its children are
shapes, and a shape is any block that implements the `SvgBlock` interface.

This page covers the canvas, the primitive shapes, `container`, `boundary`, the layout modes and
shape styling. The other shape families sit inside a `diagram` in exactly the same way, and each
has its own reference:

| Shapes | Reference |
| --- | --- |
| `process`, `decision`, `terminator`, `node` | [`wdoc_flowcharts.md`](wdoc_flowcharts.md) |
| `bar_chart`, `line_chart`, `pie_chart` | [`wdoc_charts.md`](wdoc_charts.md) |
| `timeline`, `dopesheet` | [`wdoc_timelines.md`](wdoc_timelines.md) |
| `tree`, `node_table`, `card` | [`wdoc_tree.md`](wdoc_tree.md) |
| `tilemap`, `map` | [`wdoc_tilemaps.md`](wdoc_tilemaps.md) |
| The `wf_*` wireframe widgets | [`wdoc_wireframe.md`](wdoc_wireframe.md) |

`sequence_diagram` and `state_diagram` are the exception: they are page blocks of their own, not
diagram shapes. See [`wdoc_sequence_state.md`](wdoc_sequence_state.md).

## Integer and decimal fields

`diagram.width` and `diagram.height` are `i64`. Every shape coordinate is `f64`.

```wcl
diagram {
  width = 320       // i64 — an integer, no decimal point
  height = 140
  rect { x = 20.0  width = 90.0 }   // f64 — a decimal, but an integer also works
}
```

An `f64` field takes either form: `x = 20` and `x = 20.0` both work. An `i64` field takes the
integer only. **Write `width = 320.0` on the diagram and the renderer reads no width at all**:
it emits `<svg width="0">` and the diagram disappears. `wcl check` passes, and the build
reports nothing. If a diagram renders blank, check for a decimal point on `width` or `height`
first.

The same rule holds for every other `i64` field: `columns`, `seed`, `iterations`, a timeline's
`every`, a chart point's `category`, a dopesheet's frame geometry.

## The canvas fields

| Field | Type | Meaning |
| --- | --- | --- |
| `width` / `height` | `i64` | Canvas size in pixels. Required. |
| `id` | `identifier?` | Explicit HTML id on the `<svg>`. |
| `class` | `list<utf8>?` | Style classes on the `<svg>`. |
| `desc` | `utf8?` | Accessible name — becomes `<title>` plus `role="img"` and `aria-label`. |
| `layout` | `symbol?` | `:free` (default) / `:grid` / `:layered` / `:force` / `:radial`. |
| `routing` | `symbol?` | Edge routing: `:elbow` (default) / `:straight`. |
| `edge_separation` | `f64?` | Step that separates parallel edges (default 4). |
| `pan_zoom` | `bool?` | Wrap the SVG in an interactive pan and zoom viewport. |
| `zoom_min` / `zoom_max` | `f64?` | Zoom clamp (defaults 1.0 and 4.0). `1.0` is the fitted view. |
| `pan_margin` | `f64?` | Extra overscroll past the content bounds, in px (default 0). |

The per-layout fields (`columns`, `cell_width`, `layer_gap`, `repulsion`, `hub`, …) are listed
with their layout mode below. Routing, edges and `connect_points` belong to
[`wdoc_diagram_connections.md`](wdoc_diagram_connections.md).

**`width` and `height` are the rendered size, not a clip.** The renderer fits the `viewBox`
over the shapes and edges it actually drew, plus 10px of padding. A shape at `x = 400.0` on a
320-wide canvas is therefore still visible; the whole drawing scales down to fit. Use the
declared size to set the aspect ratio and the on-page size.

Give every diagram a `desc`. Without one, a screen reader announces nothing at all.

## Placing a shape

There are two ways to place a shape. They combine per axis.

**Coordinates.** `x` / `y` are the top-left corner, in canvas units. `circle` uses `cx` / `cy`
for its centre instead, and `line` uses `x1` / `y1` / `x2` / `y2`.

**Anchors.** `anchor_left`, `anchor_right`, `anchor_top` and `anchor_bottom` are **insets in
pixels from the parent box**, not fractions.

```wcl
diagram {
  width = 300  height = 120
  // 20px in from the left, 20px in from the right: the rect stretches.
  rect { anchor_left = 20.0  anchor_right = 20.0  y = 30.0  height = 60.0
         class = ["wdoc-process"] }
}
```

- One anchor on an axis **moves** the shape and keeps its size.
- Both anchors on an axis **stretch** it: the size becomes `parent - near - far`.
- Neither leaves `x` / `y` in charge.

The parent box is the diagram canvas for a top-level shape, and the container's box for a
child. Some stdlib schemas describe these fields as "fractional anchor (0–1)". That text is
wrong; the renderer reads a pixel inset.

## The primitive shapes

Everything else lowers into these. Each takes the shared shape fields — `id`, `class`, the four
anchors, `connect_points` and `link` — plus its own geometry.

### rect

```wcl
diagram { width = 200  height = 90
  rect { x = 20.0  y = 15.0  width = 120.0  height = 60.0  class = ["wdoc-process"] }
}
```

`x`, `y`, `width`, `height`, `fill`, `stroke`. This is the box shape you reach for first.

### circle

```wcl
diagram { width = 200  height = 100
  circle { cx = 100.0  cy = 50.0  r = 36.0  class = ["wdoc-node"] }
}
```

`cx`, `cy`, `r`, `fill`, `stroke`. A circle is positioned by its centre. An edge to a circle
attaches on the circle boundary rather than a bounding-box side.

### line

```wcl
diagram { width = 200  height = 80
  line { x1 = 20.0  y1 = 60.0  x2 = 180.0  y2 = 20.0  stroke = "#bf616a" }
}
```

`x1`, `y1`, `x2`, `y2`, `stroke`. A line is a bare segment with no arrowhead. For an arrow
between two shapes, use an edge (`a -> b`), not a `line`.

### label

```wcl
diagram { width = 200  height = 60
  label "Hello" { x = 100.0  y = 36.0  font_size = 22.0 }
}
```

The text is the inline label. `x` / `y` position it, `fill` colours it. Named `label`, not
`text`, because `text` is the page-level prose block.

Omit `font_size`. Give `fit_width` / `fit_height` instead. The renderer then picks a size that
fits the text inside that region. Every stdlib shape labels itself this way, which is why
a long label shrinks rather than overflowing.

### polygon

```wcl
diagram { width = 200  height = 100
  polygon { points = "40,80 100,15 160,80"  fill = "#ebcb8b"  stroke = "#333" }
}
```

`points` is a space-separated list of `x,y` pairs, as one string. The renderer closes and fills
the shape. There is no ellipse primitive. The stdlib `terminator` approximates one with a
40-point polygon.

### image

```wcl
diagram { width = 200  height = 90
  image "logo.png" { x = 50.0  y = 15.0  width = 100.0  height = 60.0 }
}
```

A raster image placed as an SVG `<image>`. The source is the inline label: a path relative to
the build entry file (copied into `_wdoc/`), a URL, or a `data:` URI. `scale` multiplies the
size. See [`wdoc_media.md`](wdoc_media.md) for the page-level form.

## container — grouping and nested layout

A `container` is an SVG group that holds child shapes and can lay them out.

```wcl
diagram { width = 240  height = 130
  container {
    anchor_left = 10.0  anchor_top = 10.0
    fill = "#eef"  stroke = "#88a"  padding = 10.0
    layout = :grid  columns = 2  cell_width = 90.0  cell_height = 44.0  gap = 10.0
    rect { class = ["wdoc-process"] }
    rect { class = ["wdoc-node"] }
    rect { class = ["wdoc-decision"] }
    rect { class = ["wdoc-terminator"] }
  }
}
```

- Set `fill` or `stroke` to draw the chrome — a background rect the full size of the box. With
  neither, the container groups its children invisibly.
- `padding` insets the children from that chrome and grows the outer box by `2 * padding`.
- The container **auto-fits its content** under `:grid` and `:layered`. A declared `width` /
  `height` is a minimum, never a ceiling.
- A container runs its own `layout` over its children, and takes the same layout fields the
  diagram does. Nest them freely.
- The chrome is **not** an obstacle. Edges route straight through it.

## boundary — a labelled box behind shapes

```wcl
diagram {
  width = 360  height = 300  layout = :radial  hub = platform  routing = :straight
  process "Platform"  { id = platform   width = 96.0  height = 44.0 }
  process "Warehouse" { id = warehouse  width = 96.0  height = 44.0 }
  process "Billing"   { id = billing    width = 96.0  height = 44.0 }
  terminator "Customer" { id = cust     width = 96.0  height = 44.0 }

  cust     -> platform  :flow
  platform -> warehouse :flow
  platform -> billing   :flow

  boundary "Achmisoft" { members = [platform, warehouse, billing] }
}
```

A `boundary` draws a labelled box *behind* the shapes named in `members`, sized to wherever the
layout put them, plus `padding` (default 12).

The difference from a container is the point of the block: a boundary **owns no layout**. It
never moves its members and never joins the solver, so it works under `:radial` and `:force`,
where a container cannot. This is the C4 boundary — box the systems you own, wherever the
solver placed them.

| Field | Meaning |
| --- | --- |
| inline label | The title drawn on the box. |
| `members` | `list<identifier>` — ids resolved anywhere in the diagram, like edge endpoints. |
| `padding` | Inset from the member bounding box (default 12). |
| `label_pos` | `:top_left` (default) / `:top` / `:top_right` / `:bottom_left` / `:bottom` / `:bottom_right`. |
| `stroke` / `fill` / `class` | Override the themed border and tint. |

The build warns about a member id that matches no shape, and skips it. A boundary is not itself an
edge endpoint.

## Layout modes

`layout` on the diagram (or on a container) decides where shapes go. The auto modes derive
positions from the edge graph, so you declare shapes and edges and omit `x` / `y`.

| Mode | Behaviour |
| --- | --- |
| `:free` | Default. Every shape sits at its own `x` / `y`. |
| `:grid` | Children flow into a regular grid, in declaration order. |
| `:layered` | Shapes rank topologically from the edges and stack along `direction`. |
| `:force` | A force-directed solver repels shapes and pulls connected ones together. |
| `:radial` | One hub at the centre, everything else on rings by graph distance. |

### :grid

```wcl
diagram { width = 260  height = 130  layout = :grid
  columns = 3  cell_width = 70.0  cell_height = 44.0  gap = 10.0
  rect { class = ["wdoc-process"] }
  rect { class = ["wdoc-node"] }
  rect { class = ["wdoc-decision"] }
  rect { class = ["wdoc-terminator"] }
}
```

Fields: `columns`, `cell_width`, `cell_height`, `gap`. Each child receives the cell size as its
parent box, so an anchored child stretches to the cell.

### :layered

```wcl
diagram { width = 320  height = 220  layout = :layered  layer_gap = 22.0
  process "Fetch" { id = fetch  width = 90.0  height = 38.0 }
  process "Parse" { id = parse  width = 90.0  height = 38.0 }
  process "Store" { id = store  width = 90.0  height = 38.0 }
  fetch -> parse :flow
  parse -> store :flow
}
```

Fields: `direction` (`:top_to_bottom` default, or `:left_to_right`), `layer_gap` between ranks,
`node_gap` within a rank. This is the flowchart layout — see
[`wdoc_flowcharts.md`](wdoc_flowcharts.md).

### :force

```wcl
diagram { width = 320  height = 240  layout = :force  routing = :straight
  node "A" { id = na }
  node "B" { id = nb }
  node "C" { id = nc }
  node "D" { id = nd }
  na -> nb
  na -> nc
  nb -> nd
  nc -> nd
}
```

Fields: `iterations` (300), `repulsion` (9000), `link_distance` (60), `gravity` (0.05), `seed`
(1). The solver is **deterministic for a given seed**. A rebuild reproduces the same picture,
and a new `seed` gives a new arrangement. Best for cyclic or undirected graphs with no
natural rank.

### :radial

```wcl
diagram { width = 360  height = 300  layout = :radial  hub = api  routing = :straight
  process "API"      { id = api     width = 96.0  height = 44.0 }
  process "Web"      { id = web     width = 96.0  height = 44.0 }
  process "Database" { id = db      width = 96.0  height = 44.0 }
  process "Stripe"   { id = stripe  width = 96.0  height = 44.0 }
  web -> api :flow
  api -> db  :flow
  api -> stripe :flow
}
```

Fields: `hub` (defaults to the highest-degree shape), `radius` (auto-fit), `ring_gap` (120 per
outer ring), `start_angle` in radians (`-PI/2`, i.e. top), `node_gap` as the minimum gap
between ring neighbours. A shape's ring is its graph distance from the hub. Pair it with
`routing = :straight` for clean spokes.

## Styling shapes

Every shape takes `fill` and `stroke` attributes, and every shape takes a `class` list. **Prefer
the class.** A colour written into `fill` is fixed; a class follows the site theme and the
reader's light or dark mode.

```wcl
// Top level, beside the pages — applies across the site.
class "dgm-box" {
  fill         = "#5e81ac"
  stroke       = "#2e3440"
  stroke_width = "2"
  dark  { fill = "#88c0d0"  stroke = "#eceff4" }
  light { fill = "#5e81ac"  stroke = "#2e3440" }
}

diagram { width = 320  height = 120
  rect { x = 20.0  y = 25.0  width = 110.0  height = 70.0  class = ["dgm-box"] }
}
```

A `class` block understands `fill`, `stroke`, `stroke_width`, `opacity`, a `css` escape hatch
for anything else, and `dark { }` / `light { }` overrides. Redeclare a built-in name — such as
a chart's `wdoc-series-1` — to recolour it everywhere.

Ready-made shape classes that read the theme palette:

| Class | Use |
| --- | --- |
| `wdoc-process` | The process-box fill. What an unstyled `process` gets. |
| `wdoc-decision` | The decision-diamond fill. |
| `wdoc-terminator` | The start/end oval fill. |
| `wdoc-node` | The graph-node circle fill. |
| `wdoc-shape-text` | Label text that stays legible over those fills. |
| `wdoc-series-1` … `-8` | The chart and timeline-phase palette. |

A stdlib shape only falls back to its theme class when you give it **no** `fill`, `stroke` or
`class` at all. Set any one of the three and you own the whole appearance, text class included.

See [`wdoc_styling.md`](wdoc_styling.md) for the class system itself.

## Icon badges

The box-like shapes — `rect`, `circle`, `container` and the flowchart shapes — take an icon badge
drawn over the box.

| Field | Meaning |
| --- | --- |
| `icon` | The icon name, optionally `pack.name`. |
| `icon_size` | Badge size. Default `min(width, height) * 0.4`. |
| `icon_pos` | `:left` (default), `:right`, `:center`, `:top_left`, `:top_right`, `:bottom_left`, `:bottom_right`. |
| `icon_class` | Style classes on the badge. |

A leading `:left` badge shifts the centred label right, so the two do not overlap. See
[`wdoc_icons.md`](wdoc_icons.md).

## Links

Any shape takes `link`, which wraps it in a clickable anchor.

```wcl
diagram { width = 320  height = 100
  process "Chapter one" { x = 40.0  y = 25.0  width = 240.0  height = 50.0
                          link = "intro" }
}
```

The target resolves like a prose link: a bare page name, or `site:page` across sites. An unknown
page fails the build. HTML anchors cannot nest, so do not put `link` on a container whose
children are also linked — link the container's title `label` instead.

## Pan and zoom

```wcl
diagram { width = 320  height = 160  pan_zoom = true  zoom_min = 0.5  zoom_max = 4.0
  rect { id = pa  x = 20.0   y = 30.0  width = 80.0  height = 50.0  class = ["wdoc-node"] }
  rect { id = pb  x = 210.0  y = 90.0  width = 80.0  height = 50.0  class = ["wdoc-process"] }
  pa -> pb
}
```

`pan_zoom = true` wraps the SVG in a viewport with wheel zoom, drag pan, and corner
`+` / `−` / `⟲` buttons. Zoom clamps to `[zoom_min, zoom_max]`, where `1.0` is the fitted view.
Panning allows half a viewport of overscroll past each content edge, plus `pan_margin`.

This is a browser behaviour. A PDF or Markdown build renders the same static SVG. A diagram
holding a `map` is interactive without asking.

## Gotchas

- Canvas `width` / `height` are `i64`. A decimal there silently renders a zero-width SVG.
- The anchors are pixel insets from the parent box, whatever the schema doc string says.
- A shape with no `id` cannot be an edge endpoint or a `boundary` member.
- `class` on a shape **replaces** the theme default rather than adding to it.
- Container chrome and boundary boxes only look solid. Edges cross them.
- A shape outside the declared canvas is not clipped — the `viewBox` grows to include it.
