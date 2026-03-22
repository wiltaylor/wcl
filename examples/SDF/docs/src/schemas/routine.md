# routine

A server-side programmable object (stored procedure, function, or trigger).

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| language | `string` | no |  |  | Implementation language (plpgsql, sql, etc.). |
| return_type | `string` | no |  |  | Return type. |
| timing | `string` | no |  |  | Trigger timing (BEFORE, AFTER, INSTEAD OF). |
| event | `string` | no |  |  | Trigger event (INSERT, UPDATE, DELETE). |
| target_table | `string` | no |  |  | Table the trigger is attached to. |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [database](schemas/database.md)
- **Children**: [routine_param](schemas/routine_param.md), [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [database](schemas/database.md)
