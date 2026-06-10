# Trees

`tree` renders an **indented file-tree**: one row per `tree_node`, indented by depth with ├─ └─ │ connector guides drawn between a parent and its children — the classic file-explorer look. Each node carries a `title` (its positional label) plus an optional `icon` and `color`. Nodes nest arbitrarily, so it suits a directory layout, a config tree, or any hierarchy where the structure is the point.

A `tree` is a diagram shape, so it lives inside a `diagram { … }`, is placed by `x` / `y` (or anchors), and its height is derived from the node count.

## A file tree

![diagram](../_wdoc/wdoc_tree-diagram-1.svg)

```wcl
iconset lucide {}

diagram { width = 360  height = 220
  tree {
    tree_node "src/" {
      icon  = "folder"
      color = "#88c0d0"
      tree_node "render/" {
        icon = "folder"
        tree_node "svg.rs"  { icon = "file" }
        tree_node "html.rs" { icon = "file" }
      }
      tree_node "lib.rs"  { icon = "file" }
      tree_node "tree.rs" { icon = "file" }
    }
    tree_node "Cargo.toml" { icon = "file" }
  }
}
```

Icons resolve from any declared `iconset` (see [Icons](../references/wdoc_icons.md)); a node's `color` is any CSS colour and themes its label + icon. Give a node an `id` to make it an edge target — an `edge` then attaches to that node's row (west / east), exactly like a `node_table` row.

## Connecting to a node

![diagram](../_wdoc/wdoc_tree-diagram-2.svg)

## Fields

### Tree



### TreeNode


