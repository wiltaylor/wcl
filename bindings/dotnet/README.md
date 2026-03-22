# WclLang — .NET bindings for WCL

.NET bindings for [WCL (Wil's Configuration Language)](https://wcl.dev), powered by a WASM runtime.

## Install

```bash
dotnet add package WclLang
```

## Usage

```csharp
using Wcl;

using var doc = WclParser.Parse("""
    server web {
        port = 8080
        host = "localhost"
    }
""");

var values = doc.Values();
Console.WriteLine(values["server"]);

var servers = doc.BlocksOfType("server");
Console.WriteLine($"Found {servers.Count} server(s)");
```

## Links

- **Website**: [wcl.dev](https://wcl.dev)
- **Documentation**: [wcl.dev/docs](https://wcl.dev/docs/)
- **GitHub**: [github.com/wiltaylor/wcl](https://github.com/wiltaylor/wcl)

## License

MIT
