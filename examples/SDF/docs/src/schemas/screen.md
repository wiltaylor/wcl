# screen

A screen or page within a user interface.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| route | `string` | no |  |  | URL route or path. |
| entry_point | `bool` | no | `false` |  | Whether this is the initial screen. |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [ui](schemas/ui.md)
- **Children**: [ui_component](schemas/ui_component.md), [layout](schemas/layout.md), [layer](schemas/layer.md), [breakpoint](schemas/breakpoint.md), [animation](schemas/animation.md), [ui_asset](schemas/ui_asset.md), [ui_state](schemas/ui_state.md), [style](schemas/style.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [ui](schemas/ui.md)
