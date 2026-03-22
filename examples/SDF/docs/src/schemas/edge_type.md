# edge_type

A relationship or edge type in a graph database.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| source | `string` | yes |  |  | Source node label. |
| target | `string` | yes |  |  | Target node label. |
| directed | `bool` | no | `true` |  |  |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [database](schemas/database.md)
- **Children**: [property](schemas/property.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [database](schemas/database.md)
