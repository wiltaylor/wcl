# parameter

A typed input parameter for a skill or agent.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| param_type | `string` | yes |  |  | Parameter type. |
| required | `bool` | no | `true` |  |  |
| default | `string` | no |  |  |  |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [ai_skill](schemas/ai_skill.md), [ai_agent](schemas/ai_agent.md)
- **Children**: [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [ai_agent](schemas/ai_agent.md), [ai_skill](schemas/ai_skill.md)
