# resource

A REST resource group (e.g. /users, /sessions).

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| path | `string` | no |  |  | Resource path segment. |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [api](schemas/api.md), [resource](schemas/resource.md)
- **Children**: [operation](schemas/operation.md), [resource](schemas/resource.md), [api_field](schemas/api_field.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [api](schemas/api.md), [resource](schemas/resource.md)
