# Data Types

WCL has a rich type system covering primitive scalars, composite collections, and a set of special types used internally by the schema and query systems.

## Primitive Types

### `string`

String values are written with double quotes. All standard escape sequences are supported:

| Escape | Meaning              |
|--------|----------------------|
| `\n`   | Newline              |
| `\t`   | Tab                  |
| `\r`   | Carriage return      |
| `\\`   | Literal backslash    |
| `\"`   | Literal double quote |
| `\0`   | Null byte            |
| `\uXXXX` | Unicode code point |

```wcl
greeting = "Hello, world!"
escaped  = "Line one\nLine two"
quoted   = "He said \"hello\""
unicode  = "caf\u00E9"
```

#### Heredocs

For multi-line strings, WCL supports heredoc syntax. The delimiter must appear alone on its line.

Standard heredoc (preserves leading whitespace):

```wcl
message = <<EOF
  Dear user,
  Welcome to WCL.
EOF
```

Indented heredoc (`<<-`): strips the leading whitespace common to all lines, based on the closing delimiter's indentation:

```wcl
message = <<-EOF
  Dear user,
  Welcome to WCL.
  EOF
```

Raw heredoc (`<<'EOF'`): disables `${...}` interpolation. The content is taken exactly as-is:

```wcl
template = <<'EOF'
  Use ${variable} syntax in your templates.
EOF
```

### `i64`

Integer literals support several bases and underscore separators for readability:

```wcl
decimal   = 1000000
hex       = 0xFF
octal     = 0o755
binary    = 0b1010_1010
separated = 1_000_000
negative  = -42
```

### `f64`

Floating-point literals support decimal and scientific notation:

```wcl
pi         = 3.14159
small      = 1.5e-3
large      = 2.998e8
negative   = -0.5
```

### `bool`

```wcl
enabled  = true
disabled = false
```

### `null`

```wcl
optional_field = null
```

### `identifier`

Bare identifiers (without quotes) are used in certain contexts such as schema type names and query selectors. They are distinct from strings at the type level.

## Composite Types

### `list(T)`

An ordered collection of values of type `T`. List literals use `[...]` with comma-separated elements. Trailing commas are allowed.

```wcl
ports    = [8080, 8081, 8082]
names    = ["alice", "bob", "carol",]   // trailing comma OK
mixed    = [1, "two", true]             // list(any)
nested   = [[1, 2], [3, 4]]             // list(list(i64))
```

### `map(K, V)`

An ordered map from keys of type `K` to values of type `V`. Map literals use `{ key: value, key2: value2 }` syntax:

```wcl
env_vars = {
  HOST:  "0.0.0.0",
  PORT:  "8080",
  DEBUG: "false",
}

scores = {
  alice: 95,
  bob:   87,
}
```

Map keys are identifiers in literal syntax. When a key needs to be a computed value or contain special characters, use the `map()` built-in function.

### `set(T)`

An unordered collection of unique values of type `T`. Sets are produced by the `set()` function and certain query operations; there is no dedicated set literal syntax.

```wcl
unique_ports = set([8080, 8080, 9090])  // {8080, 9090}
```

## Special Types

### `ref(schema)`

A typed reference to another block by its inline ID. Used to create cross-block relationships validated by the schema system:

```wcl
database db-primary { host = "db.internal" }

connection {
  target = ref(db-primary)
}
```

### `any`

Accepts any value regardless of type. Used in schemas to opt out of type checking for a specific field, and returned by queries that mix types.

### `union(T1, T2, ...)`

Accepts a value that matches any of the listed types. Declared in schemas:

```wcl
schema "flex_port" {
  port: union(i64, string)
}
```

### `function` (internal)

The type of lambda expressions and built-in functions. It cannot be stored in an attribute or returned from the evaluator — it exists only during evaluation for use in higher-order functions like `map()`, `filter()`, and `sort_by()`.

## Type Coercion

WCL is strictly typed. Implicit coercions are intentionally minimal:

| From  | To    | When                                      |
|-------|-------|-------------------------------------------|
| `i64` | `f64` | In arithmetic expressions involving floats |

All other conversions require explicit function calls:

| Function      | Converts to | Example                       |
|---------------|-------------|-------------------------------|
| `to_string(v)` | `string`   | `to_string(42)` → `"42"`      |
| `to_int(v)`   | `i64`       | `to_int("42")` → `42`         |
| `to_float(v)` | `f64`       | `to_float("3.14")` → `3.14`   |
| `to_bool(v)`  | `bool`      | `to_bool(0)` → `false`        |

Attempting an invalid coercion (for example `to_int("hello")`) produces a runtime error.

## Type Names in Schemas

When writing schema definitions, type names use the following syntax:

```wcl
schema "server_config" {
  host:    string
  port:    i64
  enabled: bool
  tags:    list(string)
  meta:    map(string, any)
  addr:    union(string, null)
}
```

See [Schemas](./schemas.md) for the complete schema language.
