# endpoint

A named endpoint or operation in a protocol.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| method | `string` | no |  |  |  |
| path | `string` | no |  |  |  |
| request_ref | `string` | no |  |  | Reference to request message. |
| response_ref | `string` | no |  |  | Reference to response message. |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [protocol](schemas/protocol.md)
- **Children**: [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [protocol](schemas/protocol.md)
