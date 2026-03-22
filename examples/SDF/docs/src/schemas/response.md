# response

A response for an API operation.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| status_code | `string` | yes |  |  | HTTP status code. |
| content_type | `string` | no |  |  | Response MIME type. |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [operation](schemas/operation.md)
- **Children**: [api_field](schemas/api_field.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [operation](schemas/operation.md)
