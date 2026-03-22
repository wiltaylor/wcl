# api_middleware

Middleware applied to an API.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| kind | `string` | yes |  |  | Middleware kind (cors, logging, auth, compression, etc.). |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [api](schemas/api.md)
- **Children**: [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [api](schemas/api.md)
