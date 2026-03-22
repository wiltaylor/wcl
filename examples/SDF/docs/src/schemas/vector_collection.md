# vector_collection

A vector collection or index for similarity search.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| dimensions | `int` | yes |  |  | Vector dimensionality. |
| distance_metric | `string` | yes |  |  | Distance metric (cosine, euclidean, dot_product). |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [database](schemas/database.md)
- **Children**: [vector_field](schemas/vector_field.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [database](schemas/database.md)
