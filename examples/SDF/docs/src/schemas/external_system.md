# external_system

An external system or third-party dependency.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| description | `string` | no |  |  | What this external system provides. |
| endpoints | `string` | no |  |  | Connection details or endpoint URLs. |
| auth | `string` | no |  |  | Authentication method. |
| data_formats | `string` | no |  |  | Data formats (JSON, XML, Protobuf, etc.). |
| rate_limits | `string` | no |  |  | Rate limits imposed by the external system. |
| implements | `string` | no |  |  | Name of an Interface this system implements. |

## Relationships

- **Parent**: *(root)*
- **Children**: [external_spec](schemas/external_spec.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: *(root)*
