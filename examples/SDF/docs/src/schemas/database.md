# database

A database engine backing the system.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| kind | `string` | no |  |  | Storage paradigm (relational, document, graph, key_value, vector). |
| engine | `string` | no |  |  | Database engine name (e.g. postgresql, mongodb). |
| version | `string` | no |  |  | Engine version. |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [system](schemas/system.md), [component](schemas/component.md)
- **Children**: [sdf_table](schemas/sdf_table.md), [collection](schemas/collection.md), [node_label](schemas/node_label.md), [edge_type](schemas/edge_type.md), [vector_collection](schemas/vector_collection.md), [reference_data](schemas/reference_data.md), [routine](schemas/routine.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [component](schemas/component.md), [system](schemas/system.md)
