# Transforms

WCL Transform is a declarative data transformation engine that enables format
conversion and field-level mapping using WCL's expression language. It supports
streaming, stateful processing, accumulators, pipelines, and binary format
parsing.

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

The `transform` block has an inline ID (`rename-fields`) used to reference it
from the CLI. Inside:

- `input` and `output` specify codecs (format adapters)
- `map` blocks define field mappings
- `in` refers to the current input record

Each attribute in the map block defines an output field name and the expression
to compute its value.

## Running a Transform

Use the CLI to execute a transform:

```bash
wcl transform run rename-fields -f transforms.wcl --input data.json --output result.json
```

- `--input`: Input data file (stdin if omitted)
- `--output`: Output file (stdout if omitted)
- `-f`: WCL file containing the transform definition

Transform statistics (records read, written, filtered) are printed to stderr.

See [CLI: transform](../cli/transform.md) for the full command reference.

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

        // Regex
        domain = regex_find(in.email, "@(.+)")
    }
}
```

Fields from the input record are accessible both via `in.field_name` and
directly as `field_name` (the top-level fields are bound into scope).

## Filtering with `@where`

The `@where` decorator on a `map` block filters records. Records that don't
match are dropped from the output:

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

Multiple `@where` decorators are ANDed:

```wcl
@where(in.status == "active")
@where(in.age >= 18)
map {
    name = in.name
}
```

## Codecs

Codecs handle format-specific parsing and emission. They are specified as
string values on the `input` and `output` attributes:

```wcl
transform convert {
    input  = "codec::json"
    output = "codec::json"
}
```

### Available Codecs

| Codec | Description |
|-------|-------------|
| `json` | JSON — arrays stream per element, objects are single records |

More codecs (YAML, CSV, TOML, HCL, XML, MessagePack) are planned.

### Codec Options

Codecs accept format-specific options:

```wcl
transform pretty-output {
    input  = "codec::json"
    output = "codec::json"

    // Codec options are passed via the output_options parameter
    // in the Rust API. CLI defaults to compact JSON output.
}
```

## Structs as Data Types

[Structs](structs.md) define pure data shapes that complement transforms.
While transforms map between formats, structs define the structure of the
data being transformed:

```wcl
struct "UserRecord" {
    id         : u64
    first_name : string
    last_name  : string
    email      : string
    age        : i32
}

struct "ApiUser" {
    id           : u64
    display_name : string
    email        : string
    role         : string
}
```

Structs can be used as field types in schemas and other structs, and will be
used by the layout engine for binary format parsing.

## Stateful Transforms

Transforms can maintain state across records using the `@stateful` decorator
on exported lambda functions. State is bounded by default (10,000 keys) with
configurable eviction.

### Declaring Stateful Functions

```wcl
@stateful
export let running_avg = (value, window) => {
    let values = state.get("window") ?? []
    let updated = concat(values, [value])
    let trimmed = updated  // keep last N values
    state.set("window", trimmed)
    sum(trimmed) / len(trimmed)
}
```

The `@stateful` decorator injects a `state` handle into the function's scope.

### State API

| Method | Signature | Description |
|--------|-----------|-------------|
| `state.get` | `get(key: string) -> any?` | Returns `null` if key not found |
| `state.set` | `set(key: string, value: any)` | Upsert a value |
| `state.delete` | `delete(key: string) -> bool` | Returns true if key existed |
| `state.has` | `has(key: string) -> bool` | Check key existence |
| `state.keys` | `keys() -> list(string)` | All keys (use sparingly) |
| `state.clear` | `clear()` | Reset all state |

### State Configuration

State blocks in transforms configure the backend:

```wcl
transform with-state {
    input  = "codec::json"
    output = "codec::json"

    state {
        backend  = "memory"
        max_keys = 10000
        eviction = "lru"      // lru, fifo, or reject_new
    }

    map {
        // stateful functions can be called here
    }
}
```

### Scoped State

Use `@stateful(scope = "name")` to link a function to a named state block:

```wcl
@stateful(scope = "per_sensor")
export let sensor_avg = (temp, alpha) => {
    let prev = state.get("avg") ?? temp
    let result = alpha * temp + (1.0 - alpha) * prev
    state.set("avg", result)
    result
}
```

When combined with `@group_by`, each group key gets independent state.

## Accumulators

Accumulators aggregate data from stream records into structured output. They
provide the mechanism for stream-to-structured data flow.

### Accumulator Operators

| Operator | Syntax | Description |
|----------|--------|-------------|
| Sum | `field += expr` | Running sum (numeric) |
| Min | `min(current, expr)` | Running minimum |
| Max | `max(current, expr)` | Running maximum |
| Count | `count()` | Increment per record |
| First | `first(expr)` | Keep only the first value seen |
| Last | `last(expr)` | Keep the most recent value |
| Collect | `collect(expr)` | Append to a list |
| Collect Unique | `collect_unique(expr)` | Append unique values only |

### Example

```wcl
transform stats {
    input  = "codec::json"
    output = "codec::json"

    accumulate records {
        total_count   += 1
        total_amount  += in.amount
        first_date     = first(in.date)
        last_date      = last(in.date)
        categories     = collect_unique(in.category)
    }
}
```

### Bounded Collection

To prevent unbounded memory growth, `collect` and `collect_unique` support
bounds:

```wcl
accumulate records {
    recent_ids = collect(in.id) {
        max_size = 1000
        overflow = "drop_oldest"  // or "drop_newest", "error"
    }
}
```

## Pipelines

Pipelines chain multiple transforms. Each step's output feeds the next step's
input.

```wcl
pipeline csv-to-api {
    steps = [
        transforms::parse_csv,
        transforms::normalize,
        transforms::emit_json,
    ]
}
```

### Streaming Fusion

Adjacent pipeline steps are fused — each record flows through all steps before
the next record enters, avoiding intermediate materialization:

```
Record 1 → Step 1 → Step 2 → Step 3 → emit
Record 2 → Step 1 → Step 2 → Step 3 → emit
```

### Pipeline Parameters

Pipelines can accept parameters:

```wcl
pipeline configurable {
    params {
        threshold : f64 = 95.0
    }

    steps = [
        transforms::ingest,
        transforms::filter { min_value = params.threshold },
        transforms::emit,
    ]
}
```

## Layouts and Binary Parsing

Layouts define how [structs](structs.md) compose into complete format
descriptions. They distinguish between **structured** (fully buffered) and
**stream** (one-record-at-a-time) sections.

### Layout Definition

```wcl
layout pcap {
    // Structured: fully buffered, fields available to stream sections
    header : PcapGlobalHeader {
        @le
        @be("magic")
        @magic("magic", 0xA1B2C3D4)
    }

    // Streamed: records flow through one at a time
    @stream @count(header.record_count)
    packets : PcapPacket {
        @le
    }
}
```

### Encoding Decorators

Encoding details live on layout sections, not on struct definitions. This
keeps structs as pure data shapes:

| Decorator | Purpose | Example |
|-----------|---------|---------|
| `@le` | Default little-endian for all fields | `@le` |
| `@be` | Default big-endian, or per-field override | `@be("magic")` |
| `@magic` | Assert a field equals an expected value | `@magic("magic", 0xA1B2C3D4)` |
| `@padding` | Skip N bytes after a field | `@padding(4)` |
| `@align` | Align to N-byte boundary | `@align(16)` |
| `@encoding` | String encoding for a field | `@encoding("payload", "raw")` |

### Structured vs Stream Sections

- **Structured** sections are fully parsed into memory and available for
  random access by later sections (e.g., a file header with record counts).
- **Stream** sections iterate one record at a time with bounded memory.

### Stream Termination

Streams can terminate in several ways:

```wcl
// Fixed count from a structured field
@stream @count(header.record_count)
records : Record

// End of input (default)
@stream
lines : TextLine

// Size-bounded
@stream @max_bytes(header.data_size)
chunks : DataChunk
```

## Pattern Type and Regex Functions

WCL has a `pattern` type that stores a compiled regex. It is a first-class
value, not just a validated string:

```wcl
schema "route" {
    path : pattern @required
}
```

See [String Functions](functions-string.md) for the full regex function
reference. All regex functions accept either a plain string or a `pattern`
value as the pattern argument:

- `regex_match(string, pattern) -> bool`
- `regex_capture(string, pattern) -> list(string)`
- `regex_replace(string, pattern, replacement) -> string`
- `regex_replace_all(string, pattern, replacement) -> string`
- `regex_split(string, pattern) -> list(string)`
- `regex_find(string, pattern) -> string?`
- `regex_find_all(string, pattern) -> list(string)`

## Exported Functions (Host-Callable Lambdas)

Functions defined with `export let` can be called from host programs (Rust,
C, WASM, Python, Go, .NET):

```wcl
export let double = x => x * 2
export let greet = name => "Hello, ${name}!"
export let add = (a, b) => a + b
```

### Decorators on Exported Functions

Use standard `@` decorators on the `export let` binding:

```wcl
@stateful
export let counter = () => {
    let n = (state.get("n") ?? 0) + 1
    state.set("n", n)
    n
}

@accumulator
export let track_total = (amount) => {
    acc.total += amount
}
```

### Rust API

```rust
let doc = wcl::parse(source, options);

// List exported functions
let names = doc.exported_function_names();
// ["double", "greet", "add"]

// Call a function
let result = doc.call_function("double", &[Value::Int(21)])?;
assert_eq!(result, Value::Int(42));
```

### C FFI

```c
// Call a function (args as JSON array)
char* result = wcl_ffi_call_function(doc, "double", "[21]");
// result is "42"
wcl_ffi_string_free(result);

// List functions (returns JSON array)
char* funcs = wcl_ffi_list_functions(doc);
// [{"name":"double","params":["x"]}, ...]
wcl_ffi_string_free(funcs);
```

### WASM / JavaScript

```javascript
const doc = wcl.parse(source);
const result = doc.callFunction("double", 21);
// result === 42
```

## Error Handling

Transforms can configure how errors are handled:

```wcl
transform with-errors {
    input  = "codec::json"
    output = "codec::json"

    on_parse_error = "skip"   // skip malformed records
    // or: "abort"            // stop the transform
    // or: "route"            // send to error stream
}
```

When using `"route"`, define an error stream to capture malformed records:

```wcl
@error_stream
errors : ParseError

map errors -> error_log {
    line   = in.error.line_number
    reason = in.error.message
}
```
