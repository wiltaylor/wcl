# ai_shared_folder

A shared resource folder accessible to AI skills and agents.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| path | `string` | yes |  |  | Folder path. |
| description | `string` | no |  |  |  |
| repository_ref | `string` | no |  |  |  |

## Relationships

- **Parent**: *(root)*, [ai_plugin](schemas/ai_plugin.md)
- **Children**: [ai_shared_file](schemas/ai_shared_file.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: *(root)*, [ai_plugin](schemas/ai_plugin.md)
