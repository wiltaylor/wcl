# foreign_key

A foreign key constraint referencing another table.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| columns | `list(string)` | yes |  |  | Source columns in this table. |
| referenced_table | `string` | yes |  |  | Referenced table name. |
| referenced_columns | `list(string)` | yes |  |  | Referenced column names. |
| on_delete | `string` | no |  |  | ON DELETE action. |
| on_update | `string` | no |  |  | ON UPDATE action. |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [sdf_table](schemas/sdf_table.md)
- **Children**: [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [sdf_table](schemas/sdf_table.md)
