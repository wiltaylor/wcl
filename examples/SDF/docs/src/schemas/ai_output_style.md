# ai_output_style

A Claude Code output style — system prompt modification.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| content | `string` | yes |  |  | The system prompt modification text. |
| keep_coding_instructions | `bool` | no | `true` |  |  |
| description | `string` | no |  |  |  |
| repository_ref | `string` | no |  |  |  |

## Relationships

- **Parent**: *(root)*, [ai_plugin](schemas/ai_plugin.md)
- **Children**: [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: *(root)*, [ai_plugin](schemas/ai_plugin.md)
