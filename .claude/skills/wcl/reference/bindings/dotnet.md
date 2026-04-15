# .NET / C# Binding

WASM-based, using a managed WASM runtime. Source: `bindings/dotnet/src/Wcl/Wcl.cs`, `WclDocument.cs`, `ParseOptions.cs`.

## Install

```bash
dotnet add package Wcl
```

Package id: `Wcl`. Namespace: `Wcl`.

## Minimal Example

```csharp
using Wcl;

var doc = WclParser.Parse(@"
server web {
    host = ""0.0.0.0""
    port = 8080
}");

if (doc.HasErrors())
{
    foreach (var d in doc.Errors())
        Console.WriteLine($"{d.Code}: {d.Message}");
    return;
}

foreach (var kv in doc.Values)
    Console.WriteLine($"{kv.Key} = {kv.Value}");
```

File:

```csharp
using var doc = WclParser.ParseFile("./config.wcl");
```

## Core API

| Symbol | Purpose |
|--------|---------|
| `WclParser.Parse(string source, ParseOptions? options = null)` | Parse + evaluate. `Wcl.cs:13` |
| `WclParser.ParseFile(string path, ParseOptions? options = null)` | Reads file; sets `RootDir` to parent |
| `WclParser.FromString<T>(string source, ParseOptions? options = null)` | Parse + deserialize to type `T`; throws on errors |
| `WclParser.ToString<T>(T value)` | Serialize a POCO to WCL text |
| `WclParser.ToStringPretty<T>(T value)` | Formatted serialization |
| `WclDocument.Values` | `Dictionary<string, WclValue>` |
| `WclDocument.HasErrors()` / `.Errors()` / `.Diagnostics()` | Diagnostics |
| `WclDocument.Query(string)` | Run a WCL query |
| `WclDocument.Blocks()` / `.BlocksOfType(string)` | Block access |
| `WclDocument.Dispose()` | Release WASM handle (implements `IDisposable`) |

## ParseOptions

```csharp
var opts = new ParseOptions {
    RootDir = "/path",
    AllowImports = true,
    MaxImportDepth = 32,
    MaxMacroDepth = 64,
    MaxLoopDepth = 8,
    MaxIterations = 10000,
    Variables = new Dictionary<string, object> { ["PORT"] = 8080 },
    Functions = new Dictionary<string, Func<WclValue[], WclValue>> {
        ["upper_rev"] = args => WclValue.NewString(((string)args[0]).ToUpper())
    }
};
```

## Serde (POCO ↔ WCL)

```csharp
public record Server(string Host, int Port);

var src = @"host = ""0.0.0.0""; port = 8080";
var srv = WclParser.FromString<Server>(src);

var back = WclParser.ToStringPretty(srv);
```

Uses `Wcl.Serde.WclDeserializer` / `WclSerializer` internally.

## Value Type Mapping

| WCL | `WclValue` (.NET) |
|-----|-------------------|
| string | `string` |
| int | `long` |
| float | `double` |
| bool | `bool` |
| null | `null` |
| list | `List<WclValue>` |
| map | `Dictionary<string, WclValue>` |
| block | `WclValue.Block` |
| date / duration | string (ISO) |
| symbol | string (`:NAME`) |

## Error Handling

- `WclParser.Parse` — always returns a `WclDocument`; inspect `HasErrors()`.
- `WclParser.FromString<T>` — **throws** `Exception` if `HasErrors()` is true.
- `WclDocument.Query` — throws for query evaluation errors.
- Always wrap `WclDocument` in a `using` block or call `Dispose()` to free the WASM handle.

## Gotchas

- Project targets .NET with a bundled WASM runtime; no native dependencies.
- Custom functions are set globally on the runtime while `Parse` runs, then cleared in a `finally` block.
- Thread safety: the runtime is a singleton; `Parse` takes a lock during the callback-install / parse / clear sequence, so concurrent calls serialize.
- Integer precision: the WASM boundary uses JSON — `long` round-trips as JSON numbers and may lose precision above 2^53. Use serde derivation for strict typing.
