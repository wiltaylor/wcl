# tree

`tree` renders an **indented file-tree**: one row per `tree_node`, indented by depth with the classic file-explorer connector guides drawn between a parent and its children. Each node carries a `title` (its positional label) plus an optional `icon` and `color`, and nodes nest arbitrarily, so it suits a directory layout, a config tree, or any hierarchy. A `tree` is a diagram shape, so it lives inside a `diagram` and is placed by `x` / `y` (or anchors).

An indented file-tree diagram shape — one row per `tree_node`, with file-explorer connector guides between parents and children.

```wcl
diagram {
  width = 360
  height = 220
  tree {
    tree_node "src/" {
      icon = "lucide.folder"
      tree_node "render/" {
        icon = "lucide.folder"
        tree_node "svg.rs" {
          icon = "lucide.file"
        }
        tree_node "html.rs" {
          icon = "lucide.file"
        }
      }
      tree_node "lib.rs" {
        icon = "lucide.file"
      }
      tree_node "tree.rs" {
        icon = "lucide.file"
      }
    }
    tree_node "Cargo.toml" {
      icon = "lucide.file"
    }
  }
}
```

![diagram](../_wdoc/fact_tree-diagram-1.svg)

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | `f64` | no | Top-left x placement in the diagram (or use anchors). |
| `y` | `f64` | no | Top-left y placement in the diagram (or use anchors). |
| `width` | `f64` | no | Tree width (default 280). Height is derived from the node count. |
| `anchor_left` | `f64` | no | Diagram anchor insets (left/right/top/bottom), like any shape. |
| `row_height` | `f64` | no | Height of every node row (default 24). The renderer can't measure content, so rows are fixed. |
| `indent` | `f64` | no | Horizontal indent added per depth level (default 18). |
| `id` | `identifier` | no | Optional explicit HTML id (edge target for the whole tree). |
| `class` | `list<utf8>` | no | Optional style classes (applied to every label). |
| `connect_points` | `list<AnchorSide>` | no | Whole-tree edge-attach sides (default all four). Per-node sides default to west + east. |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `nodes` | `tree_node` | yes | The tree's top-level nodes, top to bottom; each may nest further `node`s. |

One row of a tree; carries a positional `title` label plus an optional `icon` and `color`, and nests further `tree_node`s arbitrarily.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `title` | `utf8` | yes | The node's label (its positional slot), e.g. `node "render/"`. |
| `icon` | `utf8` | no | Optional icon name (a bundled-pack glyph, like `:name:` / the `icon` block). |
| `icon_set` | `utf8` | no | Iconset to draw `icon` from (else the first set that has it). |
| `color` | `utf8` | no | Optional colour for the label + icon (any CSS colour). Themes the row. |
| `id` | `identifier` | no | Node id — the edge target for connecting to this node. |
| `class` | `list<utf8>` | no | Optional style classes (applied to this node's label). |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `children` | `tree_node` | yes | Child nodes nested under this one. |

Icons resolve from any declared [iconset](../references/fact_icons.md); a node's `color` is any CSS colour and themes its label and icon. Give a node an `id` to make it an edge target — an `edge` then attaches to that node's row (west / east), exactly like connecting any other shape.

## Related

- [diagram](../references/fact_diagrams.md)

- [iconset / icon_def / icon](../references/fact_icons.md)

[← Back to SKILL.md](../SKILL.md)
