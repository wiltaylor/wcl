# Zig Binding

Thin wrapper over `wcl_ffi` (the C API). Source: `bindings/zig/src/wcl.zig`, `bindings/zig/build.zig.zon`.

## Install

Add to `build.zig.zon`:

```zig
.{
    .name = .my_app,
    .version = "0.1.0",
    .minimum_zig_version = "0.14.0",
    .dependencies = .{
        .wcl = .{
            .url = "https://github.com/wiltaylor/wcl/archive/refs/tags/v0.0.0.tar.gz",
            .hash = "...",
        },
    },
}
```

In `build.zig`, add the module and link `libwcl`. The binding includes `wcl.h` via `@cImport`, so you need `libwcl.a` or `libwcl.so` on the link path.

Module name: `wcl`. Version: `0.0.0`, minimum Zig: `0.14.0`.

## Minimal Example

```zig
const std = @import("std");
const wcl = @import("wcl");

pub fn main() !void {
    const allocator = std.heap.page_allocator;

    var doc = try wcl.parse(allocator,
        \\server web {
        \\    host = "0.0.0.0"
        \\    port = 8080
        \\}
    , null);
    defer doc.deinit();

    if (doc.hasErrors()) {
        var errs = try doc.errors(allocator);
        defer errs.deinit();
        std.debug.print("errors: {}\n", .{errs.value.array.items.len});
        return;
    }

    var vals = try doc.values(allocator);
    defer vals.deinit();
    std.debug.print("{}\n", .{vals.value});
}
```

File:

```zig
var doc = try wcl.parseFile(allocator, "./config.wcl", null);
defer doc.deinit();
```

## Core API (`src/wcl.zig`)

| Symbol | Purpose |
|--------|---------|
| `pub fn parse(allocator, source, opts)` | Returns `Document` or `WclError` |
| `pub fn parseFile(allocator, path, opts)` | Reads via FFI (`wcl_ffi_parse_file`) |
| `pub fn parseWithFunctions(allocator, source, opts, fns)` | Custom functions |
| `pub fn listLibraries(allocator)` | Installed libraries |
| `Document.deinit()` | Release FFI document; idempotent |
| `Document.values(allocator)` | `json.Parsed(json.Value)` — caller deinits |
| `Document.valuesRaw(allocator)` | Raw JSON `[]const u8` (caller frees) |
| `Document.hasErrors()` | bool |
| `Document.errors(allocator)` | Parsed JSON of error diagnostics |
| `Document.diagnostics(allocator)` | Parsed JSON of all diagnostics |
| `Document.query(allocator, q)` | Parsed JSON; returns `WclError.QueryFailed` on `{"error"}` |
| `Document.blocks(allocator)` | Parsed JSON array |
| `Document.blocksOfType(allocator, kind)` | Parsed JSON array |

## ParseOptions

```zig
const opts = wcl.ParseOptions{
    .root_dir         = "/path",
    .allow_imports    = true,
    .max_import_depth = 32,
    .max_macro_depth  = 64,
    .max_loop_depth   = 8,
    .max_iterations   = 10_000,
    .variables_json   = "{\"PORT\":8080}",  // must be a valid JSON object literal
};
```

## Custom Functions

```zig
fn upper(allocator: std.mem.Allocator, args: std.json.Value) anyerror!std.json.Value {
    const s = args.array.items[0].string;
    var buf = try allocator.alloc(u8, s.len);
    for (s, 0..) |c, i| buf[i] = std.ascii.toUpper(c);
    return std.json.Value{ .string = buf };
}

var fns = std.StringHashMap(*const fn (std.mem.Allocator, std.json.Value) anyerror!std.json.Value).init(allocator);
defer fns.deinit();
try fns.put("upper", &upper);

var doc = try wcl.parseWithFunctions(allocator, "x = upper(\"hi\")", null, fns);
defer doc.deinit();
```

## Error Set

```zig
pub const WclError = error{
    ParseFailed,
    ParseFileFailed,
    JsonParseFailed,
    QueryFailed,
    LibraryListFailed,
    DocumentClosed,
};
```

All `Document` methods on a closed document return `WclError.DocumentClosed`.

## Value Mapping

All values pass through JSON; results are returned as `std.json.Parsed(std.json.Value)` which you must `deinit()`. Integer access is `obj.integer`, string access is `obj.string`, array is `obj.array.items`, etc.

## Gotchas

- **Ownership of `json.Parsed`** — every `Parsed` returned must be `deinit`-ed; loops that iterate blocks should scope each call tightly.
- Options are serialized to a null-terminated JSON string internally; `variables_json` is spliced in raw, so pass a syntactically valid JSON object.
- Calls into the FFI link against `libwcl`; ensure it's on the linker path and not stripped.
- `Document.deinit` is safe to call twice; internal `closed` flag prevents double-free.
