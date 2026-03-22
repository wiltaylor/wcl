# ai_agent

An AI agent — a delegatable sub-agent.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| description | `string` | no |  |  |  |
| system_prompt | `string` | no |  |  |  |
| repository_ref | `string` | no |  |  |  |

## Relationships

- **Parent**: *(root)*, [ai_plugin](schemas/ai_plugin.md)
- **Children**: [trigger](schemas/trigger.md), [tool](schemas/tool.md), [parameter](schemas/parameter.md), [output](schemas/output.md), [data_store](schemas/data_store.md), [agent](schemas/agent.md), [actions](schemas/actions.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: *(root)*, [ai_plugin](schemas/ai_plugin.md)
