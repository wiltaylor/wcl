# column

A column in a database table.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| col_type | `string` | yes |  |  | SQL or storage type. |
| nullable | `bool` | no | `false` |  |  |
| default_value | `string` | no |  |  | Default expression. |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [sdf_table](schemas/sdf_table.md)
- **Children**: [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [sdf_table](schemas/sdf_table.md)
