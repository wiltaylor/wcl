# Records

A `type` declares a named record — a fixed set of named, typed fields. Records describe the shape of values, function parameters, and the backing type of every block.

## Declaring a record type

```wcl
type Dog {
  name: utf8
  age:  u32
}

let rex = Dog { name: "Rex", age: 4u32 }
```

## Type aliases

`type Name = TypeRef` declares a transparent alias — a readable name for any type, resolved wherever the name is used (transitively, so an alias of an alias works). Constraint decorators on the alias travel with it: every field declared with the alias is validated against them by `wcl check`.

```wcl
@min(1) @max(65535)
type Port = u16

@non_empty
type Name = utf8

type Service {
  name: Name        # rejects ""
  port: Port        # rejects 0u16 and 70000-ish values
}
```

## extends

A record may `extends` another, inheriting all its fields. The child type is a structural superset; it satisfies any context that accepts the parent.

```wcl
type Dog {
  name: utf8
  age:  u32
}

type Pet extends Dog {
  breed: utf8
}
```

## Working with records

Four builtins operate on record values: `keys` and `values` list a record's field names and values (in deterministic, sorted-by-name order); `merge` combines two records, with the second record's fields winning on a clash; `map_values` transforms every field value while keeping the keys. See [Builtin Functions](../references/builtins.md) for signatures.

```wcl
let rex      = Dog { name: "Rex", age: 4u32 }
let names    = keys(rex)                       // ["age", "name"]
let defaults = { host: "localhost", port: 80 }
let cfg      = merge(defaults, { port: 8080 }) // { host: "localhost", port: 8080 }
let doubled  = map_values({ low: 1, high: 9 }, fn(x: i64) -> i64 x * 2)
```

> [!NOTE]
> **Records and blocks**
> A record becomes a block kind when decorated with @block("kind") — see Schema & Decorators. The block's labels then bind positionally to fields marked @inline(N).
