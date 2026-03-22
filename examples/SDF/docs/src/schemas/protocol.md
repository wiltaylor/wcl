# protocol

A network protocol definition.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| format | `string` | yes |  |  | Encoding format (binary, json, xml, protobuf, msgpack, text). |
| transport | `string` | yes |  |  | Transport layer (tcp, udp, websocket, http, unix-socket). |
| version | `string` | no |  |  |  |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [system](schemas/system.md), [component](schemas/component.md)
- **Children**: [message](schemas/message.md), [sdf_enum](schemas/sdf_enum.md), [endpoint](schemas/endpoint.md), [error_code](schemas/error_code.md), [protocol_flow](schemas/protocol_flow.md), [header](schemas/header.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [component](schemas/component.md), [system](schemas/system.md)
