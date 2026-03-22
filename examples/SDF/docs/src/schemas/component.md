# component

A logical component within a system or module.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| description | `string` | no |  |  | What this component is responsible for. |

## Relationships

- **Parent**: [system](schemas/system.md), [module](schemas/module.md)
- **Children**: [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md), [test_hint](schemas/test_hint.md), [test_exclusion](schemas/test_exclusion.md), [config_ref](schemas/config_ref.md), [implementation](schemas/implementation.md), [cli](schemas/cli.md), [security](schemas/security.md), [configuration](schemas/configuration.md), [interface](schemas/interface.md), [database](schemas/database.md), [api](schemas/api.md), [protocol](schemas/protocol.md), [ui](schemas/ui.md), [sdf_type](schemas/sdf_type.md), [file_format](schemas/file_format.md), [actions](schemas/actions.md), [external_spec](schemas/external_spec.md)
- **Referenced by**: [module](schemas/module.md), [system](schemas/system.md)
