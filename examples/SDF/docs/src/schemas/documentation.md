# documentation

A documentation set within a repository.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| path | `string` | no |  |  | Root path of the documentation. |
| format | `string` | no |  |  | Documentation format (mdbook, docusaurus, sphinx, etc.). |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [repository](schemas/repository.md)
- **Children**: [doc_file](schemas/doc_file.md), [doc_folder](schemas/doc_folder.md)
- **Referenced by**: [repository](schemas/repository.md)
