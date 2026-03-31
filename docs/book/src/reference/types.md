# Type System

WCL has a rich static type system used in schema definitions, macro parameter declarations, table column declarations, and type expressions.

## Primitive Types

| Type | Keyword(s) | Description | Example literals |
|------|-----------|-------------|-----------------|
| String | `string` | UTF-8 text, supports interpolation and heredocs | `"hello"`, `"port: ${p}"` |
| Integer | `i8`, `u8`, `i16`, `u16`, `i32`, `u32`, `i64`, `u64`, `i128`, `u128` | Signed and unsigned integers of various widths | `42`, `0xFF`, `0o77`, `0b1010` |
| Float | `f32`, `f64` | IEEE 754 floating-point numbers | `3.14`, `1.0e-3` |
| Boolean | `bool` | `true` or `false` | `true`, `false` |
| Null | `null` | Absence of a value | `null` |
| Identifier | `identifier` | An identifier literal (may contain hyphens) | `#svc-api`, `#my-resource` |
| Date | `date` | ISO 8601 date | `d"2024-03-15"` |
| Duration | `duration` | ISO 8601 duration | `dur"P1Y2M3D"` |

## Integer Types

WCL provides a full set of fixed-width integer types covering both signed and unsigned variants:

| Type | Signed | Width | Minimum | Maximum |
|------|--------|-------|---------|---------|
| `i8` | Yes | 8-bit | −128 | 127 |
| `u8` | No | 8-bit | 0 | 255 |
| `i16` | Yes | 16-bit | −32,768 | 32,767 |
| `u16` | No | 16-bit | 0 | 65,535 |
| `i32` | Yes | 32-bit | −2,147,483,648 | 2,147,483,647 |
| `u32` | No | 32-bit | 0 | 4,294,967,295 |
| `i64` | Yes | 64-bit | −9.2 × 10¹⁸ | 9.2 × 10¹⁸ |
| `u64` | No | 64-bit | 0 | 1.8 × 10¹⁹ |
| `i128` | Yes | 128-bit | −1.7 × 10³⁸ | 1.7 × 10³⁸ |
| `u128` | No | 128-bit | 0 | 3.4 × 10³⁸ |

`i64` is the default integer type. Untyped integer literals are inferred as `i64`. Values that exceed 64-bit range use BigInt (`i128`/`u128`) representation at runtime.

Integer literals support multiple bases:

- Decimal: `42`
- Hexadecimal: `0xFF`
- Octal: `0o77`
- Binary: `0b1010`

## Float Types

| Type | Width | Precision | Range |
|------|-------|-----------|-------|
| `f32` | 32-bit | ~7 decimal digits | ±3.4 × 10³⁸ |
| `f64` | 64-bit | ~15 decimal digits | ±1.8 × 10³⁰⁸ |

`f64` is the default float type. Untyped float literals are inferred as `f64`.

## Date Type

The `date` type represents a calendar date in ISO 8601 format. Date literals use the `d"..."` prefix syntax:

```wcl
let release = d"2024-03-15"
let epoch   = d"1970-01-01"
```

Dates support comparison operators (`<`, `>`, `<=`, `>=`, `==`, `!=`).

## Duration Type

The `duration` type represents a time span using ISO 8601 duration format. Duration literals use the `dur"..."` prefix syntax:

```wcl
let timeout  = dur"PT30S"       // 30 seconds
let interval = dur"P1Y2M3D"    // 1 year, 2 months, 3 days
let ttl      = dur"PT1H30M"    // 1 hour, 30 minutes
```

Durations support comparison operators and can be added to dates.

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
  port    : i64
  host    : string
  tags    : list(string)
  meta    : map(string, any)
  env     : union(string, null)
  service : ref("ServiceSchema")
}
```

```wcl
macro connect(host: string, port: i64 = 5432) {
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

- Narrower integer types are accepted where a wider integer type is expected (e.g., `i8` widens to `i64`).
- All integer types are accepted where a float type is expected (integer-to-float widening is safe).
- Unsigned integers are accepted where a signed integer of greater width is expected (e.g., `u8` widens to `i16`).
- `null` is accepted for any `union(T, null)` type.
- `any` accepts all values without checking.

## Serde Type Mapping

When serializing to JSON, YAML, or TOML, WCL types map as follows:

| WCL Type | JSON | YAML | TOML |
|----------|------|------|------|
| `string` | string | string | string |
| `i8`..`u128` | number (integer) | int | integer |
| `f32`, `f64` | number (float) | float | float |
| `bool` | boolean | bool | boolean |
| `null` | null | null | not representable (omitted) |
| `identifier` | string (bare name) | string | string |
| `date` | string (ISO 8601) | date | datetime |
| `duration` | string (ISO 8601) | string | string |
| `list(T)` | array | sequence | array |
| `map(K, V)` | object | mapping | table |
| `set(T)` | array (deduplicated) | sequence | array |
| `ref(...)` | resolved value | resolved value | resolved value |
| `any` | native JSON value | native | native |

Block structure is serialized as nested objects keyed by block type, then by block ID (if present).
