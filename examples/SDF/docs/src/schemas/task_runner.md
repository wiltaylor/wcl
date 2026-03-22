# task_runner

A task runner file and its public recipes.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| runner_tool | `string` | yes |  |  | Task runner tool (just, make, npm, etc.). |
| path | `string` | no |  |  | Path to the task runner file. |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [infrastructure](schemas/infrastructure.md)
- **Children**: [recipe](schemas/recipe.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [infrastructure](schemas/infrastructure.md)
