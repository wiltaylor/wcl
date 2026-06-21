# Schema decorators

Decorators (written `@name` or `@name(args)`) attach schema metadata to declarations and fields, describing the document's legal structure that `wcl check` validates.

| Decorator | Purpose |
| --- | --- |
| `@document` | Marks the type as the document root schema (composes per namespace — many merge) |
| `@block("kind")` | Makes the type a nestable block of that kind |
| `@child("kind")` | A field holding a single nested block of the given kind |
| `@children("kind")` | A field holding a list of nested blocks (or union variants — dispatch) |
| `@inline(slot)` | Binds the block's positional label to a field |
| `@default(expr)` | Default value used when the field is omitted |
| `@table("kind")` | Marks the row schema for pipe-table syntax |
| `@decorator("name")` | Makes the type the schema for a user-defined `@name` decorator's arguments |
| `@doc` | Carries documentation metadata attached to a declaration |
| `@by_ref` | A block of this kind reifies to a resolvable reference instead of inlining (carried on a record, rendered elsewhere) |
| `@only` / `@except` | Restrict which kinds or fields a slot accepts (include-list / exclude-list) |
| `@connections(S)` | Accumulates `->` connection statements on the field as records of schema `S` |

## Related

- [Schema & Decorators](../references/concept_schema.md)

- [Fields & Blocks](../references/concept_fields_blocks.md)

- [Connections](../references/concept_connections.md)

[← All facts](../references/facts_ref.md)
