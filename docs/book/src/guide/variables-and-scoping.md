# Variables and Scoping

WCL provides `let` bindings for named local values. Unlike block attributes, `let` bindings are private to their scope and are erased before serialization.

## `let` Bindings

A `let` binding assigns a name to an expression. It is visible within the rest of the enclosing scope:

```wcl
let base_port = 8000

server #web-1 {
  port = base_port        // 8000
}

server #web-2 {
  port = base_port + 1    // 8001
}
```

`let` bindings are **not** included in the evaluated output or in serialized JSON/TOML/YAML. They exist purely to reduce repetition.

## External Variable Overrides

WCL documents can be parameterized with external variables injected at parse time. External variables **override** any `let` binding of the same name, allowing a document to define defaults that the caller can replace.

### CLI

Use the `--var` flag (repeatable) on `eval` or `validate`:

```bash
wcl eval --var PORT=8080 --var DEBUG=true config.wcl
wcl validate --var PORT=8080 config.wcl
```

Values are auto-parsed: bare numbers become `int`/`float`, `true`/`false` become `bool`, `null` becomes `null`, quoted strings become `string`, and JSON arrays/objects are supported. An unquoted string that doesn't match any of the above is treated as a string.

### Rust API

```rust
let mut opts = ParseOptions::default();
opts.variables.insert("PORT".into(), Value::Int(8080));
let doc = wcl::parse(source, opts);
```

### Python

```python
doc = wcl.parse(source, variables={"PORT": 8080, "DEBUG": True})
```

### JavaScript (WASM)

```javascript
const doc = parse(source, { variables: { PORT: 8080, DEBUG: true } });
```

### Go

```go
doc, err := wcl.Parse(source, &wcl.ParseOptions{
    Variables: map[string]any{"PORT": 8080, "DEBUG": true},
})
```

### Ruby

```ruby
doc = Wcl.parse(source, variables: { "PORT" => 8080, "DEBUG" => true })
```

### .NET

```csharp
var doc = WclParser.Parse(source, new ParseOptions {
    Variables = new Dictionary<string, object> { ["PORT"] = 8080 }
});
```

### Zig

Pass variables as a JSON object string:

```zig
var doc = try wcl.parse(allocator, source, .{
    .variables_json = "{\"PORT\":8080}",
});
```

### Override Semantics

External variables override document `let` bindings of the same name. This lets a document define sensible defaults while still being fully parameterizable:

```wcl
let port = 8080       // default, overridden if --var port=... is set
let host = "localhost" // default

server {
    port = port
    host = host
}
```

```bash
wcl eval --var port=9090 config.wcl
# → { "server": { ... "port": 9090, "host": "localhost" } }
```

External variables are also available in control flow expressions:

```wcl
let regions = ["us"]  // default

for region in regions {
    server { name = region }
}
```

```bash
wcl eval --var 'regions=["us","eu","ap"]' config.wcl
# → produces 3 server blocks
```

## `export let` Bindings

An `export let` binding works like `let` but makes the name available to files that import this module:

```wcl
// config/defaults.wcl
export let default_timeout = 5000
export let default_retries = 3
```

```wcl
// app.wcl
import "config/defaults.wcl"

service {
  timeout = default_timeout    // 5000
  retries = default_retries    // 3
}
```

Like plain `let` bindings, exported bindings are erased before serialization — they are not present in the output document.

## Re-exporting Names

An `export name` statement re-exports a name that was imported from another module, making it available to the importer's importers:

```wcl
// lib/net.wcl
export let port = 8080

// lib/index.wcl
import "lib/net.wcl"
export port    // re-export to callers of lib/index.wcl

// app.wcl
import "lib/index.wcl"
service {
  port = port    // 8080 — reached through re-export chain
}
```

## Scope Model

WCL uses lexical scoping with three scope kinds:

| Scope kind   | Created by                          | Contains                          |
|--------------|-------------------------------------|-----------------------------------|
| Module scope | Each `.wcl` file                    | Top-level `let`, `export let`, blocks, attributes |
| Block scope  | Each `{ }` block body               | `let` bindings, nested blocks, attributes |
| Macro scope  | Each macro expansion                | Macro parameters, local bindings  |

Scopes form a chain. A name is resolved by walking the chain from innermost to outermost until a binding is found.

## Name Resolution Order

Given a reference `x` inside a block:

1. Look for `x` as a `let` binding in the current block scope.
2. Look for `x` as an attribute in the current block scope.
3. Walk up to the enclosing scope and repeat.
4. Check module-level `let` and `export let` bindings.
5. Check imported names.
6. If not found, report an unresolved reference error.

## Evaluation Order

WCL does **not** evaluate declarations in the order they appear. Instead, the evaluator performs a dependency-based topological sort: each name is evaluated after all names it depends on. This means you can reference a name before its declaration:

```wcl
full_url = "${scheme}://${host}:${port}"  // declared before its parts

let scheme = "https"
let host   = "api.example.com"
let port   = 443
```

Circular references are detected and reported as errors:

```wcl
let a = b + 1   // error: cyclic reference: a → b → a
let b = a - 1
```

## Shadowing

A `let` binding in an inner scope may shadow a name from an outer scope. This produces a warning by default:

```wcl
let timeout = 5000

service {
  let timeout = 1000    // warning: shadows outer binding "timeout"
  request_timeout = timeout
}
```

To suppress the warning for a specific block, use the `@allow(shadowing)` decorator:

```wcl
let timeout = 5000

@allow(shadowing)
service {
  let timeout = 1000    // no warning
  request_timeout = timeout
}
```

## Unused Variable Warnings

A `let` binding that is declared but never referenced produces an unused-variable warning:

```wcl
let unused = "hello"   // warning: unused variable "unused"
```

To suppress the warning, prefix the name with an underscore:

```wcl
let _unused = "hello"  // no warning
```

## Comparison: `let` vs `export let` vs Attribute

| Feature                       | `let`          | `export let`      | Attribute         |
|-------------------------------|----------------|-------------------|-------------------|
| Visible in current scope      | Yes            | Yes               | Yes               |
| Visible to importers          | No             | Yes               | No                |
| Appears in serialized output  | No             | No                | Yes               |
| Can be `query`-selected       | No             | No                | Yes               |
| Subject to schema validation  | No             | No                | Yes               |
| Can be `ref`-erenced          | No             | No                | Yes (block-level) |

## Example: Shared Constants

```wcl
// shared/network.wcl
export let internal_domain = "svc.cluster.local"
export let default_port    = 8080

// services/api.wcl
import "shared/network.wcl"

let service_name = "api-gateway"

server #primary {
  host = "${service_name}.${internal_domain}"
  port = default_port
}

server #secondary {
  host = "${service_name}-2.${internal_domain}"
  port = default_port + 1
}
```

After evaluation the `let` bindings and `export let` bindings are stripped; only `host` and `port` attributes appear in the output.
