# transition

A transition between states in a state machine.

`leaf`

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| trigger | `string` | yes |  |  | The event that triggers this transition. |
| source | `string` | yes |  |  | Source state name. |
| target | `string` | yes |  |  | Target state name. |
| guard | `string` | no |  |  | Guard condition. |

## Relationships

- **Parent**: [state_machine](schemas/state_machine.md)
- **Children**: none (leaf)
- **Referenced by**: [state_machine](schemas/state_machine.md)
