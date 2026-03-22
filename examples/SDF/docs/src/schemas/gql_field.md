# gql_field

A field on a GraphQL type.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| field_type | `string` | yes |  |  | GraphQL type. |
| deprecated | `bool` | no | `false` |  |  |
| deprecation_reason | `string` | no |  |  |  |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [gql_type](schemas/gql_type.md)
- **Children**: [gql_arg](schemas/gql_arg.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [gql_type](schemas/gql_type.md)
