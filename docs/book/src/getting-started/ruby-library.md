# Using WCL as a Ruby Library

WCL has Ruby bindings powered by a WASM module and the [wasmtime](https://github.com/bytecodealliance/wasmtime-rb) runtime. The `wcl` gem provides the full 11-phase parsing pipeline with native Ruby types — values come back as `Hash`, `Array`, `Integer`, `String`, etc.

## Installation

Install from RubyGems:

```bash
gem install wcl
```

Or add to your `Gemfile`:

```ruby
gem "wcl"
```

## Parsing a WCL String

Use `Wcl.parse()` to run the full pipeline and get a `Document`:

```ruby
require "wcl"

doc = Wcl.parse(<<~WCL)
    server web-prod {
        host = "0.0.0.0"
        port = 8080
        debug = false
    }
WCL

if doc.has_errors?
  doc.errors.each { |e| puts "error: #{e.message}" }
else
  puts "Document parsed successfully"
end
```

## Parsing a WCL File

`Wcl.parse_file()` reads and parses a file. It automatically sets the root directory to the file's parent so imports resolve correctly:

```ruby
doc = Wcl.parse_file("config/main.wcl")

if doc.has_errors?
  doc.errors.each { |e| puts "error: #{e.message}" }
end
```

Raises `IOError` if the file doesn't exist.

## Accessing Evaluated Values

After parsing, `doc.values` is a Ruby `Hash` with all evaluated top-level attributes and blocks. Values are converted to native Ruby types:

```ruby
doc = Wcl.parse(<<~WCL)
    name = "my-app"
    port = 8080
    tags = ["web", "prod"]
    debug = false
WCL

puts doc.values["name"]   # "my-app" (String)
puts doc.values["port"]   # 8080 (Integer)
puts doc.values["tags"]   # ["web", "prod"] (Array)
puts doc.values["debug"]  # false (FalseClass)
```

WCL types map to Ruby types as follows:

| WCL Type | Ruby Type |
|----------|-----------|
| `string` | `String` |
| `int` | `Integer` |
| `float` | `Float` |
| `bool` | `true` / `false` |
| `null` | `nil` |
| `list` | `Array` |
| `map` | `Hash` |
| `set` | `Set` (or `Array` if items are unhashable) |

## Working with Blocks

Use `blocks` and `blocks_of_type` to access parsed blocks with resolved attributes:

```ruby
doc = Wcl.parse(<<~WCL)
    server web-prod {
        host = "0.0.0.0"
        port = 8080
    }

    server web-staging {
        host = "staging.internal"
        port = 8081
    }

    database main-db {
        host = "db.internal"
        port = 5432
    }
WCL

# Get all blocks
blocks = doc.blocks
puts "Total blocks: #{blocks.size}"  # 3

# Get blocks of a specific type
servers = doc.blocks_of_type("server")
servers.each do |s|
  puts "server id=#{s.id} host=#{s.get('host')} port=#{s.get('port')}"
end
```

Each `BlockRef` has the following properties:

```ruby
block.kind        # String — block type name (e.g. "server")
block.id          # String or nil — inline ID (e.g. "web-prod")
block.attributes  # Hash — evaluated attribute values (includes _args if inline args present)
block.children    # Array<BlockRef> — nested child blocks
block.decorators  # Array<Decorator> — decorators on this block
```

And these methods:

```ruby
block.get("port")                # attribute value, or nil if missing
block["port"]                    # same as get
block.has_decorator?("deprecated")  # true/false
```

## Running Queries

`doc.query()` accepts the same query syntax as the `wcl query` CLI command:

```ruby
doc = Wcl.parse(<<~WCL)
    server svc-api {
        port = 8080
        env = "prod"
    }

    server svc-admin {
        port = 9090
        env = "prod"
    }

    server svc-debug {
        port = 3000
        env = "dev"
    }
WCL

# Select all server blocks
all_servers = doc.query("server")

# Filter by attribute
prod = doc.query('server | .env == "prod"')

# Project a single attribute
ports = doc.query("server | .port")
puts ports.inspect  # [8080, 9090, 3000]

# Filter and project
prod_ports = doc.query('server | .env == "prod" | .port')
puts prod_ports.inspect  # [8080, 9090]

# Filter by comparison
high_ports = doc.query("server | .port > 8500")
```

Raises `Wcl::ValueError` if the query is invalid.

## Custom Functions

Register Ruby functions callable from WCL expressions by passing a `functions` hash:

```ruby
double = ->(args) { args[0] * 2 }
greet = ->(args) { "Hello, #{args[0]}!" }

doc = Wcl.parse(<<~WCL, functions: { "double" => double, "greet" => greet })
    result = double(21)
    message = greet("World")
WCL

puts doc.values["result"]   # 42
puts doc.values["message"]  # "Hello, World!"
```

Functions receive a single `args` array with native Ruby values and should return a native Ruby value. Errors propagate as diagnostics:

```ruby
safe_div = ->(args) {
  raise "division by zero" if args[1] == 0
  args[0].to_f / args[1]
}

doc = Wcl.parse('result = safe_div(10, 0)', functions: { "safe_div" => safe_div })
puts doc.has_errors?  # true — the error becomes a diagnostic
```

Functions can return any supported type:

```ruby
make_list = ->(_args) { [1, 2, 3] }
is_even = ->(args) { args[0] % 2 == 0 }
noop = ->(_args) { nil }
```

Custom functions also work in control flow expressions:

```ruby
items = ->(_args) { [1, 2, 3] }

doc = Wcl.parse(
  "for item in items() { entry { value = item } }",
  functions: { "items" => items }
)
```

## Parse Options

All options are passed as keyword arguments to `Wcl.parse`:

```ruby
doc = Wcl.parse(source,
  root_dir: "./config",           # root directory for import resolution
  allow_imports: true,            # enable/disable imports (default: true)
  max_import_depth: 32,           # max nested import depth (default: 32)
  max_macro_depth: 64,            # max macro expansion depth (default: 64)
  max_loop_depth: 32,             # max for-loop nesting (default: 32)
  max_iterations: 10000,          # max total loop iterations (default: 10,000)
  functions: { "my_fn" => my_fn } # custom functions
)
```

When processing untrusted input, disable imports to prevent file system access:

```ruby
doc = Wcl.parse(untrusted_input, allow_imports: false)
```

## Library Files

Create `.wcl` library files manually and place them in `~/.local/share/wcl/lib/`. See the [Libraries guide](../guide/libraries.md) for details.

## Error Handling

The `Document` collects all diagnostics from every pipeline phase. Each `Diagnostic` has a severity, message, and optional error code:

```ruby
doc = Wcl.parse(<<~WCL)
    server web {
        port = "not_a_number"
    }

    schema "server" {
        port: int
    }
WCL

# Check for errors
if doc.has_errors?
  doc.errors.each do |e|
    code = e.code ? "[#{e.code}] " : ""
    puts "#{e.severity}: #{code}#{e.message}"
  end
end

# All diagnostics (errors + warnings)
doc.diagnostics.each { |d| puts "#{d.severity}: #{d.message}" }
```

The `Diagnostic` type:

```ruby
d.severity  # "error", "warning", "info", or "hint"
d.message   # String — the diagnostic message
d.code      # String or nil — e.g. "E071" for type mismatch
d.error?    # true if severity is "error"
d.warning?  # true if severity is "warning"
d.inspect   # "#<Wcl::Diagnostic(error: [E071] type mismatch: ...)>"
```

Use `doc.has_errors?` as a quick check, `doc.errors` for only errors, and `doc.diagnostics` for everything including warnings.

## Complete Example

Putting it all together — parse a configuration, validate it, query it, and extract values:

```ruby
require "wcl"

doc = Wcl.parse(<<~WCL)
    schema "server" {
        port: int
        host: string @optional
    }

    server svc-api {
        port = 8080
        host = "api.internal"
    }

    server svc-admin {
        port = 9090
        host = "admin.internal"
    }
WCL

# 1. Check for errors
if doc.has_errors?
  doc.errors.each { |e| puts "#{e.severity}: #{e.message}" }
  exit 1
end

# 2. Query for all server ports
ports = doc.query("server | .port")
puts "All ports: #{ports.inspect}"  # [8080, 9090]

# 3. Iterate resolved blocks
doc.blocks_of_type("server").each do |server|
  id = server.id || "(no id)"
  host = server.get("host")
  port = server.get("port")
  puts "#{id}: #{host}:#{port}"
end

# 4. Custom functions
double = ->(args) { args[0] * 2 }
doc2 = Wcl.parse("result = double(21)", functions: { "double" => double })
puts "result = #{doc2.values['result']}"  # 42
```

## Building from Source

```bash
# Build WASM module and copy to gem
just build ruby-wasm

# Install dependencies
cd bindings/ruby
bundle install

# Run tests
bundle exec rake test

# Build gem
gem build wcl.gemspec

# Or via just
just test ruby
```

This requires the Rust toolchain (with `wasm32-wasip1` target) and Ruby 3.1+.
