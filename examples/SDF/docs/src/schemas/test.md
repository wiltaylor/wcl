# test

Testing strategy and framework configuration for a system or component.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| framework | `string` | no |  |  | Test framework (jest, pytest, cargo-test, etc.). |
| strategy | `string` | no |  |  | Testing strategy (unit, integration, e2e, property). |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [system](schemas/system.md), [component](schemas/component.md)
- **Children**: [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [system](schemas/system.md)
