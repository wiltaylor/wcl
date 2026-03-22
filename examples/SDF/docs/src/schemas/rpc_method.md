# rpc_method

A gRPC method within a service.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| request_type | `string` | yes |  |  | Request message type. |
| response_type | `string` | yes |  |  | Response message type. |
| client_streaming | `bool` | no | `false` |  |  |
| server_streaming | `bool` | no | `false` |  |  |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [rpc_service](schemas/rpc_service.md)
- **Children**: [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [rpc_service](schemas/rpc_service.md)
