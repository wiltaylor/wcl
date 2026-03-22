# threat_model

A known threat and its mitigations.

`leaf`

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| severity | `string` | no |  |  | Threat severity (low, medium, high, critical). |
| mitigation | `string` | no |  |  | Mitigation strategy. |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [security](schemas/security.md)
- **Children**: none (leaf)
- **Referenced by**: [security](schemas/security.md)
