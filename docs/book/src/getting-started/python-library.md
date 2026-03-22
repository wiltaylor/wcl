# Using WCL as a Python Library

WCL has native Python bindings via PyO3. The `pywcl` package provides the full 11-phase parsing pipeline with Pythonic types — values come back as native `dict`, `list`, `int`, `str`, etc.

## Installation

Install from PyPI:

```bash
pip install pywcl
```

Or install from source (requires a Rust toolchain and `maturin`):

```bash
cd bindings/python
pip install -e .
```

## Parsing a WCL String

Use `wcl.parse()` to run the full pipeline and get a `Document`:

```python
import wcl

doc = wcl.parse("""
    server web-prod {
        host = "0.0.0.0"
        port = 8080
        debug = false
    }
""")

if doc.has_errors:
    for e in doc.errors:
        print(f"error: {e.message}")
else:
    print("Document parsed successfully")
```

## Parsing a WCL File

`parse_file()` reads and parses a file. It automatically sets the root directory to the file's parent so imports resolve correctly:

```python
doc = wcl.parse_file("config/main.wcl")

if doc.has_errors:
    for e in doc.errors:
        print(f"error: {e.message}")
```

Raises `IOError` if the file doesn't exist.

## Accessing Evaluated Values

After parsing, `doc.values` is a Python `dict` with all evaluated top-level attributes and blocks. Values are converted to native Python types:

```python
doc = wcl.parse("""
    name = "my-app"
    port = 8080
    tags = ["web", "prod"]
    debug = false
""")

print(doc.values["name"])   # "my-app" (str)
print(doc.values["port"])   # 8080 (int)
print(doc.values["tags"])   # ["web", "prod"] (list)
print(doc.values["debug"])  # False (bool)
```

WCL types map to Python types as follows:

| WCL Type | Python Type |
|----------|------------|
| `string` | `str` |
| `int` | `int` |
| `float` | `float` |
| `bool` | `bool` |
| `null` | `None` |
| `list` | `list` |
| `map` | `dict` |
| `set` | `set` (or `list` if items are unhashable) |

## Working with Blocks

Use `blocks()` and `blocks_of_type()` to access parsed blocks with resolved attributes:

```python
doc = wcl.parse("""
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
""")

# Get all blocks
blocks = doc.blocks()
print(f"Total blocks: {len(blocks)}")  # 3

# Get blocks of a specific type
servers = doc.blocks_of_type("server")
for s in servers:
    print(f"server id={s.id} host={s.get('host')} port={s.get('port')}")
```

Each `BlockRef` has the following properties:

```python
block.kind        # str — block type name (e.g. "server")
block.id          # str | None — inline ID (e.g. "web-prod")
block.attributes  # dict — evaluated attribute values (includes _args if inline args present)
block.children    # list[BlockRef] — nested child blocks
block.decorators  # list[Decorator] — decorators on this block
```

And these methods:

```python
block.get("port")              # attribute value, or None if missing
block.has_decorator("deprecated")  # True/False
```

## Working with Tables

Tables evaluate to a list of row dicts. Each row is a dict mapping column names to cell values:

```python
doc = wcl.parse("""
    table users {
        name : string
        age  : int
        | "alice" | 25 |
        | "bob"   | 30 |
    }
""")

users = doc.values["users"]
print(users)
# [{"name": "alice", "age": 25}, {"name": "bob", "age": 30}]

# Access individual rows
print(users[0]["name"])  # "alice"
print(users[1]["age"])   # 30
```

Tables inside blocks appear in the block's attributes:

```python
doc = wcl.parse("""
    service main {
        table config {
            key   : string
            value : int
            | "port" | 8080 |
        }
    }
""")

block = doc.blocks_of_type("service")[0]
print(block.get("config"))  # [{"key": "port", "value": 8080}]
```

## Running Queries

`doc.query()` accepts the same query syntax as the `wcl query` CLI command:

```python
doc = wcl.parse("""
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
""")

# Select all server blocks
all_servers = doc.query("server")

# Filter by attribute
prod = doc.query('server | .env == "prod"')

# Project a single attribute
ports = doc.query("server | .port")
print(ports)  # [8080, 9090, 3000]

# Filter and project
prod_ports = doc.query('server | .env == "prod" | .port')
print(prod_ports)  # [8080, 9090]

# Filter by comparison
high_ports = doc.query("server | .port > 8500")
print(high_ports)  # [BlockRef for svc-admin, BlockRef for svc-debug]
```

Raises `ValueError` if the query is invalid.

## Custom Functions

Register Python functions callable from WCL expressions by passing a `functions` dict:

```python
def double(args):
    return args[0] * 2

def greet(args):
    return f"Hello, {args[0]}!"

doc = wcl.parse("""
    result = double(21)
    message = greet("World")
""", functions={"double": double, "greet": greet})

print(doc.values["result"])   # 42
print(doc.values["message"])  # "Hello, World!"
```

Functions receive a single `args` list with native Python values and should return a native Python value. Errors propagate as diagnostics:

```python
def safe_div(args):
    if args[1] == 0:
        raise ValueError("division by zero")
    return args[0] / args[1]

doc = wcl.parse("result = safe_div(10, 0)", functions={"safe_div": safe_div})
assert doc.has_errors  # The ValueError becomes a diagnostic
```

Functions can return any supported type:

```python
def make_list(args):
    return [1, 2, 3]

def is_even(args):
    return args[0] % 2 == 0

def noop(args):
    return None
```

Custom functions also work in control flow expressions:

```python
def items(args):
    return [1, 2, 3]

doc = wcl.parse(
    "for item in items() { entry { value = item } }",
    functions={"items": items},
)
```

## Parse Options

All options are passed as keyword arguments to `parse()`:

```python
doc = wcl.parse(source,
    root_dir="./config",         # root directory for import resolution
    allow_imports=True,          # enable/disable imports (default: True)
    max_import_depth=32,         # max nested import depth (default: 32)
    max_macro_depth=64,          # max macro expansion depth (default: 64)
    max_loop_depth=32,           # max for-loop nesting (default: 32)
    max_iterations=10000,        # max total loop iterations (default: 10,000)
    functions={"my_fn": my_fn},  # custom functions
)
```

When processing untrusted input, disable imports to prevent file system access:

```python
doc = wcl.parse(untrusted_input, allow_imports=False)
```

## Library Files

Create `.wcl` library files manually and place them in `~/.local/share/wcl/lib/`. Use `wcl.list_libraries()` to list installed libraries. See the [Libraries guide](../guide/libraries.md) for details.

## Error Handling

The `Document` collects all diagnostics from every pipeline phase. Each `Diagnostic` has a severity, message, and optional error code:

```python
doc = wcl.parse("""
    server web {
        port = "not_a_number"
    }

    schema "server" {
        port: int
    }
""")

# Check for errors
if doc.has_errors:
    for e in doc.errors:
        code = f"[{e.code}] " if e.code else ""
        print(f"{e.severity}: {code}{e.message}")

# All diagnostics (errors + warnings)
for d in doc.diagnostics:
    print(f"{d.severity}: {d.message}")
```

The `Diagnostic` type:

```python
d.severity  # "error", "warning", "info", or "hint"
d.message   # str — the diagnostic message
d.code      # str | None — e.g. "E071" for type mismatch
repr(d)     # "Diagnostic(error: [E071] type mismatch: ...)"
```

Use `doc.has_errors` (bool) as a quick check, `doc.errors` for only errors, and `doc.diagnostics` for everything including warnings.

## Complete Example

Putting it all together — parse a configuration, validate it, query it, and extract values:

```python
import wcl

doc = wcl.parse("""
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
""")

# 1. Check for errors
if doc.has_errors:
    for e in doc.errors:
        print(f"{e.severity}: {e.message}")
    exit(1)

# 2. Query for all server ports
ports = doc.query("server | .port")
print(f"All ports: {ports}")  # [8080, 9090]

# 3. Iterate resolved blocks
for server in doc.blocks_of_type("server"):
    id = server.id or "(no id)"
    host = server.get("host")
    port = server.get("port")
    print(f"{id}: {host}:{port}")

# 4. Custom functions
def double(args):
    return args[0] * 2

doc2 = wcl.parse("result = double(21)", functions={"double": double})
print(f"result = {doc2.values['result']}")  # 42
```

## Building from Source

```bash
# Install development dependencies
cd wcl_python
python -m venv .venv
source .venv/bin/activate
pip install maturin pytest

# Build and install in development mode
maturin develop

# Run tests
pytest tests/ -v

# Or via just
just test-python
```

This requires the Rust toolchain and Python 3.8+.
