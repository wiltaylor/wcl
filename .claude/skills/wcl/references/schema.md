# Schema & Decorators

Decorators (written `@name` or `@name(args)`) attach schema metadata to declarations and fields. Together they describe a document's legal structure, which `wcl check` validates.

## Declaration decorators

| Decorator | On | Meaning |
| --- | --- | --- |
| `@document` | type | Marks the document root schema (one per namespace) |
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

> [!NOTE]
> **Reflection**
> decorator_names(T) and decorator_arg(T, name, slot) read decorators back at evaluation time — used by libraries (like wdoc) that dispatch on a block's declared kind.
