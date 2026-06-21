# Fields & Blocks

_The structural backbone — fields bind names to values, blocks group and nest them._

Fields and blocks are the structural backbone of a document. Everything else — types, functions, schema — describes and constrains them.

## Fields

A field binds a name to a value with `=`. The value is any [expression](../references/concept_expressions.md).

```wcl
name    = "alpha"
count   = 3u32
enabled = true
ratio   = count / 2u32
```

## Blocks

A block is a named group of fields that also lets you nest other blocks under it. The available fields and labels are set by the [schema](../references/concept_schema.md). A block kind may also be namespace-qualified (`wdoc::process { ... }`).

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

- [Schema & Decorators](../references/concept_schema.md)

- [Expressions](../references/concept_expressions.md)

- [Namespaces](../references/concept_namespaces.md)

[← All concepts](../references/concepts_ref.md)
