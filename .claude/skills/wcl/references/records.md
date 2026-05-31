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

> [!NOTE]
> **Records and blocks**
> A record becomes a block kind when decorated with @block("kind") — see Schema & Decorators. The block's labels then bind positionally to fields marked @inline(N).
