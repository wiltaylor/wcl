# Schema & Decorators

_Decorators that describe a document's legal structure, validated by wcl check._

Decorators (written `@name` or `@name(args)`) attach schema metadata to declarations and fields. Together they describe a document's legal structure, which `wcl check` validates.

## Declaration decorators

| Decorator | On | Meaning |
| --- | --- | --- |
| `@document` | type | Marks the document root schema (composes per namespace) |
| `@block("kind")` | type | Makes the type a nestable block of that kind |
| `@table("kind")` | type | Row schema for pipe-table syntax |
| `@decorator("name")` | type | Schema for a user-defined `@name` decorator |
| `@schemaless` | type | Opens the type: instances accept undeclared fields / children |
| `@by_ref` | type | A block in a slot reifies to a resolvable reference instead of inlining |
| `@dynamic` | connection | Lets `->` operands name ids resolved at consume time |

## Field decorators

| Decorator | Meaning |
| --- | --- |
| `@child("kind")` | A single nested block of the given kind |
| `@children("kind")` | A list of nested blocks (or union variants) |
| `@inline(slot)` | Bind the block label to a field at that position |
| `@default(expr)` | Default value when the field is omitted |
| `@connections(S)` | Accumulate `->` connection statements as records |
| `@min(n)` / `@max(n)` | Numeric range constraint, checked by `wcl check` |
| `@non_empty` | The string / list value must not be empty |
| `@ref("kind")` | The id value(s) must name an existing block of that kind |

The constraint decorators also attach to a [type alias](../references/concept_records.md) (`@min(1) type Port = u16`), applying to every field declared with the alias.

## Referential integrity with @ref

A field holding an `identifier` (or `list<identifier>`) that is semantically a reference to another block can declare that with `@ref("kind")`. `wcl check` then verifies every id names an existing block of that kind — anywhere in the document — and reports a dangling reference otherwise.

```wcl
@block("screen") type Screen { @inline(0) id: identifier  name: utf8 }
@block("flow") type Flow {
  @inline(0) id: identifier
  @ref("screen") entry_screen: identifier   // must be a declared screen id
  @ref("screen") steps: list<identifier>    // every element checked
}
```

## Child-count constraints on @block

`@block` accepts two named arguments that constrain nested children: `max_children = N` caps the total nested-block count, and `required_children = ["kind", ...]` demands at least one child of each listed kind. Both are enforced by `wcl check`.

```wcl
@block("stage", max_children = 4, required_children = ["step"])
type Stage {
  @inline(0) name: utf8
  @children("step") steps: list<Step>
}
```

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

## Composing document schemas

Several `@document` schemas can govern the same namespace, and they **merge**: a top-level field or block is legal if **any** of them declares it. This lets you import a library that ships its own `@document` and still add your own top-level tags.

```wcl
import <wdoc.wcl>            // brings in wdoc's library @document

@document
type ProjectDoc {           // your own root schema — merges with wdoc's
  @children("project_meta") metas: list<ProjectMeta>
}

@block("project_meta")
type ProjectMeta { @inline(0) id: identifier  owner: utf8 }

project_meta info { owner = "Wil" }   // your tag, alongside wdoc pages
page index { text { span "Hello" {} } }
```

Imported (library) `@document` schemas merge silently. Only a **second root-authored** `@document` in the same namespace is an error — you get one root schema, which composes with whatever the imports provide.

> [!NOTE]
> **Reflection**
> decorator_names(T) and decorator_arg(T, name, slot) read decorators back at evaluation time — used by libraries (like wdoc) that dispatch on a block's declared kind.

## Examples

### A document root with one kind of child block

The block label becomes an inline field; the port defaults to 80.

```wcl
@block("service")
type Service {
  @inline(0)   name: utf8     // service "web" → name = "web"
  @default(80) port: u32
  region: utf8
}

@document
type Config {
  @children("service") services: list<Service>
}

service "web" { region = "us-east-1" }
service "api" { port = 9090u32  region = "eu-west-1" }
```

**Expected:** Each `service` block contributes a `Service`; `web`'s port defaults to 80.

## Related

- [Records](../references/concept_records.md)

- [Fields & Blocks](../references/concept_fields_blocks.md)

- [Connections](../references/concept_connections.md)

- [References](../references/concept_references.md)

[← All concepts](../references/concepts_ref.md)
