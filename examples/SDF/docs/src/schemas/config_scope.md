# config_scope

A configuration source with a priority for precedence.

`leaf`

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| priority | `int` | yes |  |  | Priority for precedence ordering. |
| path | `string` | no |  |  | Filesystem path for this config source. |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [configuration](schemas/configuration.md)
- **Children**: none (leaf)
- **Referenced by**: [configuration](schemas/configuration.md)
