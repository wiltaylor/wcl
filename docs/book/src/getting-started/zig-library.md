# Using WCL as a Zig Library

WCL can be embedded into Zig programs via the `wcl` package. It uses a prebuilt static library under the hood, so you get the full 11-phase WCL pipeline without needing a Rust toolchain.

## Adding the Dependency

Add `wcl` to your `build.zig.zon`:

```zig
.dependencies = .{
    .wcl = .{
        .url = "https://github.com/wiltaylor/wcl/archive/refs/heads/main.tar.gz",
        .hash = "...",
    },
},
```

Then in your `build.zig`, import and use the module:

```zig
const wcl_dep = b.dependency("wcl", .{
    .target = target,
    .optimize = optimize,
});

// Add to your executable or library module
exe.root_module.addImport("wcl", wcl_dep.module("wcl"));
```

> **Note:** This package links a statically compiled Rust library via the C ABI. Prebuilt libraries are provided for Linux (x86\_64, aarch64), macOS (x86\_64, aarch64), and Windows (x86\_64).

## Parsing a WCL String

Use `wcl.parse()` to run the full pipeline and get a `Document`:

```zig
const std = @import("std");
const wcl = @import("wcl");

pub fn main() !void {
    var gpa: std.heap.GeneralPurposeAllocator(.{}) = .init;
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    var doc = try wcl.parse(allocator,
        \\server web-prod {
        \\    host = "0.0.0.0"
        \\    port = 8080
        \\    debug = false
        \\}
    , null);
    defer doc.deinit();

    if (doc.hasErrors()) {
        var errs = try doc.errors(allocator);
        defer errs.deinit();
        for (errs.value.array.items) |item| {
            const msg = item.object.get("message").?.string;
            std.debug.print("error: {s}\n", .{msg});
        }
    } else {
        std.debug.print("Document parsed successfully\n", .{});
    }
}
```

Always call `doc.deinit()` when you're done with a document. This releases the underlying Rust resources.

## Parsing a WCL File

`parseFile` reads and parses a file. It automatically sets `root_dir` to the file's parent directory so imports resolve correctly:

```zig
var doc = try wcl.parseFile(allocator, "config/main.wcl", null);
defer doc.deinit();
```

## Accessing Evaluated Values

After parsing, `values()` returns a parsed `std.json.Value` containing all evaluated top-level attributes and blocks:

```zig
var doc = try wcl.parse(allocator,
    \\name = "my-app"
    \\port = 8080
    \\tags = ["web", "prod"]
, null);
defer doc.deinit();

var vals = try doc.values(allocator);
defer vals.deinit();

const obj = vals.value.object;
const name = obj.get("name").?.string;    // "my-app"
const port = obj.get("port").?.integer;   // 8080
```

You can also get the raw JSON string with `valuesRaw()`:

```zig
const raw = try doc.valuesRaw(allocator);
defer allocator.free(raw);
```

> **Type mapping:** Values cross the FFI boundary as JSON. In the `std.json.Value` union: strings are `.string`, integers are `.integer` (`i64`), floats are `.float` (`f64`), booleans are `.bool`, arrays are `.array`, objects are `.object`, and null is `.null`.

## Working with Blocks

Use `blocks()` and `blocksOfType()` to access parsed blocks with their resolved attributes:

```zig
var doc = try wcl.parse(allocator,
    \\server web-prod {
    \\    host = "0.0.0.0"
    \\    port = 8080
    \\}
    \\server web-staging {
    \\    host = "staging.internal"
    \\    port = 8081
    \\}
    \\database main-db {
    \\    host = "db.internal"
    \\    port = 5432
    \\}
, null);
defer doc.deinit();

// Get all blocks
var all = try doc.blocks(allocator);
defer all.deinit();
std.debug.print("Total blocks: {d}\n", .{all.value.array.items.len}); // 3

// Get blocks of a specific type
var servers = try doc.blocksOfType(allocator, "server");
defer servers.deinit();
for (servers.value.array.items) |block| {
    const obj = block.object;
    const id = if (obj.get("id")) |v| v.string else "(no id)";
    const attrs = obj.get("attributes").?.object;
    std.debug.print("server id={s} host={s}\n", .{ id, attrs.get("host").?.string });
}
```

Each block in the JSON array has the following fields: `kind` (string), `id` (string or null), `labels` (array), `attributes` (object), `children` (array), and `decorators` (array).

## Running Queries

`query()` accepts the same query syntax as the `wcl query` CLI command:

```zig
var doc = try wcl.parse(allocator,
    \\server svc-api {
    \\    port = 8080
    \\    env = "prod"
    \\}
    \\server svc-admin {
    \\    port = 9090
    \\    env = "prod"
    \\}
    \\server svc-debug {
    \\    port = 3000
    \\    env = "dev"
    \\}
, null);
defer doc.deinit();

// Select all server ports
var result = try doc.query(allocator, "server | .port");
defer result.deinit();

// The result is {"ok": <value>} — access via .object.get("ok")
const ports = result.value.object.get("ok").?;
// ports.array.items contains [8080, 9090, 3000]
```

Query syntax supports filtering (`server | .env == "prod"`), projection (`server | .port`), and selection by ID (`server#svc-api`).

## Custom Functions

You can register Zig functions that are callable from WCL expressions using `parseWithFunctions`:

```zig
const std = @import("std");
const wcl = @import("wcl");
const json = std.json;
const Allocator = std.mem.Allocator;

fn doubleImpl(_: Allocator, args: json.Value) !json.Value {
    const n = args.array.items[0].integer;
    return json.Value{ .integer = n * 2 };
}

pub fn main() !void {
    var gpa: std.heap.GeneralPurposeAllocator(.{}) = .init;
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    var functions = std.StringHashMap(
        *const fn (Allocator, json.Value) anyerror!json.Value,
    ).init(allocator);
    defer functions.deinit();
    try functions.put("double", &doubleImpl);

    var doc = try wcl.parseWithFunctions(allocator,
        \\result = double(21)
    , null, functions);
    defer doc.deinit();

    var vals = try doc.values(allocator);
    defer vals.deinit();
    // vals.value.object.get("result").?.integer == 42
}
```

Arguments arrive as a `std.json.Value` (the full JSON array of arguments). Return a `json.Value` for the result, or return an error to signal failure.

## Parse Options

`ParseOptions` controls the parser behavior:

```zig
var doc = try wcl.parse(allocator, source, wcl.ParseOptions{
    // Root directory for import path resolution
    .root_dir = "./config",

    // Whether imports are allowed (null = default true)
    .allow_imports = false,

    // Maximum depth for nested imports (default: 32)
    .max_import_depth = 32,

    // Maximum macro expansion depth (default: 64)
    .max_macro_depth = 64,

    // Maximum for-loop nesting depth (default: 32)
    .max_loop_depth = 32,

    // Maximum total iterations across all for loops (default: 10,000)
    .max_iterations = 10000,
});
```

When processing untrusted WCL input, disable imports to prevent file system access:

```zig
var doc = try wcl.parse(allocator, untrusted_input, wcl.ParseOptions{
    .allow_imports = false,
});
```

Pass `null` for default options:

```zig
var doc = try wcl.parse(allocator, source, null);
```

## Library Files

Create `.wcl` library files manually and place them in `~/.local/share/wcl/lib/`. Use `wcl.listLibraries()` to list installed libraries. See the [Libraries guide](../guide/libraries.md) for details.

## Error Handling

The `Document` collects all diagnostics from every pipeline phase. Use `diagnostics()` to get all diagnostics or `errors()` to get only errors:

```zig
var doc = try wcl.parse(allocator,
    \\server web {
    \\    port = "not_a_number"
    \\}
    \\schema "server" {
    \\    port: int
    \\}
, null);
defer doc.deinit();

var diags = try doc.diagnostics(allocator);
defer diags.deinit();

for (diags.value.array.items) |item| {
    const obj = item.object;
    const severity = obj.get("severity").?.string;
    const message = obj.get("message").?.string;
    const code = if (obj.get("code")) |v| switch (v) {
        .string => |s| s,
        else => "",
    } else "";
    std.debug.print("{s}: [{s}] {s}\n", .{ severity, code, message });
}
```

WCL API functions return Zig error unions. The `WclError` error set includes:

| Error | Meaning |
|-------|---------|
| `ParseFailed` | Source string parsing returned null |
| `ParseFileFailed` | File parsing returned null (I/O error or invalid path) |
| `QueryFailed` | Query execution returned an error |
| `LibraryListFailed` | Library listing failed |
| `DocumentClosed` | Operation on a closed document |

## Complete Example

Putting it all together -- parse a configuration, validate it, query it, and extract values:

```zig
const std = @import("std");
const wcl = @import("wcl");

pub fn main() !void {
    var gpa: std.heap.GeneralPurposeAllocator(.{}) = .init;
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    var doc = try wcl.parse(allocator,
        \\schema "server" {
        \\    port: int
        \\    host: string @optional
        \\}
        \\
        \\server svc-api {
        \\    port = 8080
        \\    host = "api.internal"
        \\}
        \\
        \\server svc-admin {
        \\    port = 9090
        \\    host = "admin.internal"
        \\}
    , null);
    defer doc.deinit();

    // 1. Check for errors
    if (doc.hasErrors()) {
        var errs = try doc.errors(allocator);
        defer errs.deinit();
        for (errs.value.array.items) |item| {
            const msg = item.object.get("message").?.string;
            std.debug.print("error: {s}\n", .{msg});
        }
        return error.ValidationFailed;
    }

    // 2. Query for all server ports
    var ports = try doc.query(allocator, "server | .port");
    defer ports.deinit();
    const ok = ports.value.object.get("ok").?;
    std.debug.print("All ports: ", .{});
    for (ok.array.items) |p| {
        std.debug.print("{d} ", .{p.integer});
    }
    std.debug.print("\n", .{});

    // 3. Iterate resolved blocks
    var servers = try doc.blocksOfType(allocator, "server");
    defer servers.deinit();
    for (servers.value.array.items) |block| {
        const obj = block.object;
        const id = if (obj.get("id")) |v| v.string else "(no id)";
        const attrs = obj.get("attributes").?.object;
        std.debug.print("{s}: {s}:{d}\n", .{
            id,
            attrs.get("host").?.string,
            attrs.get("port").?.integer,
        });
    }
}
```

## Building from Source

If you want to rebuild the static library from the Rust source (e.g., after modifying the WCL codebase), run:

```bash
# Using just (recommended)
just build zig        # native platform only
just build zig-all    # all platforms (requires cargo-zigbuild + zig)
```
