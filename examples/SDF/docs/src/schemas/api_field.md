# api_field

A field in an API request or response body.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| field_type | `string` | yes |  |  | Field type. |
| required | `bool` | no |  |  |  |
| default | `string` | no |  |  |  |
| format | `string` | no |  |  | Format hint (e.g. date-time, email, uuid). |
| field_validation | `string` | no |  |  | Validation expression. |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [api](schemas/api.md), [resource](schemas/resource.md), [request_body](schemas/request_body.md), [response](schemas/response.md), [rpc_call](schemas/rpc_call.md), [odata_entity](schemas/odata_entity.md), [api_field](schemas/api_field.md)
- **Children**: [api_field](schemas/api_field.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [api](schemas/api.md), [api_field](schemas/api_field.md), [odata_entity](schemas/odata_entity.md), [request_body](schemas/request_body.md), [resource](schemas/resource.md), [response](schemas/response.md), [rpc_call](schemas/rpc_call.md)
