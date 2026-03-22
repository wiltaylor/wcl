# change

A change request — either a feature or a bug fix.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| change_type | `string` | no |  |  | Change type (feature, bug). |
| status | `string` | no |  |  | Change status (pending, in_progress, done). |
| description | `string` | no |  |  |  |
| rationale | `string` | no |  |  |  |
| integration | `string` | no |  |  |  |
| expected | `string` | no |  |  |  |
| actual | `string` | no |  |  |  |

## Relationships

- **Parent**: [system](schemas/system.md)
- **Children**: [constraint](schemas/constraint.md), [affected_item](schemas/affected_item.md), [actions](schemas/actions.md)
- **Referenced by**: [system](schemas/system.md)
