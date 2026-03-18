# Type System

WCL has a rich static type system used in schema definitions, macro parameter declarations, table column declarations, and type expressions.

## Primitive Types

| Type | Keyword | Description | Example literals |
|------|---------|-------------|-----------------|
| String | `string` | UTF-8 text, supports interpolation and heredocs | `"hello"`, `"port: ${p}"` |
| Integer | `int` | 64-bit signed integer (decimal, hex, octal, binary) | `42`, `0xFF`, `0o77`, `0b1010` |
| Float | `float` | 64-bit IEEE 754 double | `3.14`, `1.0e-3` |
| Boolean | `bool` | `true` or `false` | `true`, `false` |
| Null | `null` | Absence of a value | `null` |
| Identifier | `identifier` | An identifier literal (may contain hyphens) | `#svc-api`, `#my-resource` |

## Composite Types

| Type | Syntax | Description | Example |
|------|--------|-------------|---------|
| List | `list(T)` | Ordered sequence of values of type `T` | `[1, 2, 3]` |
| Map | `map(K, V)` | Key-value pairs; keys are type `K`, values type `V` | `{ a: 1, b: 2 }` |
| Set | `set(T)` | Unordered collection of unique values of type `T` | `[1, 2, 3]` |

## Special Types

| Type | Syntax | Description |
|------|--------|-------------|
| Any | `any` | Accepts a value of any type; opts out of type checking |
| Union | `union(T1, T2, ...)` | A value that may be any one of the listed types |
| Ref | `ref("SchemaName")` | A reference to a block conforming to the named schema |
| Function | — | First-class lambda values; not directly nameable in type expressions |

## Type Expressions

Type expressions appear in schema fields, macro parameters, and table column declarations:

```wcl
schema "Config" {
  port    : int
  host    : string
  tags    : list(string)
  meta    : map(string, any)
  env     : union(string, null)
  service : ref("ServiceSchema")
}
```

```wcl
macro connect(host: string, port: int = 5432) {
  host = host
  port = port
}
```

```wcl
table routes {
  path:    string
  method:  string
  active:  bool
  | "/api" | "GET" | true |
}
```

## Type Coercion

WCL does not implicitly coerce between types. Type mismatches produce an error at evaluation time (E050) or schema validation time (E071). Explicit conversions are performed via built-in functions.

Exceptions:

- Integer literals are accepted where `float` is expected (widening is safe).
- `null` is accepted for any `union(T, null)` type.
- `any` accepts all values without checking.

## Serde Type Mapping

When serializing to JSON, YAML, or TOML, WCL types map as follows:

| WCL Type | JSON | YAML | TOML |
|----------|------|------|------|
| `string` | string | string | string |
| `int` | number (integer) | int | integer |
| `float` | number (float) | float | float |
| `bool` | boolean | bool | boolean |
| `null` | null | null | not representable (omitted) |
| `identifier` | string (bare name) | string | string |
| `list(T)` | array | sequence | array |
| `map(K, V)` | object | mapping | table |
| `set(T)` | array (deduplicated) | sequence | array |
| `ref(...)` | resolved value | resolved value | resolved value |
| `any` | native JSON value | native | native |

Block structure is serialized as nested objects keyed by block type, then by block ID (if present).
