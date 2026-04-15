# Go Binding

Embeds the WCL WASM module and runs it with [wazero](https://github.com/tetratelabs/wazero) — no CGO. Source: `bindings/go/wcl.go`, `bindings/go/types.go`.

## Install

```bash
go get github.com/wiltaylor/wcl/bindings/go
```

Requires Go ≥ 1.25.0. Transitive dep: `github.com/tetratelabs/wazero`.

```go
import wcl "github.com/wiltaylor/wcl/bindings/go"
```

## Minimal Example

```go
package main

import (
    "fmt"
    "log"

    wcl "github.com/wiltaylor/wcl/bindings/go"
)

func main() {
    doc, err := wcl.Parse(`
        server web {
            host = "0.0.0.0"
            port = 8080
        }
    `, nil)
    if err != nil {
        log.Fatal(err)
    }
    defer doc.Close()

    if doc.HasErrors() {
        diags, _ := doc.Errors()
        for _, d := range diags {
            fmt.Printf("%s: %s\n", derefCode(d.Code), d.Message)
        }
        return
    }

    values, _ := doc.Values()
    fmt.Println(values)
}
```

File:

```go
doc, err := wcl.ParseFile("./config.wcl", nil) // sets RootDir to the file's parent
```

## Core API

From `wcl.go` and `types.go`:

| Symbol | Purpose |
|--------|---------|
| `Parse(source string, opts *ParseOptions) (*Document, error)` | Parse + evaluate |
| `ParseFile(path string, opts *ParseOptions) (*Document, error)` | Reads file in Go, sets `RootDir` |
| `Document.Values() (map[string]any, error)` | Evaluated top-level values |
| `Document.HasErrors() bool` | |
| `Document.Errors() ([]Diagnostic, error)` | Only severity == "error" |
| `Document.Diagnostics() ([]Diagnostic, error)` | All |
| `Document.Query(q string) (any, error)` | Run a WCL query |
| `Document.Blocks() ([]BlockRef, error)` | Top-level blocks |
| `Document.BlocksOfType(kind string) ([]BlockRef, error)` | Filtered |
| `Document.Close()` | Frees WASM resources (also a finalizer); idempotent |

`ParseOptions` (types.go:14):

```go
type ParseOptions struct {
    RootDir        string
    AllowImports   *bool
    MaxImportDepth uint32
    MaxMacroDepth  uint32
    MaxLoopDepth   uint32
    MaxIterations  uint32
    Functions      map[string]func(args []any) (any, error)
    Variables      map[string]any
}
```

`BlockRef`:

```go
type BlockRef struct {
    Kind       string         `json:"kind"`
    ID         *string        `json:"id,omitempty"`
    Attributes map[string]any `json:"attributes,omitempty"`
    Children   []BlockRef     `json:"children,omitempty"`
    Decorators []Decorator    `json:"decorators,omitempty"`
}
```

`Diagnostic`: `Severity`, `Message`, `Code *string`.

## Custom Functions

```go
doc, err := wcl.Parse(`x = upper_rev("hi")`, &wcl.ParseOptions{
    Functions: map[string]func(args []any) (any, error){
        "upper_rev": func(args []any) (any, error) {
            s := args[0].(string)
            // ... upper + reverse
            return strings.ToUpper(s), nil
        },
    },
})
```

## Value Type Mapping

| WCL | Go |
|-----|----|
| string | `string` |
| int | `int64` (or `float64` when round-tripped through JSON) |
| float | `float64` |
| bool | `bool` |
| null | `nil` |
| list | `[]any` |
| map | `map[string]any` |
| block | `BlockRef` (from `Blocks()`) / `map[string]any` (from `Values()`) |
| date / duration | string |
| symbol | string prefixed `:` |

## Error Handling

- `Parse` / `ParseFile` return `error` only for host-side failures (file I/O, runtime init).
- **WCL parse/eval errors are always on the returned `Document`** — check `HasErrors()` and iterate `Errors()`.
- `Document.Query` returns `error` when the query text is invalid or evaluation fails.
- `Document` has a finalizer, but always `defer doc.Close()` — the finalizer only runs on GC and may leak file handles transiently.

## Gotchas

- Close the `Document` promptly; the finalizer is a safety net, not a guarantee.
- All integer values from `Values()` that survive the JSON round-trip come back as `float64`; use `BlocksOfType` / `Errors` where types are preserved, or cast carefully.
- Custom functions are installed per-call and cleared after — do not keep a reference to them across calls.
- The WASM runtime is a package-level singleton; concurrent `Parse` calls are safe (document handles use an internal mutex).
