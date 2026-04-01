# Transforms

WCL Transform is a declarative data transformation engine that enables format
conversion and field-level mapping using WCL's expression language.

## Basic Transform

A transform maps data from an input format to an output format:

```wcl
transform rename-fields {
    input  = "codec::json"
    output = "codec::json"

    map {
        user_name = in.name
        user_age  = in.age
    }
}
```

The `map` block defines field mappings. `in` refers to the current input record.
Each attribute in the map block defines an output field and the expression to
compute its value.

## Running a Transform

Use the CLI to execute a transform:

```bash
wcl transform run rename-fields -f transforms.wcl --input data.json --output result.json
```

- `--input`: Input data file (stdin if omitted)
- `--output`: Output file (stdout if omitted)
- `-f`: WCL file containing the transform definition

## Field Expressions

Map expressions can use any WCL expression:

```wcl
transform enrich {
    input  = "codec::json"
    output = "codec::json"

    map {
        // Direct field mapping
        id = in.id

        // String interpolation
        display_name = "${in.first_name} ${in.last_name}"

        // Function calls
        email = lower(in.email)

        // Ternary expressions
        role = in.is_admin ? "admin" : "member"

        // Math
        age_months = in.age * 12
    }
}
```

## Filtering with `@where`

The `@where` decorator on a `map` block filters records:

```wcl
transform active-users {
    input  = "codec::json"
    output = "codec::json"

    @where(in.active == true)
    map {
        name = in.name
    }
}
```

Records that don't match the where condition are dropped from the output.

## Codecs

Codecs handle format-specific parsing and emission:

| Codec | Description |
|-------|-------------|
| `json` | JSON (arrays stream per element) |

More codecs (YAML, CSV, TOML, HCL, XML, MessagePack) are planned.

## Pattern Type and Regex Functions

WCL has a `pattern` type for regex values:

```wcl
schema "route" {
    path : pattern @required
}
```

Built-in regex functions:

- `regex_match(string, pattern) -> bool`
- `regex_capture(string, pattern) -> list(string)`
- `regex_replace(string, pattern, replacement) -> string`
- `regex_replace_all(string, pattern, replacement) -> string`
- `regex_split(string, pattern) -> list(string)`
- `regex_find(string, pattern) -> string?`
- `regex_find_all(string, pattern) -> list(string)`

## Exported Functions

Functions defined with `export let` can be called from host programs:

```wcl
export let double = x => x * 2
export let greet = name => "Hello, ${name}!"
```

### Rust API

```rust
let doc = wcl::parse(source, options);
let result = doc.call_function("double", &[Value::Int(21)])?;
assert_eq!(result, Value::Int(42));
```

### C FFI

```c
char* result = wcl_ffi_call_function(doc, "double", "[21]");
// result is "42"
```
