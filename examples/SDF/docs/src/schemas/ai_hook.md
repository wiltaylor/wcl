# ai_hook

A Claude Code hook — an event-driven lifecycle handler.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| event | `string` | yes |  |  | Lifecycle event (PreToolUse, PostToolUse, SessionStart, etc.). |
| matcher | `string` | no |  |  | Pattern to match against the event payload. |
| description | `string` | no |  |  |  |
| repository_ref | `string` | no |  |  |  |

## Relationships

- **Parent**: *(root)*, [ai_plugin](schemas/ai_plugin.md)
- **Children**: [handler](schemas/handler.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: *(root)*, [ai_plugin](schemas/ai_plugin.md)
