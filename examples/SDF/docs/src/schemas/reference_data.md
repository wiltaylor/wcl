# reference_data

A named set of reference/seed data for a table.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| table_ref | `string` | yes |  |  | Name of the target table. |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [database](schemas/database.md)
- **Children**: [row](schemas/row.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [database](schemas/database.md)
