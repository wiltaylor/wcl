# plugin_source

A structured source for fetching a plugin from an external location.

`leaf`

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| source_type | `string` | yes |  |  | Source type (github, url, npm, pip). |
| repo | `string` | no |  |  | GitHub owner/repo. |
| git_ref | `string` | no |  |  | Git ref (branch, tag, SHA). |
| url | `string` | no |  |  | Git URL. |
| package | `string` | no |  |  | Package name. |

## Relationships

- **Parent**: [plugin_entry](schemas/plugin_entry.md)
- **Children**: none (leaf)
- **Referenced by**: [plugin_entry](schemas/plugin_entry.md)
