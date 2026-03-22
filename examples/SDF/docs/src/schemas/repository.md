# repository

A source code repository.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| type | `string` | no |  |  | VCS type (git, svn, mercurial). |
| visibility | `string` | no |  |  | Repository visibility (public, private, internal). |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: *(root)*
- **Children**: [branching_strategy](schemas/branching_strategy.md), [commit_convention](schemas/commit_convention.md), [pr_workflow](schemas/pr_workflow.md), [system_ref](schemas/system_ref.md), [submodule](schemas/submodule.md), [remote](schemas/remote.md), [issue_labels](schemas/issue_labels.md), [infrastructure](schemas/infrastructure.md), [ignore](schemas/ignore.md), [documentation](schemas/documentation.md)
- **Referenced by**: *(root)*
