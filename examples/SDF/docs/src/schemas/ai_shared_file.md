# ai_shared_file

A file within a shared AI folder.

`leaf`

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| path | `string` | yes |  |  | File path relative to the folder. |
| content | `string` | no |  |  | Inline file content. |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [ai_shared_folder](schemas/ai_shared_folder.md)
- **Children**: none (leaf)
- **Referenced by**: [ai_shared_folder](schemas/ai_shared_folder.md)
