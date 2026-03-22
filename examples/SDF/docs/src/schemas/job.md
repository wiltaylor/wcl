# job

A job within a CI/CD pipeline.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| runs_on | `string` | no |  |  | Runner environment. |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [pipeline](schemas/pipeline.md)
- **Children**: [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [pipeline](schemas/pipeline.md)
