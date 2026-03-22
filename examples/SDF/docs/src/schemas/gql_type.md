# gql_type

A GraphQL type definition.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| kind | `string` | no |  |  | Type kind (type, input, enum, interface, union, scalar). |
| implements | `string` | no |  |  | Interfaces this type implements. |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [api](schemas/api.md)
- **Children**: [gql_field](schemas/gql_field.md), [union_member](schemas/union_member.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [api](schemas/api.md)
