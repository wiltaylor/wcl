# Flowcharts and swimlanes

A flowchart is a `diagram` holding flowchart shapes, wired with edges, under
`layout = :layered`. Declare the steps and the arrows; the renderer ranks the steps and routes
the arrows. There is no `flowchart` block.

```wcl
diagram {
  width = 320  height = 220  layout = :layered  layer_gap = 20.0
  desc = "parse pipeline"

  terminator "Start"  { id = start }
  process    "Parse"  { id = parse   width = 100.0 }
  decision   "Valid?" { id = valid   width = 100.0  height = 60.0 }
  terminator "Render" { id = render  width = 100.0 }

  start -> parse  :flow
  parse -> valid  :flow
  valid -> render :yes
}
```

## The four node shapes

| Block | Drawn as | Use |
| --- | --- | --- |
| `process` | Rectangle | An action or a step. |
| `decision` | Diamond | A branch — a test with two or more answers. |
| `terminator` | Oval | A start or end node. |
| `node` | Circle | A graph vertex, for network and relationship pictures. |

All four take the same fields. Only the drawn outline differs.

| Field | Default | Meaning |
| --- | --- | --- |
| inline label | required | The centred text: `process "Validate" { … }`. |
| `id` | — | Name used by an edge. Needed for auto-layout to rank the shape. |
| `x` / `y` | `0.0` | Top-left placement. Omit under an auto-layout. |
| `width` / `height` | `80.0` / `40.0` | Box size. `node` defaults to `56.0` square. |
| `fill` / `stroke` | — | Inline paint. Prefer a `class`. |
| `class` | — | Style classes. |
| `connect_points` | all sides | Which sides edges attach to (`:north` / `:east` / `:south` / `:west`). |
| `icon`, `icon_size`, `icon_pos`, `icon_class` | — | An icon badge inside the box. |
| `link` | — | Make the shape a link to an in-site page. |

These are lowered shapes, not native ones. Each expands into a primitive plus a `label`. That is
why they work on every backend, and why you can copy one to declare your own.

Three behaviours follow from that lowering:

- **The label auto-fits.** The shapes set no `font_size`, so a long label shrinks to fit the box
  instead of overflowing it. A `decision` fits its text into half the box width, because a diamond's
  usable interior is narrower than its bounding box.
- **An unstyled shape gets a theme class.** With no `fill`, no `stroke` and no `class`, a
  `process` paints itself `wdoc-process` and its label `wdoc-shape-text`. Set any one of those
  three fields and the fallback stops for the whole shape — the label then takes your class too.
- **A `:left` icon badge shifts the label right** so the two do not overlap. Other `icon_pos`
  values leave the label centred.

```wcl
diagram { width = 260  height = 90
  process "Deploy" { x = 20.0  y = 20.0  width = 220.0  height = 50.0
                     icon = "lucide.rocket" }
}
```

## Layered auto-layout

`layout = :layered` on the diagram ranks the shapes topologically from the edge graph, then
elbow-routes the arrows.

| Field | Meaning |
| --- | --- |
| `direction` | `:top_to_bottom` (default) or `:left_to_right`. |
| `layer_gap` | Spacing between ranks. |
| `node_gap` | Spacing between shapes within one rank. |

Under an auto-layout, **omit `x` and `y`**. The shapes carry `0.0` defaults precisely so the
layout can place them through a wrapper transform. Per-shape `width` and `height` are still
honoured, and they size the cell.

A shape that no edge names has no rank to derive, so it lands in the first rank beside the
roots. If a step floats to the top unexpectedly, it is missing an edge.

```wcl
diagram { width = 420  height = 200  layout = :layered  direction = :left_to_right
  process  "Fetch"   { id = f  width = 90.0 }
  decision "Cached?" { id = c  width = 90.0  height = 56.0 }
  process  "Parse"   { id = pp  width = 90.0 }
  process  "Serve"   { id = s  width = 90.0 }
  f -> c  :flow
  c -> pp :no
  c -> s  :yes
  pp -> s :flow
}
```

Label a decision's branches with the `:yes` and `:no` edge kinds. Those two kinds draw the word
on the edge. That is the whole reason they exist: a `->` statement carries no label of its own.
For any other branch word, use the computed `edges` record form with a `label`. See
[`wdoc_diagram_connections.md`](wdoc_diagram_connections.md).

Two other layouts suit graph-shaped pictures rather than flows: `:radial` for one hub and its
neighbours, `:force` for a cyclic graph with no natural rank. Both are in
[`wdoc_diagrams.md`](wdoc_diagrams.md), together with `boundary`, which groups shapes under any
layout.

## Swimlanes

wdoc has no swimlane block. You compose one from three parts. Draw each lane as a translucent
`rect` band. Name it with a `label`. Place the nodes inside their lane with `x` / `y`. The lanes
are ordinary shapes, so everything else keeps working — icons, decisions, boundaries, and edges
that cross lanes.

Composing lanes means placing by hand, so **leave `layout` at its default** — an auto-layout
would move the nodes out of their bands. Keep `routing = :elbow`, which is what makes a
lane-crossing arrow read as a right-angled hand-off.

Use a translucent grey for the bands (`"#7f7f7f14"` and `"#7f7f7f2e"`), so they read on light
and dark themes alike.

### Horizontal lanes — the flow runs left to right

```wcl
diagram {
  width = 560  height = 240  routing = :elbow
  desc = "order fulfilment by team"

  // Lane bands, alternating tint.
  rect { x = 0.0  y = 0.0    width = 560.0  height = 78.0  fill = "#7f7f7f14" }
  rect { x = 0.0  y = 80.0   width = 560.0  height = 78.0  fill = "#7f7f7f2e" }
  rect { x = 0.0  y = 160.0  width = 560.0  height = 78.0  fill = "#7f7f7f14" }

  label "Customer"  { x = 52.0  y = 16.0   font_size = 12.0  fill = "#888" }
  label "Sales"     { x = 42.0  y = 96.0   font_size = 12.0  fill = "#888" }
  label "Warehouse" { x = 58.0  y = 176.0  font_size = 12.0  fill = "#888" }

  // One node per lane, placed inside its band.
  process "Place order" { id = order  x = 120.0  y = 23.0   width = 120.0  height = 32.0 }
  process "Approve"     { id = appr   x = 300.0  y = 103.0  width = 120.0  height = 32.0 }
  process "Pick & pack" { id = pick   x = 120.0  y = 183.0  width = 120.0  height = 32.0 }
  process "Ship"        { id = ship   x = 410.0  y = 183.0  width = 110.0  height = 32.0 }

  order -> appr :flow
  appr  -> pick :flow
  pick  -> ship :flow
}
```

### Vertical lanes — the flow runs top to bottom

The same idea rotated. The bands become full-height columns and each node sits in its column.

```wcl
diagram {
  width = 480  height = 270  routing = :elbow

  rect { x = 0.0    y = 0.0  width = 158.0  height = 270.0  fill = "#7f7f7f14" }
  rect { x = 160.0  y = 0.0  width = 158.0  height = 270.0  fill = "#7f7f7f2e" }
  rect { x = 320.0  y = 0.0  width = 160.0  height = 270.0  fill = "#7f7f7f14" }

  label "Customer"  { x = 79.0   y = 18.0  font_size = 12.0  fill = "#888" }
  label "Sales"     { x = 239.0  y = 18.0  font_size = 12.0  fill = "#888" }
  label "Warehouse" { x = 400.0  y = 18.0  font_size = 12.0  fill = "#888" }

  process "Place order" { id = vorder  x = 24.0   y = 40.0   width = 110.0  height = 32.0 }
  process "Approve"     { id = vappr   x = 184.0  y = 110.0  width = 110.0  height = 32.0 }
  process "Pick & pack" { id = vpick   x = 344.0  y = 110.0  width = 110.0  height = 32.0 }
  process "Ship"        { id = vship   x = 344.0  y = 200.0  width = 110.0  height = 32.0 }

  vorder -> vappr :flow
  vappr  -> vpick :flow
  vpick  -> vship :flow
}
```

Draw the bands **first**. Shapes render in declaration order, so a band declared after a node
paints over it.

## Data-driven flowcharts

Shapes and edges are ordinary block children and an ordinary list field, so you can compute
both. Build the edges with `map` and let the layout place everything:

```wcl
diagram { width = 400  height = 260  layout = :layered
  process "Fetch" { id = s1  width = 100.0 }
  process "Parse" { id = s2  width = 100.0 }
  process "Store" { id = s3  width = 100.0 }
  edges = [
    { source: "s1", destination: "s2", kind: :flow },
    { source: "s2", destination: "s3", kind: :flow },
  ]
}
```

## Gotchas

- `process`, `decision`, `terminator` and `node` are **shapes**. They are legal inside a
  `diagram` or a `container`, and nowhere else. A page-level one fails the build.
- The layout cannot rank a shape with no `id`, and no edge can name it. Give every flowchart
  node an id.
- Setting `x` / `y` under `:layered` does not pin a shape. The coordinates apply *inside* the
  cell the layout assigned, so the shape is displaced from its rank instead.
- Hand-placed swimlane bands need the default free layout to stay under their nodes.
- Give the diagram a `desc`. A flowchart with no accessible name announces nothing.
