# Records

_Named records via the type keyword — fixed sets of named, typed fields._

A `type` declares a named record — a fixed set of named, typed fields. Records describe the shape of values, function parameters, and the backing type of every block.

## Declaring a record type

A record **value** is written as a bare `{ field: value, … }` literal. There is no `Dog { … }` constructor syntax: the literal takes its type from the position it lands in — the declared type of the field, parameter, or return it fills.

```wcl
type Dog {
  name: utf8
  age:  u32
}

@document
type Kennel {
  resident: Dog
}

resident = { name: "Rex", age: 4u32 }   // checked against Dog
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
let rex      = { name: "Rex", age: 4u32 }
let names    = keys(rex)                       // ["age", "name"]
let defaults = { host: "localhost", port: 80 }
let cfg      = merge(defaults, { port: 8080 }) // { host: "localhost", port: 8080 }
let doubled  = map_values({ low: 1, high: 9 }, fn(x: i64) -> i64 x * 2)
```

> [!NOTE]
> **Records and blocks**
>
> A record becomes a block kind when decorated with @block("kind") — see Schema & Decorators. The block's labels then bind positionally to fields marked @inline(N).

## Examples

### Declaring and constructing a record

A `type` declares a named record; a bare `{ … }` literal fills its fields, typed by the position it lands in.

```wcl
type Dog {
  name: utf8
  age:  u32
}

@document
type Kennel {
  resident: Dog
}

resident = { name: "Rex", age: 4u32 }
```

**Expected:** `resident` is a `Dog` with `name = "Rex"` and `age = 4`.

## Related

- [Unions](../references/concept_unions.md)

- [Interfaces](../references/concept_interfaces.md)

- [Type Aliases](../references/concept_type_aliases.md)

[← Back to SKILL.md](../SKILL.md)
