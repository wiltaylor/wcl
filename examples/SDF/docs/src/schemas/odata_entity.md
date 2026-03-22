# odata_entity

An OData entity set.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| entity_type | `string` | no |  |  | OData entity type. |
| path | `string` | no |  |  | Entity set path. |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [api](schemas/api.md)
- **Children**: [api_field](schemas/api_field.md), [navigation_property](schemas/navigation_property.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [api](schemas/api.md)
