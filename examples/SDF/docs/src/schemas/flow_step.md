# flow_step

A single step in a protocol flow.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| message_ref | `string` | yes |  |  | Reference to a Message. |
| direction | `string` | yes |  |  | Direction of this step. |
| condition | `string` | no |  |  | Guard condition. |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [protocol_flow](schemas/protocol_flow.md)
- **Children**: [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [protocol_flow](schemas/protocol_flow.md)
