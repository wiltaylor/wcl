# pipeline

A CI/CD pipeline workflow.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| platform | `string` | yes |  |  | CI platform (github-actions, gitlab-ci, jenkins, etc.). |
| path | `string` | no |  |  | Path to the pipeline file. |
| triggers | `string` | no |  |  | Events that trigger the pipeline. |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [infrastructure](schemas/infrastructure.md)
- **Children**: [job](schemas/job.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [infrastructure](schemas/infrastructure.md)
