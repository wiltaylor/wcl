# Block Schema

_Declaring nestable blocks with @block, @inline, @child/@children, and @default._

`@block("kind")` makes a type a nestable block of that kind. `@inline(slot)` binds the block label to a field at that position, `@child("kind")` / `@children("kind")` declare nested blocks, and `@default(expr)` supplies a value when a field is omitted. The block label maps to the `@inline` field — `service "web"` sets `name = "web"`.

## A worked example

A document root with one kind of child block, whose label becomes an inline field and whose port has a default:

```wcl
@block("service")
type Service {
  @inline(0)   name: utf8     // service "web" -> name = "web"
  @default(80) port: u32
  region: utf8
}

@document
type Config {
  @children("service") services: list<Service>
}
```

Given that schema, this document validates: each `service` block contributes a `Service`, with `name` from the label and `port` defaulting to `80`.

```wcl
service "web" { region = "us-east-1" }
service "api" { port = 9090u32  region = "eu-west-1" }
```

## Related

- [Document Schema](../references/concept_document_schema.md)

- [Referential Integrity](../references/concept_ref_integrity.md)

- [Child-count Constraints](../references/concept_child_count_constraints.md)

- [Fields](../references/concept_fields.md)

- [Blocks](../references/concept_blocks.md)

- [Records](../references/concept_records.md)

[← Back to SKILL.md](../SKILL.md)
