# Decorators

Decorators attach metadata and behavioral hints to blocks, attributes, tables, and schema fields. They are the primary extension mechanism in WCL — used for schema constraints, documentation, macro transforms, and custom tooling.

## Syntax

Decorators are written with a leading `@` followed by a name and an optional argument list:

```wcl
@name
@name(positional_arg)
@name(key = value)
@name(positional_arg, key = value)
```

Multiple decorators can be stacked on the same target:

```wcl
port = 8080 @required @validate(min = 1, max = 65535) @doc("The port this service listens on")
```

## Arguments

Decorator arguments are full WCL expressions. They can reference variables, use arithmetic, or call built-in functions:

```wcl
let max_port = 65535

port = 8080 @validate(min = 1, max = max_port)
```

Arguments may be positional, named, or a mix of both. When a decorator accepts a single primary argument, positional form is the most concise:

```wcl
env = "production" @default("development")
```

Named arguments are used when providing multiple values or for clarity:

```wcl
@deprecated(message = "use new_field instead", since = "2.0")
```

## Targets

Decorators can be placed on:

| Target         | Example                                        |
|----------------|------------------------------------------------|
| Attribute      | `port = 8080 @required`                        |
| Block          | `service "api" @deprecated("use v2") { ... }` |
| Table          | `table#hosts @open { ... }`                    |
| Schema field   | `port: int @validate(min = 1, max = 65535)`    |
| Schema itself  | `schema "service" @open { ... }`               |
| Partial block  | `partial service @partial_requires([port]) { }`|

## Stacking

Any number of decorators can appear on a single target. They are evaluated in declaration order:

```wcl
schema "endpoint" {
    url: string @required
                @validate(pattern = "^https://")
                @doc("Must be a full HTTPS URL")
}
```

## Built-in vs Custom Decorators

WCL ships with a set of built-in decorators covering common needs. You can also define your own using decorator schemas, which let you validate arguments and restrict which targets a decorator may appear on.

- See [Built-in Decorators](./decorators-builtin.md) for a full reference of the decorators provided by WCL.
- See [Decorator Schemas](./decorator-schemas.md) for how to define and validate custom decorators.
