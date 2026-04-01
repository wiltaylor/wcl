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

Fields use the same `name : type @decorators` syntax as schemas.

## Using Structs as Types

Struct names can be used anywhere a type is expected:

```wcl
// In schemas
schema "sprite" {
    position : Point @required
    tint     : Color @optional
    name     : string @required
}

// In other structs (composition)
struct "Rectangle" {
    origin : Point
    size   : Point
}

// In lists
struct "Polygon" {
    vertices : list(Point)
}
```

## Nested Composition

Structs compose naturally through nesting:

```wcl
struct "Address" {
    street : string
    city   : string
    zip    : string
}

struct "Person" {
    name    : string
    age     : i32
    address : Address
}

struct "Company" {
    name      : string
    employees : list(Person)
    hq        : Address
}
```

## Struct Variants

Structs support tagged variants using WCL's existing variant system. This
models tagged unions / sum types:

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
    variant "file" {
        path : string
        size : u64
    }
}
```

The `@tagged("field")` decorator specifies which field discriminates between
variants. When a value is validated against this struct, the variant is
selected by matching the tag field's value.

## Binary Format Structs

Structs are pure data shapes — they define *what* data looks like, not *how*
it's encoded. Encoding details (endianness, padding, alignment) are specified
in [layout](transforms.md#layouts-and-binary-parsing) definitions, not in the
struct itself.

```wcl
// Pure data shape — no encoding opinion
struct "PcapGlobalHeader" {
    magic         : u32
    version_major : u16
    version_minor : u16
    thiszone      : i32
    snaplen       : u32
    link_type     : u32
}

struct "PcapPacket" {
    ts_sec       : u32
    ts_usec      : u32
    captured_len : u32
    original_len : u32
    payload      : list(u8)
}
```

Encoding is specified in the layout:

```wcl
layout pcap {
    header : PcapGlobalHeader {
        @le                              // default little-endian
        @be("magic")                     // magic field is big-endian
        @magic("magic", 0xA1B2C3D4)     // assert constant value
    }

    @stream @count(header.record_count)
    packets : PcapPacket {
        @le
    }
}
```

This separation means the same struct can be reused with different encodings
(e.g., little-endian pcap vs big-endian pcap).

## Pattern Type in Structs

Structs can use the `pattern` type for fields that store regex values:

```wcl
struct "Route" {
    path    : pattern
    method  : string
    handler : string
}

struct "ParserConfig" {
    delimiter : pattern
    quote     : string @optional
    escape    : string @optional
}
```

See [Transforms: Pattern Type](transforms.md#pattern-type-and-regex-functions)
for details on the `pattern` type.

## Structs vs Schemas

| Feature | Schema | Struct |
|---------|--------|--------|
| Purpose | Validates block structure | Defines value/data shapes |
| Used on | Blocks (`service`, `endpoint`, etc.) | Values (fields, params, type annotations) |
| Syntax | `schema "name" { ... }` | `struct "name" { ... }` |
| Fields | `name : type @decorators` | `name : type @decorators` |
| Variants | Supported | Supported |
| Binary encoding | N/A | Via layout decorators |

Both use the same field syntax and support decorators and variants. The key
difference: schemas validate *blocks* in WCL documents, while structs define
*value types* usable anywhere a type annotation is expected.
