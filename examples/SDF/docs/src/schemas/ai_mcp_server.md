# ai_mcp_server

A Model Context Protocol (MCP) server.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| transport | `string` | no |  |  | Transport type (stdio, sse, http). |
| command | `string` | no |  |  | Command to start the server. |
| cwd | `string` | no |  |  | Working directory. |
| url | `string` | no |  |  | URL for HTTP-based servers. |
| description | `string` | no |  |  |  |
| repository_ref | `string` | no |  |  |  |

## Relationships

- **Parent**: *(root)*, [ai_plugin](schemas/ai_plugin.md)
- **Children**: [oauth](schemas/oauth.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: *(root)*, [ai_plugin](schemas/ai_plugin.md)
