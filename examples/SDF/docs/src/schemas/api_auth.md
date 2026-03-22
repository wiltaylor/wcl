# api_auth

An authentication scheme for an API.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| scheme | `string` | yes |  |  | Auth scheme (bearer, basic, apiKey, oauth2, openid). |
| token_url | `string` | no |  |  |  |
| authorization_url | `string` | no |  |  |  |
| scopes | `string` | no |  |  |  |
| header_name | `string` | no |  |  |  |
| location | `string` | no |  |  | Where the credential goes (header, query, cookie). |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [api](schemas/api.md)
- **Children**: [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [api](schemas/api.md)
