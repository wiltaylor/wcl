# Fields & Blocks

Fields and blocks are the structural backbone of a document. Everything else — types, functions, schema — describes and constrains them.

## Fields

A field binds a name to a value with `=`. The value is any expression.

```wcl
name    = "alpha"
count   = 3u32
enabled = true
ratio   = count / 2u32
```

## Blocks

A block is a named group of fields and also lets you nest other blocks under it. To specify which fields and labels are available on a block see [Schema & Decorators](../references/schema.md).

```wcl
// One label → name = "web".
service "web" {
  port   = 8080u32
  region = "us-east-1"
}
```

Multiple labels work the same way — declare an `@inline` for each position you want to expose:

```wcl
// Two labels → verb = "GET", path = "/users".
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
