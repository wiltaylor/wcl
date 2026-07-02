# Blocks

_A named group of fields that can also nest other blocks; the schema sets its labels and fields._

A block is a named group of [fields](../references/concept_fields.md) that also lets you nest other blocks under it. The available fields and labels are set by the [block schema](../references/concept_block_schema.md). A block kind may also be namespace-qualified (`wdoc::process { ... }`).

```wcl
// One label -> name = "web".
service "web" {
  port   = 8080u32
  region = "us-east-1"
}
```

Multiple labels work the same way — declare an `@inline` for each position you want to expose:

```wcl
// Two labels -> verb = "GET", path = "/users".
route "GET" "/users" {
  handler = "list_users"
}
```

## Nested blocks

Blocks can hold further blocks. Nesting depth is unbounded; structure is constrained by the schema (`@child` for one, `@children` for many).

```wcl
service "web" {
  metadata {
    region = "us-east-1"
    tags {
      environment = "prod"
    }
  }
}
```

## Related

- [Fields](../references/concept_fields.md)

- [Namespaces](../references/concept_namespaces.md)

- [Child-count Constraints](../references/concept_child_count_constraints.md)

- [Document Schema](../references/concept_document_schema.md)

[← Back to SKILL.md](../SKILL.md)
