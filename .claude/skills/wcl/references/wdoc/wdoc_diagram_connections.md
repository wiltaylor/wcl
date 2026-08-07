# Connections and routing

Inside a `diagram`, `source -> destination` draws an arrow between two shapes. The endpoints are
shape `id`s, never coordinates: you say which shapes connect, and the renderer works out where
the line leaves and enters.

```wcl
diagram {
  width = 320  height = 120
  rect { id = a  x = 10.0   y = 30.0  width = 70.0  height = 40.0  class = ["wdoc-process"] }
  rect { id = b  x = 240.0  y = 30.0  width = 70.0  height = 40.0  class = ["wdoc-process"] }

  a -> b
  a -> b :flow
}
```

## The two authoring forms

**A connection statement** — `a -> b`, optionally tagged with a kind symbol:

```wcl
a -> b            // kind :default
a -> b :flow      // tagged
```

**A computed `edges` field** — a list of records:

```wcl
diagram {
  width = 380  height = 100  routing = :straight
  rect { id = la  x = 10.0   y = 30.0  width = 70.0  height = 36.0  class = ["wdoc-node"] }
  rect { id = lb  x = 290.0  y = 30.0  width = 70.0  height = 36.0  class = ["wdoc-node"] }
  edges = [ { source: "la", destination: "lb", label: "ships to", dash: "5 4" } ]
}
```

Both forms may appear in one diagram. The record form is the one to compute — build it with
`map` over your data. It also carries the only presentation payload there is:

| Record field | Meaning |
| --- | --- |
| `source` / `destination` | Shape ids, as strings. Required. |
| `kind` | An `EdgeKind` symbol. |
| `label` | Text drawn at the edge midpoint, class `wdoc-edge-label`. |
| `dash` | An inline `stroke-dasharray` value, e.g. `"5 4"`. |

`label` and `dash` are unreachable from `->`: the statement grammar carries a kind and nothing
else.

## Where edges live

`diagram` and `container` each declare an `edges` connection slot, and the renderer gathers
them **recursively**. The top-level diagram draws an edge declared inside a nested container, so
an edge may cross container walls freely. Container chrome and `boundary` boxes are not
obstacles; edges pass through them.

An endpoint that names no rendered shape drops the edge with a build warning:

```console
warning: diagram edge a → ghost: endpoint 'ghost' matches no shape id
```

The build still succeeds. A picture missing an arrow usually means a typo in an id.

## Generated endpoints

The stdlib `Edge` connection is declared `@dynamic`. An endpoint may therefore name an id that a
`wdoc_repeater` or a `wdoc_component` produces at render time, not only a literal block. This is
what makes data-driven ER and class diagrams possible.

```wcl
node_table { id = orders  width = 150.0  title = "orders"
  wdoc_repeater { each = ["id", "user_id"]  as = :c
    node_row { id = $"orders_${c}"  p $"${c}: int" }
  }
}
// `orders_user_id` does not exist in the source, yet it is addressable:
users_id -> orders_user_id :data
```

Without `@dynamic`, an endpoint that resolves to no literal block is a `wcl check` error. That
is the useful default for a hand-written diagram, because it catches typos. A custom connection
schema opts into the loose behaviour explicitly:

```wcl
symbol_set RelKind { default }
@dynamic
connection Ref: &SvgBlock -> &SvgBlock : RelKind
```

Consume a custom connection with `@connections(Ref)` on your own block type.

## Edge kinds

`EdgeKind` is `:default`, `:flow`, `:data`, `:yes`, `:no`.

The kind reaches the output as a `data-kind` attribute on the drawn path:

```html
<polyline points="…" stroke="currentColor" marker-end="url(#wdoc-arrow)" data-kind="flow" />
```

Two consequences an author must know:

- **`:default`, `:flow` and `:data` look identical out of the box.** No bundled theme styles
  them apart. They are a hook, not a built-in appearance. Style them yourself with a `base`
  rule carrying an attribute selector:

  ```wcl
  base "[data-kind=\"data\"]" { css = "stroke-dasharray: 6 3;" }
  ```

- **`:yes` and `:no` also label their edge** with that word. This is how a decision branch names
  its answer, since the `->` grammar has no label of its own:

  ```wcl
  check -> ship   :yes
  check -> cancel :no
  ```

## Ports: where an edge attaches

Every shape exposes anchor points, one at the midpoint of each side. The renderer picks the pair
that suits the two shapes.

`connect_points` restricts the sides a shape offers. The symbols are **`:north`, `:east`,
`:south`, `:west`**.

```wcl
rect { id = db  x = 40.0  y = 40.0  width = 90.0  height = 40.0
       connect_points = [:north, :south] }
```

- The field omitted means all four sides.
- `connect_points = []` empties the list, and the edge then attaches at the shape's centre.
- The renderer drops a symbol outside the set. Several stdlib doc strings say `:left` / `:right` /
  `:top` / `:bottom`. Those names are wrong. A shape given them keeps no anchors at all, and
  behaves like an empty list.
- A round shape (`circle`, and the `node` graph shape) ignores the side midpoints and attaches
  on the circle boundary, along the centre-to-centre line.
- A `node_table` exposes a port per row. A row's default is `[:west, :east]`, so a foreign-key
  edge can target one row rather than the whole table.

Under `:elbow` routing, several edges leaving one shape toward the same side **share a single
anchor**. That draws a branching trunk instead of a fan of near-parallel lines. `:straight`
routing skips it, because converging spokes would cross the shape body.

## Routing

Set routing on the **diagram** (or container). There is no per-edge routing.

| Mode | Behaviour |
| --- | --- |
| `:elbow` | Default. An orthogonal multi-bend polyline that routes around other shapes with A*. |
| `:straight` | One direct line between the two anchors. |

```wcl
diagram { width = 320  height = 200  routing = :straight  layout = :radial  hub = hub
  node "Hub"  { id = hub }
  node "One"  { id = one }
  node "Two"  { id = two }
  hub -> one
  hub -> two
}
```

Use `:straight` with `:radial` and `:force`, where the picture is a graph and the bends of elbow
routing add nothing. Keep `:elbow` for flowcharts and box-and-line architecture diagrams.

`edge_separation` (default 4) is the nudge step that pulls overlapping parallel elbow paths
apart, so two edges over the same route stay legible. It has no effect under `:straight`.

## Arrowheads and labels

Every edge draws the same shared marker at the **destination** end: a filled triangle,
`marker-end="url(#wdoc-arrow)"`. There is no per-edge arrowhead choice, no source-end head, and
no undirected form. For a plain undecorated connector, draw a `line` shape instead — a `line`
has no marker.

A `label` renders as SVG text in the `wdoc-edge-label` class, at the path midpoint. The label
joins the `viewBox` fit, so a label wider than its shapes does not clip. Restyle it by
redeclaring the class:

```wcl
class "wdoc-edge-label" { css = "font-weight: 600;" }
```

## Gotchas

- A shape with no `id` cannot be an endpoint. The stdlib shapes all take `id`; it is optional
  on every one of them.
- Endpoint ids are bare identifiers in a `->` statement (`a -> b`) and **quoted strings** in an
  `edges` record (`source: "a"`).
- `a -> a` in a diagram draws a degenerate zero-length arrow. Diagram edges have no self-loop.
  Use a `state_diagram` transition or a `sequence_diagram` message for that — both draw a real
  loop.
- A dropped edge is a warning, not an error. Read the build output.
- Kind styling is yours to write. Do not expect `:flow` to look different by itself.
