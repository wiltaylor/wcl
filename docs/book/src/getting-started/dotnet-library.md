# Using WCL as a .NET Library

WCL can be embedded into .NET programs via the `Wcl` package. It uses a native shared library (P/Invoke) under the hood, so you get the full 11-phase WCL pipeline without needing a Rust toolchain.

## Adding the Dependency

Add a project reference to the WCL library:

```xml
<ProjectReference Include="../wcl_dotnet/src/Wcl/Wcl.csproj" />
```

The library targets `netstandard2.1` and works with .NET Core 3.0+ and .NET 5+.

> **Note:** This package uses P/Invoke with a native shared library (`libwcl_ffi.so` / `.dylib` / `.dll`). The native library must be present in the `runtimes/{rid}/native/` directory or alongside your application binary.

## Parsing a WCL String

Use `WclParser.Parse()` to run the full pipeline and get a `WclDocument`:

```csharp
using Wcl;

using var doc = WclParser.Parse(@"
    server web-prod {
        host = ""0.0.0.0""
        port = 8080
        debug = false
    }
");

if (doc.HasErrors())
{
    foreach (var diag in doc.Errors())
    {
        Console.Error.WriteLine($"error: {diag.Message}");
    }
}
else
{
    Console.WriteLine("Document parsed successfully");
}
```

`WclDocument` implements `IDisposable` and should be disposed when no longer needed. A finalizer is set as a safety net, but explicit disposal (via `using` or `Dispose()`) is preferred.

## Parsing a WCL File

`ParseFile` reads and parses a file. It automatically sets `RootDir` to the file's parent directory so imports resolve correctly:

```csharp
using Wcl;

using var doc = WclParser.ParseFile("config/main.wcl");
```

To override parse options:

```csharp
var options = new ParseOptions
{
    RootDir = "./config"
};

using var doc = WclParser.ParseFile("config/main.wcl", options);
```

## Accessing Evaluated Values

After parsing, `doc.Values` is an `OrderedMap<string, WclValue>` containing all evaluated top-level attributes and blocks:

```csharp
using Wcl;
using Wcl.Eval;

using var doc = WclParser.Parse(@"
    name = ""my-app""
    port = 8080
    tags = [""web"", ""prod""]
");

// Access scalar values
if (doc.Values.TryGetValue("name", out var name))
    Console.WriteLine($"name: {name.AsString()}");

if (doc.Values.TryGetValue("port", out var port))
    Console.WriteLine($"port: {port.AsInt()}");

// Access list values
if (doc.Values.TryGetValue("tags", out var tags))
{
    foreach (var tag in tags.AsList())
        Console.WriteLine($"tag: {tag.AsString()}");
}
```

`WclValue` is a sealed type with these value kinds:

| Kind | Factory | Accessor |
|------|---------|----------|
| `String` | `WclValue.NewString("...")` | `.AsString()` |
| `Int` | `WclValue.NewInt(42)` | `.AsInt()` |
| `Float` | `WclValue.NewFloat(3.14)` | `.AsFloat()` |
| `Bool` | `WclValue.NewBool(true)` | `.AsBool()` |
| `Null` | `WclValue.Null` | `.IsNull` |
| `List` | `WclValue.NewList(...)` | `.AsList()` |
| `Map` | `WclValue.NewMap(...)` | `.AsMap()` |
| `Set` | `WclValue.NewSet(...)` | `.AsSet()` |
| `BlockRef` | `WclValue.NewBlockRef(...)` | `.AsBlockRef()` |

Safe accessors like `.TryAsString()` return `null` instead of throwing on type mismatch.

## Working with Blocks

Use `Blocks()` and `BlocksOfType()` to access blocks with their resolved attribute values:

```csharp
using var doc = WclParser.Parse(@"
    server web-prod {
        host = ""0.0.0.0""
        port = 8080
    }

    server web-staging {
        host = ""staging.internal""
        port = 8081
    }

    database main-db {
        host = ""db.internal""
        port = 5432
    }
");

// Get all blocks as resolved BlockRefs
var blocks = doc.Blocks();
Console.WriteLine($"Total blocks: {blocks.Count}"); // 3

// Get blocks of a specific type
var servers = doc.BlocksOfType("server");
foreach (var s in servers)
{
    Console.WriteLine($"server id={s.Id} host={s.Get("host")} port={s.Get("port")}");
}
```

Each `BlockRef` provides:

```csharp
public class BlockRef
{
    public string Kind { get; }
    public string? Id { get; }
    public List<string> Labels { get; }
    public OrderedMap<string, WclValue> Attributes { get; }
    public List<BlockRef> Children { get; }
    public List<DecoratorValue> Decorators { get; }

    public WclValue? Get(string key);           // safe attribute access
    public bool HasDecorator(string name);
    public DecoratorValue? GetDecorator(string name);
}
```

## Running Queries

`Query()` accepts the same query syntax as the `wcl query` CLI command:

```csharp
using var doc = WclParser.Parse(@"
    server svc-api {
        port = 8080
        env = ""prod""
    }

    server svc-admin {
        port = 9090
        env = ""prod""
    }

    server svc-debug {
        port = 3000
        env = ""dev""
    }
");

// Select all server blocks
var all = doc.Query("server");

// Filter by attribute
var prod = doc.Query(@"server | .env == ""prod""");

// Project a single attribute
var ports = doc.Query("server | .port");
// → List [8080, 9090, 3000]

// Filter and project
var prodPorts = doc.Query(@"server | .env == ""prod"" | .port");
// → List [8080, 9090]

// Select by ID
var api = doc.Query("server#svc-api");
```

## Custom Functions

You can register C# functions that are callable from WCL expressions. This lets your application extend WCL with domain-specific logic:

```csharp
using Wcl;
using Wcl.Eval;

var opts = new ParseOptions
{
    Functions = new Dictionary<string, Func<WclValue[], WclValue>>
    {
        ["double"] = args => WclValue.NewInt(args[0].AsInt() * 2),
        ["greet"] = args => WclValue.NewString($"Hello, {args[0].AsString()}!"),
    }
};

using var doc = WclParser.Parse(@"
    result = double(21)
    message = greet(""World"")
", opts);

Console.WriteLine(doc.Values["result"].AsInt());     // 42
Console.WriteLine(doc.Values["message"].AsString());  // "Hello, World!"
```

Arguments and return values are serialized as JSON across the FFI boundary. Functions receive `WclValue[]` arguments and must return a `WclValue`. Use the factory methods to create return values.

To signal a function failure, throw an exception:

```csharp
["safe_div"] = args =>
{
    var a = args[0].AsFloat();
    var b = args[1].AsFloat();
    if (b == 0) throw new Exception("division by zero");
    return WclValue.NewFloat(a / b);
}
```

## Deserializing into C# Types

### With `FromString<T>`

Deserialize a WCL string directly into a C# type:

```csharp
using Wcl;

public class AppConfig
{
    public string Name { get; set; }
    public long Port { get; set; }
    public bool Debug { get; set; }
}

var config = WclParser.FromString<AppConfig>(@"
    name = ""my-app""
    port = 8080
    debug = false
");

Console.WriteLine($"{config.Name} on port {config.Port}");
```

`FromString<T>` throws if there are parse errors.

### Serializing to WCL

Convert a C# object back to WCL text:

```csharp
var config = new AppConfig { Name = "my-app", Port = 8080, Debug = false };

var wcl = WclParser.ToString(config);
// name = "my-app"
// port = 8080
// debug = false

var pretty = WclParser.ToStringPretty(config);
// Same but with indentation for nested structures
```

## Parse Options

`ParseOptions` controls the pipeline behavior. All fields are nullable — only set values are sent to the engine:

```csharp
var options = new ParseOptions
{
    // Root directory for import path resolution
    RootDir = "./config",

    // Whether imports are allowed (default: true)
    AllowImports = true,

    // Maximum depth for nested imports (default: 32)
    MaxImportDepth = 32,

    // Maximum macro expansion depth (default: 64)
    MaxMacroDepth = 64,

    // Maximum for-loop nesting depth (default: 32)
    MaxLoopDepth = 32,

    // Maximum total iterations across all for loops (default: 10,000)
    MaxIterations = 10000,

    // Custom functions callable from WCL expressions
    Functions = new Dictionary<string, Func<WclValue[], WclValue>> { ... },
};
```

When processing untrusted input, disable imports to prevent file system access:

```csharp
var options = new ParseOptions { AllowImports = false };
using var doc = WclParser.Parse(untrustedInput, options);
```

Pass `null` for default options:

```csharp
using var doc = WclParser.Parse(source, null);
```

## Library Management

Install, list, and uninstall WCL library files programmatically:

```csharp
using Wcl.Library;

// Install a library file
var path = LibraryManager.Install("myapp.wcl", @"
    schema ""config"" {
        port: int
        host: string @optional
    }

    declare my_fn(input: string) -> string
");
Console.WriteLine($"Installed to: {path}");

// List installed libraries
var libs = LibraryManager.List();
foreach (var lib in libs)
    Console.WriteLine(lib);

// Uninstall
LibraryManager.Uninstall("myapp.wcl");
```

After installation, WCL files can use `import <myapp.wcl>` to access the schemas and function declarations.

## Error Handling

The `WclDocument` collects all diagnostics from every pipeline phase. Each `Diagnostic` includes a severity, message, and optional error code:

```csharp
using var doc = WclParser.Parse(@"
    server web {
        port = ""not_a_number""
    }

    schema ""server"" {
        port: int
    }
");

foreach (var diag in doc.Diagnostics)
{
    var severity = diag.IsError ? "ERROR" : "WARN";
    var code = diag.Code ?? "----";
    Console.Error.WriteLine($"[{severity}] {code}: {diag.Message}");
}
```

The `Diagnostic` type:

```csharp
public class Diagnostic
{
    public string Severity { get; }   // "error", "warning", "info", "hint"
    public string Message { get; }
    public string? Code { get; }      // e.g. "E071" for type mismatch
    public bool IsError { get; }
}
```

## Thread Safety

Documents are safe to use from multiple threads. All methods acquire a lock internally, and values are cached after first access:

```csharp
using var doc = WclParser.Parse("x = 42");

var tasks = Enumerable.Range(0, 10).Select(_ => Task.Run(() =>
{
    var values = doc.Values;
    Console.WriteLine(values["x"].AsInt()); // 42
}));

await Task.WhenAll(tasks);
```

## Complete Example

Putting it all together — parse a configuration, validate it, query it, and extract values:

```csharp
using Wcl;
using Wcl.Eval;

using var doc = WclParser.Parse(@"
    schema ""server"" {
        port: int
        host: string @optional
    }

    server svc-api {
        port = 8080
        host = ""api.internal""
    }

    server svc-admin {
        port = 9090
        host = ""admin.internal""
    }
");

// 1. Check for errors
if (doc.HasErrors())
{
    foreach (var e in doc.Errors())
        Console.Error.WriteLine(e.Message);
    Environment.Exit(1);
}

// 2. Query for all server ports
var ports = doc.Query("server | .port");
Console.WriteLine($"All ports: {ports}");

// 3. Iterate resolved blocks
foreach (var server in doc.BlocksOfType("server"))
{
    var id = server.Id ?? "(no id)";
    var host = server.Get("host");
    var port = server.Get("port");
    Console.WriteLine($"{id}: {host}:{port}");
}

// 4. Custom functions
var opts = new ParseOptions
{
    Functions = new Dictionary<string, Func<WclValue[], WclValue>>
    {
        ["double"] = args => WclValue.NewInt(args[0].AsInt() * 2)
    }
};

using var doc2 = WclParser.Parse("result = double(21)", opts);
Console.WriteLine($"result = {doc2.Values["result"].AsInt()}"); // 42
```

## Building from Source

```bash
# Build the native library and .NET project
just build dotnet

# Run .NET tests
just test dotnet
```

This requires the .NET SDK (6.0+) and a Rust toolchain (for building the native library).
