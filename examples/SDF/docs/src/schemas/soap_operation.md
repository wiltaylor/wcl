# soap_operation

A SOAP operation.

## Fields

| Field | Type | Required | Default | Constraints | Description |
|-------|------|----------|---------|-------------|-------------|
| soap_action | `string` | no |  |  |  |
| input_message | `string` | no |  |  |  |
| output_message | `string` | no |  |  |  |
| style | `string` | no |  |  | SOAP style (document, rpc). |
| description | `string` | no |  |  |  |

## Relationships

- **Parent**: [soap_service](schemas/soap_service.md)
- **Children**: [behaviour](schemas/behaviour.md), [constraint](schemas/constraint.md)
- **Referenced by**: [soap_service](schemas/soap_service.md)
