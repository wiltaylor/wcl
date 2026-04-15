# wdoc Drawings

Inline SVG diagrams authored as WCL blocks. Fully evaluated through the normal pipeline (variables, loops, expressions).

Sources: `crates/wcl_wdoc/src/wdoc.wcl` (widget templates), `crates/wcl_wdoc/src/shapes.rs` (renderer + primitive enums), `crates/wcl_wdoc/src/graph_layout.rs` (layout algorithms).

## Imports

```wcl
import <wdoc.wcl>
use wdoc::draw::{diagram, rect, circle, ellipse, line, path, text, connection}
# Widgets as needed:
use wdoc::draw::{flow_terminal, flow_process, flow_decision, flow_io, flow_subprocess}
use wdoc::draw::{c4_person, c4_system, c4_container, c4_component, c4_boundary}
use wdoc::draw::{uml_class, uml_actor, uml_package, uml_note}
use wdoc::draw::{phone, browser, button, input, card, avatar, toggle, badge, navbar}
use wdoc::draw::{server, database, cloud, user}
```

## The `diagram` Block

```wcl
diagram pipeline {
  width   = 700                       # SVG viewport width (default 600)
  height  = 320                       # SVG viewport height (default 400)
  align   = "layered"                 # layout algorithm (see below)
  gap     = 40                        # spacing between laid-out nodes
  padding = 0                         # inner padding
  root    = "start"                   # only for radial layouts

  # shapes + connections as children
}
```

## Primitive Shapes

Six kinds, defined in `shapes.rs::ShapeKind`.

### `rect`
```wcl
rect box {
  x = 20, y = 20, width = 160, height = 60
  rx = 8, ry = 8                  # corner radii
  fill = "var(--color-code-bg)"
  stroke = "var(--color-link)"
  stroke_width = 2
  text t { content = "Hello" }    # nested text auto-centers
}
```

### `circle`
```wcl
circle dot {
  x = 100, y = 100
  r = 40
  fill = "var(--color-link)"
  text t { content = "Node", fill = "#fff" }
}
```

### `ellipse`
```wcl
ellipse oval {
  x = 50, y = 50
  rx = 80, ry = 40
  fill = "var(--color-nav-bg)"
  stroke = "var(--color-nav-border)"
}
```

### `line`
```wcl
line divider {
  x1 = 0,   y1 = 100
  x2 = 400, y2 = 100
  stroke = "var(--color-text)"
  stroke_width = 2
  stroke_dasharray = "8,4"
}
```

### `path`
```wcl
path arrow {
  d = "M 10 10 L 100 10 L 90 0 M 100 10 L 90 20"
  fill = "none"
  stroke = "var(--color-link)"
  stroke_width = 2
}
```

### `text`
```wcl
text label {
  x = 50, y = 100, width = 200, height = 30
  content   = "Section heading"
  font_size = 16
  anchor    = "start"   # "start" | "middle" | "end"
}
```

## Positioning

**Absolute:** `x`, `y`, `width`, `height` — fixed coordinates.

**Anchored:** `top`, `bottom`, `left`, `right` — offsets from parent edges. Opposing anchors stretch the shape.

```wcl
rect outer  { top = 0, left = 0, right = 0, bottom = 0, fill = "var(--color-nav-bg)" }
rect header { top = 0, left = 0, right = 0, height = 40, fill = "var(--color-link)" }
```

If no absolute/anchor coords are provided, the `align` layout algorithm positions the shape.

## Common Visual Attributes

| Attribute | Purpose |
|-----------|---------|
| `fill` | Fill color (CSS or `var(--color-*)`) |
| `stroke` | Outline color |
| `stroke_width` | Outline thickness |
| `stroke_dasharray` | Dash pattern, e.g. `"8,4"` |
| `opacity` | 0–1 |
| `rx`, `ry` | Corner radii (rect) / axes (ellipse) |
| `r` | Radius (circle) |

## Connections

```wcl
connection ab {
  from        = "a"              # source shape ID (required)
  to          = "b"              # target shape ID (required)
  direction   = "to"             # "to" | "from" | "both" | "none"
  from_anchor = "right"          # "top" | "bottom" | "left" | "right" | "center" | "auto"
  to_anchor   = "left"
  curve       = "bezier"         # "straight" (default) | "bezier"
  label       = "flows to"
  stroke      = "var(--color-link)"
  stroke_width = 2
  stroke_dasharray = "5,3"
}
```

Nested shape references use dotted paths: `from = "boundary.child_id"`.

## Layout Algorithms (`align`)

From `shapes.rs::Alignment`:

| Value | Use case |
|-------|----------|
| `none` | Manual positioning (default) |
| `flow` | Linear sequence |
| `stack` | Equal-spaced stack |
| `center` | Center shapes in canvas |
| `layered` | Sugiyama (flowcharts, DAGs) — reads connections, assigns layers |
| `force` | Force-directed (network diagrams) |
| `radial` | Tree around `root` node |
| `grid` | Grid arrangement |

Layered example:

```wcl
diagram pipeline {
  width = 700, height = 320, align = "layered", gap = 40

  flow_terminal start  { width = 120, height = 40, label = "Start" }
  flow_process  step   { width = 150, height = 50, label = "Process" }
  flow_decision ok     { width = 120, height = 80, label = "Valid?" }
  flow_terminal done   { width = 120, height = 40, label = "End" }

  connection e1 { from = "start", to = "step", direction = "to" }
  connection e2 { from = "step",  to = "ok",   direction = "to" }
  connection e3 { from = "ok",    to = "done", direction = "to", label = "Yes" }
}
```

## Composite Widgets

All widgets are WCL template functions that expand to primitive shape lists. Mix freely with primitives.

### UI mockups
| Widget | Attributes |
|--------|-----------|
| `phone` | `title`, `header_fill` |
| `browser` | `title`, `url` |
| `button` | `label`, `variant` (`primary`, `secondary`, `outline`) |
| `input` | `label`, `placeholder` |
| `card` | (children render inside) |
| `avatar` | `label` (initials) |
| `toggle` | `on` (`"true"` / `"false"`) |
| `badge` | `label`, `color` |
| `navbar` | `items` (comma-separated), `active_index` |

### Flowchart
| Widget | Shape |
|--------|-------|
| `flow_terminal` | Rounded oval |
| `flow_process` | Process box |
| `flow_decision` | Diamond |
| `flow_io` | Parallelogram |
| `flow_subprocess` | Box with double border |

Common attributes: `width`, `height`, `label`, `color`.

### C4 architecture
| Widget | Attributes |
|--------|-----------|
| `c4_person` | `label`, `description` |
| `c4_system` | `label`, `description`, `external` (`"true"` → dashed) |
| `c4_container` | `label`, `description`, `technology` |
| `c4_component` | `label`, `description`, `technology` |
| `c4_boundary` | `label` (children render inside) |

### UML
| Widget | Attributes |
|--------|-----------|
| `uml_class` | `label`, `stereotype`, `fields`, `methods` (pipe-separated) |
| `uml_actor` | `label` |
| `uml_package` | `label` |
| `uml_note` | `label` |

### Network / infrastructure
| Widget | Attributes |
|--------|-----------|
| `server` | `label`, `color` |
| `database` | `label`, `color` |
| `cloud` | `label`, `color` |
| `user` | `label`, `color` |

## Theme Variables

Always prefer theme variables to hard-coded colors so the diagram adapts to light/dark mode.

- `--color-bg`, `--color-text`
- `--color-link` (primary accent)
- `--color-code-bg`, `--color-code-text`
- `--color-nav-bg`, `--color-nav-border`

## Dynamic Diagrams

WCL expressions work inside diagram blocks:

```wcl
let colors = ["#FF6B6B", "#4ECDC4", "#45B7D1"]
let items  = ["Item 1", "Item 2", "Item 3"]

diagram generated {
  width = 400, height = 200

  for i in range(0, len(items)) {
    rect box-${i} {
      x = 50 + i * 100
      y = 50
      width = 80
      height = 60
      fill = colors[i]
      text t { content = items[i] }
    }
  }
}
```

## Examples

- `docs/wdoc-drawing-overview.wcl` — diagram intro
- `docs/wdoc-drawing-shapes.wcl` — primitives
- `docs/wdoc-drawing-connections.wcl` — arrows
- `docs/wdoc-drawing-layouts.wcl` — layout algorithms
- `docs/wdoc-drawing-widgets.wcl` — every widget
- `docs/wdoc-example-flowchart.wcl`, `wdoc-example-wireframe.wcl`, `wdoc-example-swimlane.wcl`
