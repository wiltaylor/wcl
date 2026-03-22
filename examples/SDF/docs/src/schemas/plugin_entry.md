# plugin_entry

A plugin entry in a marketplace.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| source | `string` | no |  |  | Relative path to the plugin. |
| version | `string` | no |  |  |  |
| category | `string` | no |  |  |  |
| strict | `bool` | no | `true` |  |  |
| license | `string` | no |  |  |  |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [ai_marketplace](schemas/ai_marketplace.md)
- **Children**: [plugin_source](schemas/plugin_source.md), [author](schemas/author.md)
- **Referenced by**: [ai_marketplace](schemas/ai_marketplace.md)
