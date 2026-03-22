# rate_limit

Rate limiting rules for an API.

`leaf`

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| limit | `string` | no |  |  | Number of requests allowed. |
| window | `string` | no |  |  | Time window (e.g. 1m, 1h). |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [api](schemas/api.md)
- **Children**: none (leaf)
- **Referenced by**: [api](schemas/api.md)
