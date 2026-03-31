# Using WCL as a Go Library

WCL can be embedded into Go programs via the `wcl_go` package. It uses an embedded WASM module with [wazero](https://wazero.io/) (a pure Go, zero-dependency WebAssembly runtime), so you get the full 11-phase WCL pipeline without needing a Rust toolchain or C compiler.

## Adding the Dependency

Add the module to your project:

```bash
go get github.com/wiltaylor/wcl/bindings/go
```

Then import it:

```go
import wcl "github.com/wiltaylor/wcl/bindings/go"
```

No CGo or C compiler required — the WASM module is embedded directly in the Go binary.

## Parsing a WCL String

Use `wcl.Parse()` to run the full pipeline and get a `Document`:

```go
package main

import (
    "fmt"
    "log"

    wcl "github.com/wiltaylor/wcl/bindings/go"
)

func main() {
    doc, err := wcl.Parse(`
        server web-prod {
            host = "0.0.0.0"
            port = 8080
            debug = false
        }
    `, nil)
    if err != nil {
        log.Fatal(err)
    }
    defer doc.Close()

    if doc.HasErrors() {
        errs, _ := doc.Errors()
        for _, e := range errs {
            fmt.Printf("error: %s\n", e.Message)
        }
    } else {
        fmt.Println("Document parsed successfully")
    }
}
```

Always call `doc.Close()` when you're done with a document. A finalizer is set as a safety net, but explicit cleanup is preferred.

## Parsing a WCL File

`ParseFile` reads and parses a file. It automatically sets `RootDir` to the file's parent directory so imports resolve correctly:

```go
doc, err := wcl.ParseFile("config/main.wcl", nil)
if err != nil {
    log.Fatal(err)
}
defer doc.Close()
```

> **Note:** File reading happens on the Go side; the WASM module does not have direct filesystem access. Imports within parsed files that reference other files on disk will not resolve in the WASM environment.

## Accessing Evaluated Values

After parsing, `Values()` returns an ordered map of all evaluated top-level attributes and blocks:

```go
doc, _ := wcl.Parse(`
    name = "my-app"
    port = 8080
    tags = ["web", "prod"]
`, nil)
defer doc.Close()

values, err := doc.Values()
if err != nil {
    log.Fatal(err)
}

fmt.Println(values["name"])  // "my-app"
fmt.Println(values["port"])  // 8080 (float64 — JSON numbers)
fmt.Println(values["tags"])  // ["web", "prod"]
```

> **Type note:** Values cross the WASM boundary as JSON, so numbers arrive as `float64`, strings as `string`, booleans as `bool`, lists as `[]any`, and maps as `map[string]any`.

## Working with Blocks

Use `Blocks()` and `BlocksOfType()` to access parsed blocks with their resolved attributes:

```go
doc, _ := wcl.Parse(`
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
`, nil)
defer doc.Close()

// Get all blocks
blocks, _ := doc.Blocks()
fmt.Printf("Total blocks: %d\n", len(blocks)) // 3

// Get blocks of a specific type
servers, _ := doc.BlocksOfType("server")
for _, s := range servers {
    fmt.Printf("server id=%v host=%v port=%v\n",
        s.ID, s.Attributes["host"], s.Attributes["port"])
}
```

Each `BlockRef` has the following fields:

```go
type BlockRef struct {
    Kind       string         // block type name (e.g. "server")
    ID         *string        // inline ID (e.g. "web-prod"), nil if none
    Attributes map[string]any // evaluated attribute values (includes _args if inline args present)
    Children   []BlockRef     // nested child blocks
    Decorators []Decorator    // decorators applied to this block
}
```

## Working with Tables

Tables evaluate to a slice of row maps (`[]map[string]interface{}`). Each row is a map from column name to cell value:

```go
doc, _ := wcl.Parse(`
    table users {
        name : string
        age  : i64
        | "alice" | 25 |
        | "bob"   | 30 |
    }
`)

users := doc.Values["users"].([]interface{})
row0 := users[0].(map[string]interface{})
fmt.Println(row0["name"]) // "alice"
fmt.Println(row0["age"])  // 25
```

Tables inside blocks appear in the block's attributes map.

## Running Queries

`Query()` accepts the same query syntax as the `wcl query` CLI command:

```go
doc, _ := wcl.Parse(`
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
`, nil)
defer doc.Close()

// Select all server blocks
all, _ := doc.Query("server")

// Filter by attribute
prod, _ := doc.Query(`server | .env == "prod"`)

// Project a single attribute
ports, _ := doc.Query("server | .port")
fmt.Println(ports) // [8080, 9090, 3000]

// Filter and project
prodPorts, _ := doc.Query(`server | .env == "prod" | .port`)
fmt.Println(prodPorts) // [8080, 9090]

// Select by ID
api, _ := doc.Query("server#svc-api")
```

## Custom Functions

You can register Go functions that are callable from WCL expressions. This lets your application extend WCL with domain-specific logic:

```go
opts := &wcl.ParseOptions{
    Functions: map[string]func([]any) (any, error){
        "double": func(args []any) (any, error) {
            n, ok := args[0].(float64)
            if !ok {
                return nil, fmt.Errorf("expected number")
            }
            return n * 2, nil
        },
        "greet": func(args []any) (any, error) {
            name, ok := args[0].(string)
            if !ok {
                return nil, fmt.Errorf("expected string")
            }
            return fmt.Sprintf("Hello, %s!", name), nil
        },
    },
}

doc, err := wcl.Parse(`
    result = double(21)
    message = greet("World")
`, opts)
if err != nil {
    log.Fatal(err)
}
defer doc.Close()

values, _ := doc.Values()
fmt.Println(values["result"])  // 42
fmt.Println(values["message"]) // "Hello, World!"
```

Arguments and return values are serialized as JSON across the WASM boundary. Numbers are `float64`, strings are `string`, lists are `[]any`, maps are `map[string]any`, booleans are `bool`, and `nil` maps to null.

Return an error to signal a function failure:

```go
"safe_div": func(args []any) (any, error) {
    a, b := args[0].(float64), args[1].(float64)
    if b == 0 {
        return nil, fmt.Errorf("division by zero")
    }
    return a / b, nil
},
```

## Parse Options

`ParseOptions` controls the parser behavior:

```go
allowImports := true

opts := &wcl.ParseOptions{
    // Root directory for import path resolution
    RootDir: "./config",

    // Whether imports are allowed (pointer for optional; nil = default true)
    AllowImports: &allowImports,

    // Maximum depth for nested imports (default: 32)
    MaxImportDepth: 32,

    // Maximum macro expansion depth (default: 64)
    MaxMacroDepth: 64,

    // Maximum for-loop nesting depth (default: 32)
    MaxLoopDepth: 32,

    // Maximum total iterations across all for loops (default: 10,000)
    MaxIterations: 10000,

    // Custom functions callable from WCL expressions
    Functions: map[string]func([]any) (any, error){ ... },
}

doc, err := wcl.Parse(source, opts)
```

When processing untrusted WCL input, disable imports to prevent file system access:

```go
noImports := false
doc, err := wcl.Parse(untrustedInput, &wcl.ParseOptions{
    AllowImports: &noImports,
})
```

Pass `nil` for default options:

```go
doc, err := wcl.Parse(source, nil)
```

## Error Handling

The `Document` collects all diagnostics from every pipeline phase. Each `Diagnostic` includes a severity, message, and optional error code:

```go
doc, _ := wcl.Parse(`
    server web {
        port = "not_a_number"
    }

    schema "server" {
        port: i64
    }
`, nil)
defer doc.Close()

diags, _ := doc.Diagnostics()
for _, d := range diags {
    code := ""
    if d.Code != nil {
        code = "[" + *d.Code + "] "
    }
    fmt.Printf("%s: %s%s\n", d.Severity, code, d.Message)
}
```

The `Diagnostic` type:

```go
type Diagnostic struct {
    Severity string  // "error", "warning", "info", "hint"
    Message  string
    Code     *string // e.g. "E071" for type mismatch, nil if no code
}
```

## Thread Safety

Documents are safe to use from multiple goroutines. All methods acquire a read lock internally:

```go
doc, _ := wcl.Parse("x = 42", nil)
defer doc.Close()

var wg sync.WaitGroup
for i := 0; i < 10; i++ {
    wg.Add(1)
    go func() {
        defer wg.Done()
        values, _ := doc.Values()
        fmt.Println(values["x"])
    }()
}
wg.Wait()
```

## Complete Example

Putting it all together — parse a configuration, validate it, query it, and extract values:

```go
package main

import (
    "fmt"
    "log"

    wcl "github.com/wiltaylor/wcl/bindings/go"
)

func main() {
    doc, err := wcl.Parse(`
        schema "server" {
            port: i64
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
    `, nil)
    if err != nil {
        log.Fatal(err)
    }
    defer doc.Close()

    // 1. Check for errors
    if doc.HasErrors() {
        errs, _ := doc.Errors()
        for _, e := range errs {
            log.Printf("%s: %s", e.Severity, e.Message)
        }
        log.Fatal("validation failed")
    }

    // 2. Query for all server ports
    ports, _ := doc.Query("server | .port")
    fmt.Println("All ports:", ports)

    // 3. Iterate resolved blocks
    servers, _ := doc.BlocksOfType("server")
    for _, s := range servers {
        id := "(no id)"
        if s.ID != nil {
            id = *s.ID
        }
        fmt.Printf("%s: %v:%v\n", id, s.Attributes["host"], s.Attributes["port"])
    }
}
```

## Building from Source

If you want to rebuild the WASM module from the Rust source (e.g., after modifying the WCL codebase), run:

```bash
# Using just (recommended)
just build go

# Or via go generate (requires Rust toolchain)
cd bindings/go && go generate ./...
```
