# Wireframes

The `wf_*` family mocks up a user interface: windows, device frames, panels, inputs and
controls. The output is a static SVG drawing. Nothing is interactive.

**A widget is a diagram shape, not a page block.** Every `wf_*` type extends the `Widget`
interface, which extends `SvgBlock`. A widget is therefore a legal child of a `diagram` or a
`container`. Place it by `x` / `y` or by anchors, connect it by edges, and mix it with `rect`,
`card` and every other shape. Writing `wf_button` straight into a page body is a schema error.

```wcl
diagram {
  width = 300  height = 200
  wf_window "Account settings" {
    wf_panel { title = "Profile"
      wf_input "Display name" { value = "Wil Taylor" }
      wf_input "Email"        { value = "ai@wiltaylor.dev" }
    }
    wf_row {
      wf_button "Cancel"
      wf_button "Save" { icon = "lucide.check" }
    }
  }
}
```

**A widget is sized by its measured content.** You position the top-left corner and wdoc
measures the rest. `width` / `height` are advisory on every widget except the device frames.

The whole family is **native**: one Rust renderer measures the widget tree bottom-up and emits a
single positioned `<g>`. The `@children(Widget)` declarations exist for schema validation.

## The four groups

| Group | Blocks |
| --- | --- |
| Controls (leaves) | `wf_label`, `wf_button`, `wf_input`, `wf_dropdown`, `wf_checkbox`, `wf_radio`, `wf_toggle` |
| Frames and panels | `wf_window`, `wf_browser`, `wf_phone`, `wf_tablet`, `wf_panel` |
| Layout containers | `wf_row`, `wf_column`, `wf_grid` |
| Node graph | `wf_node_graph`, with its `wf_node` and `wf_link` children |

## Fields every widget shares

The `Widget` interface declares these, and every `wf_*` type repeats them:

| Field | Type | Meaning |
| --- | --- | --- |
| `id` | `identifier` | Edge-connection and anchor name. |
| `class` | `list<utf8>` | Extra classes, read for SVG `fill` / `stroke` overrides on the box. |
| `disabled` | `bool` | Dims the control. |
| `x`, `y` | `f64` | Top-left placement in the diagram (default `0.0`). |
| `width`, `height` | `f64` | Advisory. The widget is normally content-measured. |
| `anchor_left`, `anchor_right`, `anchor_top`, `anchor_bottom` | `f64` | Fractional (0–1) anchors pinning an edge to the parent box. |
| `connect_points` | `list<AnchorSide>` | Sides an edge attaches to (`:north :east :south :west`). |
| `theme` | `symbol` | Per-element UI theme, naming a `theme` block. |
| `accent` | `symbol` | Per-element accent hue. |
| `mode` | `symbol` | `:dark` or `:light`. |

Each unset theming field falls back to the site's `ui_*` theme, then to the document theme.

## Controls

| Block | Inline label | Own fields |
| --- | --- | --- |
| `wf_label` | `text` | — |
| `wf_button` | `text` | `icon` — a leading glyph as `pack.name`, e.g. `"lucide.check"` |
| `wf_input` | `placeholder` | `value` — set it and the field renders solid instead of greyed |
| `wf_dropdown` | `text` — the selected option | — |
| `wf_checkbox` | `label` | `checked` |
| `wf_radio` | `label` | `selected` |
| `wf_toggle` | `label` (optional) | `on` |

`wf_radio` marks one option; grouping several is your own layout job.

## Frames and panels

Each takes `@children(Widget)` and stacks them vertically.

| Block | Inline label | Own fields |
| --- | --- | --- |
| `wf_window` | `title` | `controls` — the titlebar dots and close glyph (default `true`) |
| `wf_browser` | `url` — shown in the address bar | — |
| `wf_phone` | `title` (optional status-bar caption) | `orientation` — `:portrait` (default) or `:landscape` |
| `wf_tablet` | `title` (optional) | `orientation` |
| `wf_panel` | — (`title` is a plain field) | `title` |

**The device frames are the one exception to content measuring.** `wf_browser`, `wf_phone` and
`wf_tablet` have a realistic fixed default size, so content inside them is proportioned
correctly:

| Frame | Default |
| --- | --- |
| `wf_browser` | 640 × 440 |
| `wf_phone` | 280 × 580 portrait (swapped in landscape) |
| `wf_tablet` | 480 × 640 portrait (swapped in landscape) |

An explicit `width` or `height` pins **that axis only**; the unset axis keeps the device
default, and the height still grows when the content would overflow.

## Layout containers

Thin wrappers that change how children flow. Drop one inside a window or a panel.

| Block | Flow |
| --- | --- |
| `wf_column` | Top to bottom. This is the default flow, so use it for an explicit column inside a row or a grid. |
| `wf_row` | Left to right. |
| `wf_grid` | Across `columns` equal-width columns (default `2`), wrapping into rows. |

**A container ignores its children's `x` / `y`.** The renderer lays them out internally. Only
the outermost widget's placement positions the group in the diagram.

## Placing and connecting

Widgets are shapes, so you connect them by `id` exactly as you connect a `rect`:

```wcl
diagram {
  width = 360  height = 170
  wf_button "Open settings" { id = launch  x = 20.0  y = 65.0 }
  wf_window "Settings" {
    id = win  x = 180.0  y = 20.0
    wf_checkbox "Dark mode" { checked = true }
    wf_button "Close"
  }
  launch -> win
}
```

Under an auto-layout diagram (`layout = :layered` / `:force`), omit `x` / `y`: the widgets are
measured and the solver places them.

## Node graphs

`wf_node_graph` mocks up a node editor — a shader graph, a blueprint, a dataflow pipeline. Each
`wf_node` is a titled box with `inputs` listed down its left edge and `outputs` down its right.
A `wf_link` wires one node's output to another's input.

```wcl
diagram {
  width = 560  height = 240
  wf_node_graph {
    wf_node "Texture"  { id = tex   outputs = ["RGB", "Alpha"] }
    wf_node "Fresnel"  { id = fres  outputs = ["Factor"] }
    wf_node "Multiply" { id = mul   inputs = ["A", "B"]  outputs = ["Result"] }
    wf_node "Output"   { id = out   inputs = ["Color"] }
    wf_link "tex.RGB"     { to = "mul.A" }
    wf_link "fres.Factor" { to = "mul.B" }
    wf_link "mul.Result"  { to = "out.Color" }
  }
}
```

- Endpoints are `"node.port"` strings. A bare `"node"` targets that node's first port.
- Nodes auto-lay-out from the link graph, using the same layered solver an auto-layout diagram
  uses. Give a node an explicit `x` / `y` to pin it while the rest flow around it.
- `direction` on the graph is `:left_to_right` (default) or `:top_to_bottom`.
- Links route with the shared orthogonal edge router.

| `wf_node` field | Type | Meaning |
| --- | --- | --- |
| `title` | `utf8` | The caption. `@inline(0)`. |
| `id` | `identifier` | The link-endpoint name. |
| `inputs`, `outputs` | `list<utf8>` | Port labels, down the left and right edges. |
| `x`, `y` | `f64` | Pin the node instead of auto-laying it out. |
| `class` | `list<utf8>` | Box `fill` / `stroke` overrides. |

| `wf_link` field | Type | Meaning |
| --- | --- | --- |
| `from` | `utf8` | Source endpoint. `@inline(0)`. |
| `to` | `utf8` | Destination endpoint. |
| `label` | `utf8` | Optional caption at the link's midpoint. |

`wf_node` and `wf_link` are internal children of the graph, like an `li` under a `list`. They
extend no block interface, so they declare neither a `lower` nor `@native`, and they are not
placeable shapes on their own.

## Writing your own widget

Declare a `@block` type extending `Widget` with a `lower` returning `list<Svg>`. It plugs into
the diagram render path like any custom shape:

```wcl
// A coloured status badge. The lower emits a filled box with a centred
// label at the widget's own x / y, and `fill` recolours one instance.
@block("wf_badge")
type WfBadge extends Widget {
  @inline(0) text: utf8
  fill = "#2e7d32"
  id: identifier?  class: list<utf8>?  disabled: bool?
  x = 0.0  y = 0.0  width = 96.0  height = 26.0
  anchor_left: f64?  anchor_right: f64?  anchor_top: f64?  anchor_bottom: f64?
  connect_points: list<AnchorSide>?
  theme: symbol?  accent: symbol?  mode: symbol?
  lower = fn(b: WfBadge) -> list<Svg> {
    [ Svg::Rect {
        x: b.x, y: b.y, width: b.width, height: b.height,
        fill: b.fill, class: b.class,
      },
      Svg::Label {
        content: b.text,
        x: b.x + b.width / 2.0,
        y: b.y + b.height / 2.0 + 4.0,
        fit_width: b.width, fit_height: b.height,
        fill: "#ffffff",
      } ]
  }
}

diagram {
  width = 320  height = 40
  wf_badge "passing" { x = 12.0  y = 7.0 }
  wf_badge "failing" { x = 130.0  y = 7.0  fill = "#c62828" }
}
```

Two rules differ from the built-ins:

1. **Repeat the shared field block verbatim.** A WCL schema lists a type's own fields, so an
   interface does not supply them. Copy the `id` … `mode` lines above.
2. **A custom widget reads its own `x` / `y`.** It is a standalone shape, so the lowering
   positions itself — unlike a built-in child, whose parent container places it.

## Gotchas

- **A `wf_*` block needs a `diagram` or `container` parent.** It is a shape, not page content.
- **Container children ignore `x` / `y`.** Only the outermost widget's placement counts.
- **`width` / `height` are advisory**, except on the three device frames, where they pin one
  axis each.
- **A custom widget does not nest inside a built-in container.** The built-in containers lay out
  the built-in widgets only, so your widget renders as a standalone shape in the diagram.
- **Wireframes are static.** A `wf_toggle` with `on = true` draws the switch across; it does not
  toggle.
- **Theming is read in Rust.** Default text and icons paint with the resolved UI-theme palette,
  and a `class` supplies `fill` / `stroke` overrides baked onto the box.
