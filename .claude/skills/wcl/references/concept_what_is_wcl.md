# What WCL Is

_A typed configuration & schema language: declare types, compose a document, validate and evaluate it._

WCL is a typed configuration & schema language: you declare record types, compose a document of block instances, and the toolchain validates and evaluates it.

A WCL document is built from two structural pieces — [fields](../references/concept_fields.md) that bind names to values and [blocks](../references/concept_blocks.md) that group and nest them — and a schema (written with decorators) that says which blocks and fields are legal. `wcl check` validates a document against that schema, and `wcl eval` resolves it to plain data.

Where most config formats stop at untyped key/value data, WCL adds a type system (numbers with widths, strings with encodings, unions, interfaces, references), first-class functions, and a document model that gathers and validates structured blocks — so a configuration can carry its own schema and be checked before anything consumes it.

## Related

- [Fields](../references/concept_fields.md)

- [Blocks](../references/concept_blocks.md)

- [Records](../references/concept_records.md)

- [Document Schema](../references/concept_document_schema.md)

[← Back to SKILL.md](../SKILL.md)
