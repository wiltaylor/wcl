# wdoc Drawings

Inline SVG diagrams authored as WCL blocks. Fully evaluated through the normal pipeline (variables, loops, expressions).

Sources: `crates/wcl_wdoc/src/wdoc/header.wcl` (widget templates), `crates/wcl_wdoc/src/source.rs` (host functions), `crates/wcl_wdoc/src/shapes.rs` (renderer + primitive enums), `crates/wcl_wdoc/src/graph_layout.rs` (layout algorithms).

## Imports

```wcl
import <wdoc.wcl>
use wdoc::{p, bold}
use wdoc::draw::{diagram, rect, circle, ellipse, line, path, text, text_block, inline_svg}
use wdoc::draw::{image, icon, map, connection, group}
# Widgets as needed:
use wdoc::draw::{phone, phone_landscape, browser, window, tablet, tablet_landscape}
use wdoc::draw::{button, slider, input, card, avatar, toggle, checkbox, radio, button_group}
use wdoc::draw::{textbox, dropdown, inline_image, menubar, context_menu, menu_item}
use wdoc::draw::{badge, navbar, stat_card, profile_card, action_panel, list_item}
use wdoc::draw::{datatable, datatable_column, datatable_row, datatable_cell}
use wdoc::draw::{graph_node, graph_row, graph_divider, pie_chart, bar_chart, line_chart, chart_point}
use wdoc::draw::{flow_terminal, flow_process, flow_decision, flow_io, flow_subprocess}
use wdoc::draw::{c4_person, c4_system, c4_container, c4_component, c4_boundary}
use wdoc::draw::{uml_class, uml_actor, uml_package, uml_note}
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

Primitive kinds are defined in `shapes.rs::ShapeKind`.

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

### `text_block` and `inline_svg`

`text_block` lays out wrapped rich text inside a drawing. `inline_svg` embeds trusted SVG markup as a drawing primitive.

```wcl
text_block notes {
  x = 40, y = 40, width = 280, height = 120
  content = "Use " + bold("markup") + " inside wrapped drawing text."
}

inline_svg logo {
  x = 360, y = 40, width = 48, height = 48
  content = "<svg viewBox=\"0 0 24 24\"><path d=\"M4 12h16\"/></svg>"
}
```

### `map`
PNG-backed viewport with internal pan/zoom. Child shapes are ordinary drawing shapes, positioned in the map's `content_width` / `content_height` coordinate space.

```wcl
map world {
  x = 0, y = 0, width = 900, height = 560
  src = "images/world-map.png"
  content_width = 2400
  content_height = 1600
  view_x = 600, view_y = 380
  view_width = 900, view_height = 560
  min_zoom = 0.5
  max_zoom = 8

  circle brisbane { x = 1730, y = 1040, width = 18, height = 18, fill = "#ef4444", map_fixed = true }
  text brisbane_label { x = 1752, y = 1028, width = 110, height = 24, content = "Brisbane", anchor = "start" }
}
```

Set `map_fixed = true` on a child shape to keep its on-screen size stable while the map zooms. By default the shape is anchored at its center; override with `map_anchor_x` / `map_anchor_y` for pin-tip behavior.

### `icon`
SVG icon from a configured `wdoc::icon_set`. Icons are loaded from local `name.svg` files, sanitized, and normalized to the set's `normalize_width` / `normalize_height` unless `normalize_mode = "none"`.

```wcl
icon save {
  x = 24, y = 24, width = 32, height = 32
  name = "save"
  fill = "var(--color-link)"
}
```

Use `icon_part` blocks on the icon set to map custom CSS selectors and properties to variables supplied through `props`; `fill` and `stroke` are supported by default.

### `group`

Groups nest shapes under one positioned container. Children can be referenced by dotted connection paths such as `panel.submit`.

```wcl
group panel {
  x = 20, y = 20, width = 280, height = 160
  rect bg { left = 0, top = 0, right = 0, bottom = 0, rx = 8, fill = "var(--color-bg)" }
  button submit { x = 20, y = 100, width = 120, height = 36, label = "Save" }
}
```

### Timeline, Sprite, Tilemap, and Runtime Shapes

The renderer also supports animation and game-oriented drawing primitives:

| Primitive | Purpose |
|-----------|---------|
| `dopesheet` | Timeline data container |
| `sprite` | Sprite image/animation primitive |
| `dopesheet_view` | Visual timeline editor/view |
| `tilemap` | Tile-grid layer |
| `game_layer` | Layer container for game-style scenes |
| `terminal` | Terminal widget with `terminal_command`, `terminal_output`, and `terminal_prompt` children |
| `class`, `state`, `animation`, `keyframe`, `event`, `set_signal` | Runtime styling/state/animation/event model |

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

### UI mockups and controls
| Widget | Attributes |
|--------|-----------|
| `phone` | `title`, `header_fill` |
| `phone_landscape` | `title`, `header_fill` |
| `browser` | `title`, `url` |
| `window` | `title` |
| `tablet` | `title` |
| `tablet_landscape` | `title` |
| `button` | `label`, `variant` (`primary`, `secondary`, `outline`) |
| `slider` | `value`, `min`, `max`, `step` |
| `input` | `label`, `placeholder` |
| `card` | (children render inside) |
| `avatar` | `label` (initials) |
| `toggle` | `on` (`"true"` / `"false"`) |
| `checkbox` | `label`, `checked` |
| `radio` | `label`, `checked`, `name` |
| `button_group` | child buttons |
| `textbox` | `value`, `placeholder`, multiline text |
| `dropdown` | `label`, `items`, `selected_index` |
| `inline_image` | `src`, `alt` |
| `menubar` | child menu items |
| `context_menu` | child menu items |
| `menu_item` | `label`, `icon?`, `shortcut?` |
| `badge` | `label`, `color` |
| `navbar` | `items` (comma-separated), `active_index` |
| `stat_card` | `label`, `value`, `trend?` |
| `profile_card` | `name`, `subtitle`, `avatar?` |
| `action_panel` | heading/action grouping |
| `list_item` | `label`, `subtitle?`, `icon?` |

### Data table widget

`datatable` is the wireframe table widget. It can render WDoc text and nested wireframe controls in cells.

```wcl
datatable users {
  x = 20, y = 80, width = 720, height = 260

  datatable_column name { label = "Name", width = 180 }
  datatable_column status { label = "Status", width = 140 }
  datatable_column action { label = "Action", width = 160 }

  datatable_row alice {
    datatable_cell name { p "Alice Example" }
    datatable_cell status { badge active { label = "Active", color = "#16a34a" } }
    datatable_cell action { button edit { label = "Edit", variant = "secondary" } }
  }
}
```

Rows can also come from a WCL table inside the widget block:

```wcl
datatable users {
  datatable_column name { label = "Name" }
  datatable_column role { label = "Role" }

  table rows {
    name: string
    role: string
    | "Alice" | "Admin" |
    | "Bob"   | "Viewer" |
  }
}
```

Use sub-block rows when cells need WDoc markup or controls. Use an embedded `table` when the rows are plain data.

### Graph and charts

| Widget | Attributes |
|--------|-----------|
| `graph_node` | `label`, child rows/dividers |
| `graph_row` | `label`, `value?` |
| `graph_divider` | divider inside a graph node |
| `pie_chart` | data rows/children |
| `bar_chart` | data rows/children |
| `line_chart` | data rows/children |
| `chart_point` | `x`, `y`, `label?` |

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
