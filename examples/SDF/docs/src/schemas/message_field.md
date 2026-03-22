# message_field

A field within a protocol message.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| field_type | `string` | yes |  |  | Logical type of the field. |
| position | `string` | no |  |  | Byte offset or field order. |
| size | `string` | no |  |  | Bit or byte width. |
| encoding | `string` | no |  |  | Encoding (base64, hex, utf-8). |
| required | `bool` | no |  |  |  |
| default | `string` | no |  |  |  |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [message](schemas/message.md), [header](schemas/header.md), [message_field](schemas/message_field.md)
- **Children**: [message_field](schemas/message_field.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [header](schemas/header.md), [message](schemas/message.md), [message_field](schemas/message_field.md)
