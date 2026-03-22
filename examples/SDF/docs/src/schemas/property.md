# property

A property on a graph node or edge.

`leaf`

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| prop_type | `string` | yes |  |  | Property type. |
| indexed | `bool` | no | `false` |  |  |
| unique | `bool` | no | `false` |  |  |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [node_label](schemas/node_label.md), [edge_type](schemas/edge_type.md)
- **Children**: none (leaf)
- **Referenced by**: [edge_type](schemas/edge_type.md), [node_label](schemas/node_label.md)
