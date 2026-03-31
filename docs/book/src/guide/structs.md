# Structs

Structs define named value/data shapes that can be used as types throughout WCL.
While [schemas](schemas.md) validate block structure, structs define the shape of
values — they can be used as field types in schemas, other structs, function
parameters, and type annotations.

## Defining a Struct

```wcl
struct "Point" {
    x : f64
    y : f64
}

struct "Color" {
    r : u8
    g : u8
    b : u8
    a : u8 @optional
}
```

## Using Structs as Types

Struct names can be used anywhere a type is expected:

```wcl
// In schemas
schema "sprite" {
    position : Point @required
    tint     : Color @optional
    name     : string @required
}

// In other structs
struct "Rectangle" {
    origin : Point
    size   : Point
}

// In lists
struct "Polygon" {
    vertices : list(Point)
}
```

## Struct Variants

Structs support tagged variants using WCL's existing variant system:

```wcl
@tagged("type")
struct "Message" {
    type : string

    variant "text" {
        body : string
    }
    variant "image" {
        url    : string
        width  : i32
        height : i32
    }
}
```

The `@tagged("field")` decorator specifies which field discriminates between
variants.

## Structs vs Schemas

| Feature | Schema | Struct |
|---------|--------|--------|
| Purpose | Validates block structure | Defines value/data shapes |
| Used on | Blocks (`service`, `endpoint`, etc.) | Values (fields, params, type annotations) |
| Syntax | `schema "name" { ... }` | `struct "name" { ... }` |
| Fields | `name : type @decorators` | `name : type @decorators` |
| Variants | Supported | Supported |

Both use the same field syntax and support decorators and variants.
