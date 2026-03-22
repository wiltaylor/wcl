# message

A named message, packet, or frame in a protocol.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| direction | `string` | yes |  |  | Message direction (request, response, notification, bidirectional). |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [protocol](schemas/protocol.md)
- **Children**: [message_field](schemas/message_field.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md), [test_hint](schemas/test_hint.md), [test_exclusion](schemas/test_exclusion.md), [config_ref](schemas/config_ref.md)
- **Referenced by**: [protocol](schemas/protocol.md)
