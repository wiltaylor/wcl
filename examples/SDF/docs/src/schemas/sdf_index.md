# sdf_index

A database index for query performance.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| columns | `list(string)` | yes |  |  | Columns included in this index. |
| index_type | `string` | no |  |  | Index type (btree, hash, gin, etc.). |
| unique | `bool` | no | `false` |  | Whether the index enforces uniqueness. |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [sdf_table](schemas/sdf_table.md), [collection](schemas/collection.md)
- **Children**: [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [collection](schemas/collection.md), [sdf_table](schemas/sdf_table.md)
