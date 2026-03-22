# ui

A user interface surface.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| platform | `string` | no |  |  | Target platform (web, mobile, desktop, cli, tui). |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [system](schemas/system.md), [component](schemas/component.md)
- **Children**: [screen](schemas/screen.md), [design_system](schemas/design_system.md), [navigation](schemas/navigation.md), [input_mapping](schemas/input_mapping.md), [ui_asset](schemas/ui_asset.md), [ui_state](schemas/ui_state.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [component](schemas/component.md), [system](schemas/system.md)
