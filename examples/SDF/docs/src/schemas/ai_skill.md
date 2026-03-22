# ai_skill

An AI skill — a composable, reusable capability powered by AI.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| description | `string` | no |  |  |  |
| user_invocable | `bool` | no | `false` |  |  |
| repository_ref | `string` | no |  |  | Repository block this skill belongs to. |

## Relationships

- **Parent**: *(root)*, [ai_plugin](schemas/ai_plugin.md)
- **Children**: [trigger](schemas/trigger.md), [tool](schemas/tool.md), [parameter](schemas/parameter.md), [output](schemas/output.md), [data_store](schemas/data_store.md), [agent](schemas/agent.md), [procedure](schemas/procedure.md), [actions](schemas/actions.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: *(root)*, [ai_plugin](schemas/ai_plugin.md)
