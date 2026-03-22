# output

A typed output produced by a skill or agent.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| output_type | `string` | yes |  |  | Output type. |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [ai_skill](schemas/ai_skill.md), [ai_agent](schemas/ai_agent.md)
- **Children**: [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [ai_agent](schemas/ai_agent.md), [ai_skill](schemas/ai_skill.md)
