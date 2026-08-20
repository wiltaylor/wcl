# Trees, node tables and cards

Three drawn blocks that carry **structure or rich content** instead of one primitive shape:

| Block | Draws | Use it for |
| --- | --- | --- |
| `tree` + `tree_node` | An indented file-explorer view with connector guides | Directory layouts, config trees, any hierarchy |
| `node_table` + `node_row` | A titled header over a stack of rows, each row its own port | Database / ER tables, UML class diagrams |
| `card` | A box whose body is ordinary wdoc content | Notes on a canvas, rich timeline events |

All three are **diagram shapes**. Each one `extends SvgBlock`, so it is a legal child of a
`diagram` or a `container`, and nothing else. A `tree` written straight into a page body is a
schema error. Wrap it:

```wcl
page arch {
  diagram {
    width = 360  height = 220
    tree { … }
  }
}
```

All three are **native** blocks: wdoc draws them in Rust. They declare no `lower` function, so
there is none to read or override.

## Shared placement fields

Each of the three carries the same placement set as `rect` or `circle`:

| Field | Type | Meaning |
| --- | --- | --- |
| `x`, `y` | `f64` | Top-left placement in the parent diagram (default `0.0`). |
| `anchor_left`, `anchor_right`, `anchor_top`, `anchor_bottom` | `f64` | Fractional (0–1) anchor insets, instead of `x` / `y`. |
| `id` | `identifier` | Edge-connection name for the whole shape. |
| `class` | `list<utf8>` | Style classes. |
| `connect_points` | `list<AnchorSide>` | Sides an edge attaches to. `AnchorSide` is `:north :east :south :west`; omit it for all four. |

Under an auto-layout diagram (`layout = :layered` / `:force` / `:grid`), omit `x` / `y` and let
the solver place the shape.

## `tree`

One row per node, indented by depth, with `├─ └─ │` guides drawn between a parent and its
children. Nodes nest as deep as you like.

```wcl
diagram {
  width = 360  height = 220
  tree {
    tree_node "src/" {
      icon = "lucide.folder"
      tree_node "render/" {
        icon = "lucide.folder"
        tree_node "svg.rs"  { icon = "lucide.file" }
        tree_node "html.rs" { icon = "lucide.file" }
      }
      tree_node "lib.rs"  { icon = "lucide.file" }
      tree_node "tree.rs" { icon = "lucide.file" }
    }
    tree_node "Cargo.toml" { icon = "lucide.file" }
  }
}
```

`tree` fields, beyond the shared placement set:

| Field | Type | Meaning |
| --- | --- | --- |
| `width` | `f64` | Tree width (default `280`). The **height is derived** from the node count — you do not set it. |
| `row_height` | `f64` | Height of every row (default `24`). Fixed: the renderer cannot measure a row's content. |
| `indent` | `f64` | Horizontal indent added per depth level (default `18`). |
| `nodes` | `@children("tree_node")` | The top-level nodes, top to bottom. |

`tree_node` fields:

| Field | Type | Meaning |
| --- | --- | --- |
| `title` | `utf8` | The row label. It is `@inline(0)`, so you write `tree_node "render/" { … }`. |
| `icon` | `utf8` | An icon name in `pack.name` form, for example `"lucide.file"`. |
| `icon_set` | `utf8` | Which iconset to draw `icon` from. Otherwise the first set that has it wins. |
| `color` | `utf8` | Any CSS colour. It themes that one row's label and icon. |
| `id` | `identifier` | Makes this row an edge target. |
| `class` | `list<utf8>` | Style classes on this row's label. |
| `children` | `@children("tree_node")` | Nested nodes. |

**A node is not a shape of its own.** The parent `tree` draws every row. A node still becomes an
edge target when you give it an `id`, and the edge lands on that row's west or east side:

```wcl
diagram {
  width = 420  height = 160
  tree {
    tree_node "config/" { tree_node "app.wcl" { id = appcfg } }
  }
  rect { id = loader  x = 260.0  y = 40.0  width = 120.0  height = 40.0 }
  appcfg -> loader
}
```

Two style classes carry the defaults: `wdoc-tree-guide` (the connector lines) and
`wdoc-tree-label` (the row text). Redeclare either one to restyle every tree.

## `node_table`

A header over a stack of rows. Each row holds real wdoc content and exposes its own connection
ports, so a foreign key points at one column instead of at the whole box.

```wcl
diagram {
  width = 420  height = 170
  routing = :straight
  node_table {
    id = users  x = 20.0  y = 20.0  width = 150.0  title = "users"
    node_row { id = users_id    p "id: int" }
    node_row { id = users_email p "email: text" }
  }
  node_table {
    id = orders  x = 250.0  y = 20.0  width = 150.0  title = "orders"
    node_row { id = orders_id      p "id: int" }
    node_row { id = orders_user_id p "user_id: int" }
  }
  orders_user_id -> users_id :data
}
```

`node_table` fields, beyond the shared placement set:

| Field | Type | Meaning |
| --- | --- | --- |
| `width` | `f64` | Table width (default `200`). The **height is derived** from the rows. |
| `title` | `utf8` | Header caption. Omit it for a header-less table. |
| `header_height` | `f64` | Header height when `title` is set (default `28`). |
| `row_height` | `f64` | Height of every row (default `30`). Fixed: the renderer cannot measure a row's content. |
| `rows` | `@children("node_row")` | The rows, top to bottom. |

`node_row` fields:

| Field | Type | Meaning |
| --- | --- | --- |
| `id` | `identifier` | Row id. This is the edge target (`orders_user_id -> users_id`). |
| `class` | `list<utf8>` | Style classes on the row content. |
| `connect_points` | `list<AnchorSide>` | Sides this row exposes a port on. Default `[:west, :east]`. |
| `body` | `@children(ContentBlock)` | The row's content — `p`, `code`, `list`, and so on. |

Style classes: `wdoc-node-table-frame`, `wdoc-node-table-sep`, `wdoc-node-table-port`,
`wdoc-node-table-title` and `wdoc-node-row`.

## `card`

A box whose body is ordinary page content: paragraphs with inline formatting, lists, code,
callouts, even a nested diagram. wdoc draws the body as HTML inside an SVG `<foreignObject>`.

```wcl
diagram {
  width = 260  height = 110
  card {
    x = 20.0  y = 15.0  width = 220.0  height = 80.0
    title = "Note"
    p "Rich **text** inside a diagram."
  }
}
```

`card` fields, beyond the shared placement set:

| Field | Type | Meaning |
| --- | --- | --- |
| `width`, `height` | `f64` | Box size (defaults `160` × `90`). A card does **not** grow to fit its body. |
| `title` | `utf8` | Optional plain-text heading above the body. |
| `body` | `@children(ContentBlock)` | The card's content. |
| `on` | `utf8` | ISO date. Read **only** when the card is a `timeline` child. |
| `side` | `symbol` | `:near` / `:far` / `:auto`. Read only as a `timeline` child. |

A `card` doubles as a timeline's rich event item. Written under a `timeline` it needs no `x` /
`y` — the timeline reads `on` and places the card on the axis:

```wcl
diagram {
  width = 520  height = 220
  timeline {
    width = 480.0  height = 200.0
    start = "2026-01-01"  end = "2026-06-30"
    card { on = "2026-03-14"  title = "Beta"
      p "Feature freeze, then the public beta."
    }
  }
}
```

Style classes: `wdoc-card` (the box) and `wdoc-card-title`.

## Gotchas

- **All three need a `diagram` or `container` parent.** They are SVG shapes, not page blocks.
- **Rows never auto-size.** `tree.row_height` and `node_table.row_height` are fixed, because the
  renderer draws SVG and cannot measure wrapped content. Content taller than the row is clipped.
  Raise `row_height`, or shorten the content.
- **A card does not grow either.** Set `width` and `height` to fit the body you wrote.
- **Connect to the part, not the box.** Give a `tree_node` or a `node_row` an `id`, and the edge
  attaches to that row. Without an `id` you can address only the whole shape.
- **`connect_points` takes compass names** (`:north :east :south :west`), not `:left` / `:right`.
- **A `card` body is page content.** It accepts any `ContentBlock`, so a nested `code` or `list`
  is legal — but the card box is fixed, so long content is clipped rather than reflowed.
- **Themes recolour these blocks through the class system.** The bare-class defaults above carry
  neutral colours, so a document with no `site` theme still renders readable shapes.
