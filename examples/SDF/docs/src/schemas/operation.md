# operation

An HTTP operation on a resource.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| method | `string` | no |  |  | HTTP method (GET, POST, PUT, PATCH, DELETE). |
| summary | `string` | no |  |  | Brief operation summary. |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [resource](schemas/resource.md), [api](schemas/api.md)
- **Children**: [request_body](schemas/request_body.md), [response](schemas/response.md), [path_param](schemas/path_param.md), [query_param](schemas/query_param.md), [header_param](schemas/header_param.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [api](schemas/api.md), [resource](schemas/resource.md)
