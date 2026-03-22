# submodule

A git submodule reference.

`leaf`

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| repository | `string` | no |  |  | Submodule repository name. |
| url | `string` | no |  |  | Submodule URL. |
| path | `string` | yes |  |  | Path in the parent repo. |
| branch | `string` | no |  |  |  |

## Relationships

- **Parent**: [repository](schemas/repository.md)
- **Children**: none (leaf)
- **Referenced by**: [repository](schemas/repository.md)
