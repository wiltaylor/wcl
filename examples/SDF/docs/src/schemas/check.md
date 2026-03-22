# check

A validation rule enforced at one or more lifecycle phases.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| phase | `list(string)` | yes |  |  | Lifecycle phases when this check runs. |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [spec_update_checks](schemas/spec_update_checks.md)
- **Children**: [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [spec_update_checks](schemas/spec_update_checks.md)
