# library

A top-level block that declares an installed library.

`leaf`

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| git_hash | `string` | no |  |  | Git hash pinning the library version. |
| path | `string` | yes |  |  | Filesystem path to the library. |
| git_url | `string` | yes |  |  | Git remote URL for the library. |

## Relationships

- **Parent**: *(root)*
- **Children**: none (leaf)
- **Referenced by**: *(root)*
