# Schema & Decorators

Decorators (written `@name` or `@name(args)`) attach schema metadata to declarations and fields. Together they describe a document's legal structure, which `wcl check` validates.

## Declaration decorators

| Decorator | On | Meaning |
| --- | --- | --- |
| `@document` | type | Marks the document root schema (composes per namespace — see below) |
| `@block("kind")` | type | Makes the type a nestable block of that kind |
| `@table("kind")` | type | Row schema for pipe-table syntax |
| `@schemaless` | type | Opts a type out of root validation, allowing multiple decorators / reflection |

## Field decorators

| Decorator | Meaning |
| --- | --- |
| `@child("kind")` | A single nested block of the given kind |
| `@children("kind")` | A list of nested blocks (or union variants — dispatch) |
| `@inline(slot)` | Bind the block label to a field at that position |
| `@default(expr)` | Default value when the field is omitted |
| `@connections(S)` | Accumulate `->` connection statements as records |

## A worked example

A document root with one kind of child block, whose label becomes an inline field and whose port has a default:

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
```

Given that schema, this document validates: each `service` block contributes a `Service` to `services`, with `name` taken from the label and `port` defaulting to `80`.

```wcl
service "web" { region = "us-east-1" }
service "api" { port = 9090u32  region = "eu-west-1" }
```

## Composing document schemas

Several `@document` schemas can govern the same namespace, and they **merge**: a top-level field or block is legal if **any** of them declares it. This lets you import a library that ships its own `@document` (for instance wdoc's `Site`) and still add your own top-level tags — just declare your own `@document` at the root.

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

Imported (library) `@document` schemas merge silently, so importing several modules that each ship one is fine. Only a **second root-authored** `@document` in the same namespace is an error — you get one root schema, which composes with whatever the imports provide.

When you import `<wdoc.wcl>`, wdoc can auto-generate a reference page for the blocks your `@document` declares: `block_reference { type = ProjectDoc }` emits a heading and property table per block. See [Documenting your own schema](../references/wdoc_data_views.md).

> [!NOTE]
> **Reflection**
> decorator_names(T) and decorator_arg(T, name, slot) read decorators back at evaluation time — used by libraries (like wdoc) that dispatch on a block's declared kind.
