# config_item

An individual configuration setting.

`leaf`

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| item_type | `string` | no |  |  | Value type (string, int, bool, etc.). |
| default | `string` | no |  |  | Default value. |
| valid_values | `string` | no |  |  | Allowed values. |
| scopes | `string` | no |  |  | Which config scopes this item appears in. |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [configuration](schemas/configuration.md)
- **Children**: none (leaf)
- **Referenced by**: [configuration](schemas/configuration.md)
