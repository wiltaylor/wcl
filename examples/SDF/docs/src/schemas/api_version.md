# api_version

API versioning strategy.

`leaf`

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| strategy | `string` | yes |  |  | Versioning strategy (path, header, query). |
| current | `string` | no |  |  | Current version. |
| deprecated | `string` | no |  |  | Deprecated version(s). |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [api](schemas/api.md)
- **Children**: none (leaf)
- **Referenced by**: [api](schemas/api.md)
