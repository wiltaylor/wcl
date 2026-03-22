# switch

A CLI flag or option.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| long_flag | `string` | no |  |  | Long flag name (e.g. --output). |
| short_flag | `string` | no |  |  | Short flag (e.g. -o). |
| default | `string` | no |  |  | Default value. |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [cli](schemas/cli.md), [sub_command](schemas/sub_command.md)
- **Children**: [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [cli](schemas/cli.md), [sub_command](schemas/sub_command.md)
