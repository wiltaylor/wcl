# Records

_Named records via the type keyword — fixed sets of named, typed fields._

A `type` declares a named record — a fixed set of named, typed fields. Records describe the shape of values, function parameters, and the backing type of every block.

## Declaring a record type

```wcl
type Dog {
  name: utf8
  age:  u32
}

let rex = Dog { name: "Rex", age: 4u32 }
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

Four builtins operate on record values: `keys` and `values` list a record's field names and values (sorted by name); `merge` combines two records, with the second winning on a clash; `map_values` transforms every value while keeping the keys.

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

## Examples

### Declaring and constructing a record

A `type` declares a named record; a record value fills its fields.

```wcl
type Dog {
  name: utf8
  age:  u32
}

let rex = Dog { name: "Rex", age: 4u32 }
```

**Expected:** `rex` is a `Dog` record with `name = "Rex"` and `age = 4`.

## Related

- [Unions](../references/concept_unions.md)

- [Interfaces](../references/concept_interfaces.md)

- [Type Aliases](../references/concept_type_aliases.md)

[← Back to SKILL.md](../SKILL.md)
