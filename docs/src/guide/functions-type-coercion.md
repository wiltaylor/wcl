# Type Coercion Functions

WCL is strongly typed and does not coerce values implicitly. When you need to convert between types, use these explicit coercion functions. The `type_of` function lets you inspect a value's type at evaluation time.

## Reference

| Function | Signature | Description |
|---|---|---|
| `to_string` | `to_string(value: any) -> string` | Convert any value to its string representation |
| `to_int` | `to_int(value: string\|float\|bool) -> int` | Parse or truncate a value to integer |
| `to_float` | `to_float(value: string\|int\|bool) -> float` | Parse or promote a value to float |
| `to_bool` | `to_bool(value: string\|int) -> bool` | Parse a value to boolean |
| `type_of` | `type_of(value: any) -> string` | Return a string naming the value's type |

## Examples

### to_string

Converts any WCL value to a string. This is equivalent to placing a value inside `${}` interpolation.

```wcl
let s1 = to_string(42)       // "42"
let s2 = to_string(3.14)     // "3.14"
let s3 = to_string(true)     // "true"
let s4 = to_string([1, 2])   // "[1, 2]"
```

Useful when building strings from computed values:

```wcl
let label = "replicas-" + to_string(replica_count)
```

### to_int

Converts strings, floats, and booleans to integers.

```wcl
let n1 = to_int("42")       // 42
let n2 = to_int(3.9)        // 3  (truncates toward zero)
let n3 = to_int(true)       // 1
let n4 = to_int(false)      // 0
```

Parsing a string that is not a valid integer is a runtime error:

```wcl
let bad = to_int("hello")   // error
```

### to_float

Converts strings, integers, and booleans to floats.

```wcl
let f1 = to_float("3.14")   // 3.14
let f2 = to_float(10)       // 10.0
let f3 = to_float(true)     // 1.0
let f4 = to_float(false)    // 0.0
```

### to_bool

Converts strings and integers to booleans.

For strings: `"true"` → `true`, `"false"` → `false`. Any other string is a runtime error.

For integers: `0` → `false`, any non-zero → `true`.

```wcl
let b1 = to_bool("true")    // true
let b2 = to_bool("false")   // false
let b3 = to_bool(1)         // true
let b4 = to_bool(0)         // false
```

### type_of

Returns a lowercase string naming the type of the given value.

| Value type | `type_of` result |
|---|---|
| `string` | `"string"` |
| `int` | `"int"` |
| `float` | `"float"` |
| `bool` | `"bool"` |
| `list` | `"list"` |
| `map` | `"map"` |
| block reference | `"block"` |
| `null` | `"null"` |

```wcl
let t1 = type_of("hello")     // "string"
let t2 = type_of(42)          // "int"
let t3 = type_of(3.14)        // "float"
let t4 = type_of(true)        // "bool"
let t5 = type_of([1, 2, 3])   // "list"
let t6 = type_of({a: 1})      // "map"
```

Branching on type:

```wcl
let describe = v => if type_of(v) == "int" {
  "integer: " + to_string(v)
} else if type_of(v) == "string" {
  "string: " + v
} else {
  "other: " + to_string(v)
}
```

## No Implicit Coercion

WCL never coerces types automatically. The following are all errors:

```wcl
let bad1 = "count: " + 5       // error: cannot add string and int
let bad2 = 1 + true            // error: cannot add int and bool
let bad3 = if 1 { "yes" }      // error: condition must be bool
```

Always use an explicit coercion function when mixing types.
