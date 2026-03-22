# system

A deployable system or application.

`open`

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| version | `string` | no |  |  | Semantic version of the system. |
| system_type | `string` | no |  |  | Architecture style (service, library, cli, monolith). |
| description | `string` | no |  |  | Brief description of what this system does. |

## Relationships

- **Parent**: *(root)*
- **Children**: [component](schemas/component.md), [sdf_type](schemas/sdf_type.md), [database](schemas/database.md), [api](schemas/api.md), [protocol](schemas/protocol.md), [ui](schemas/ui.md), [file_format](schemas/file_format.md), [security](schemas/security.md), [security_policy](schemas/security_policy.md), [interface](schemas/interface.md), [external_spec](schemas/external_spec.md), [configuration](schemas/configuration.md), [test](schemas/test.md), [change](schemas/change.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: *(root)*
