# test_exclusion

Marks something as explicitly excluded from testing.

`leaf`

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| reason | `string` | yes |  |  | Why this is excluded from testing. |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md), [component](schemas/component.md)
- **Children**: none (leaf)
- **Referenced by**: [behaviour](schemas/behaviour.md), [component](schemas/component.md), [constraint](schemas/constraint.md), [message](schemas/message.md)
