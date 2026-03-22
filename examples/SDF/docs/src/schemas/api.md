# api

An API endpoint group.

`tagged(style)`

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| style | `string` | no |  |  | API architectural style (rest, graphql, grpc, soap, jsonrpc, odata). |
| version | `string` | no |  |  | API version identifier. |
| base_path | `string` | no |  |  | URL path prefix. |
| description | `string` | no |  |  |  |

## Variants

### Variant: `rest`

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| base_path | `string` | no |  |  | URL path prefix for all REST resources. |

### Variant: `graphql`

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| schema_path | `string` | no |  |  | Path to the GraphQL schema file. |

### Variant: `grpc`

### Variant: `soap`

### Variant: `jsonrpc`

### Variant: `odata`

## Relationships

- **Parent**: [system](schemas/system.md), [component](schemas/component.md)
- **Children**: [resource](schemas/resource.md), [gql_type](schemas/gql_type.md), [gql_query](schemas/gql_query.md), [gql_mutation](schemas/gql_mutation.md), [gql_subscription](schemas/gql_subscription.md), [rpc_service](schemas/rpc_service.md), [soap_service](schemas/soap_service.md), [rpc_namespace](schemas/rpc_namespace.md), [odata_entity](schemas/odata_entity.md), [operation](schemas/operation.md), [api_field](schemas/api_field.md), [api_auth](schemas/api_auth.md), [rate_limit](schemas/rate_limit.md), [api_middleware](schemas/api_middleware.md), [api_version](schemas/api_version.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [component](schemas/component.md), [system](schemas/system.md)
