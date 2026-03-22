# interface

A contract between parts of the system.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| interface_type | `string` | no |  |  | Interface kind (sync, async, event). |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [system](schemas/system.md), [component](schemas/component.md)
- **Children**: [method](schemas/method.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [component](schemas/component.md), [system](schemas/system.md)
