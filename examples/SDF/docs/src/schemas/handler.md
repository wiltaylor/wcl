# handler

A handler that executes when a hook fires.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| handler_type | `string` | yes |  |  | Handler type (command, prompt, agent). |
| command | `string` | no |  |  | Shell command to execute. |
| prompt | `string` | no |  |  | Prompt text. |
| timeout | `int` | no |  |  | Timeout in milliseconds. |

## Relationships

- **Parent**: [ai_hook](schemas/ai_hook.md)
- **Children**: [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [ai_hook](schemas/ai_hook.md)
