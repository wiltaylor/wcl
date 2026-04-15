# Bindings Overview

WCL ships bindings for nine host languages. All WASM-based bindings (all except Rust and C) embed the same `wcl_wasm` module, so they share option names (camelCase), diagnostic shape, and query semantics.

Workspace version: `0.0.0-local` (pre-release). Published names and entry modules:

| Host | Install | Package / module | Source |
|------|---------|------------------|--------|
| Rust | `cargo add wcl_lang` | `wcl_lang` crate | `crates/wcl_lang/src/lib.rs` |
| Python | `pip install pywcl` | `pywcl` (import as `wcl`) | `bindings/python/python/wcl/__init__.py` |
| JavaScript / TypeScript | `npm install wcl_wasm` | `wcl_wasm` | `bindings/wasm/pkg/wcl_wasm.d.ts` |
| Go | `go get github.com/wiltaylor/wcl/bindings/go` | `github.com/wiltaylor/wcl/bindings/go` | `bindings/go/wcl.go` |
| .NET / C# | `dotnet add package WclLang` (publishes as `Wcl`) | `Wcl` namespace | `bindings/dotnet/src/Wcl/Wcl.cs` |
| JVM / Java / Kotlin | Maven `io.github.wiltaylor:wcl` | `io.github.wiltaylor.wcl` | `bindings/jvm/src/main/java/io/github/wiltaylor/wcl/Wcl.java` |
| Ruby | `gem install wcl` | `Wcl` module | `bindings/ruby/lib/wcl.rb` |
| C / C++ | Build `crates/wcl_ffi` → `libwcl`, include `wcl.h` | `wcl_ffi` C API | `crates/wcl_ffi/wcl.h` |
| Zig | `build.zig.zon` dep + `wcl.h` | `wcl.zig` module | `bindings/zig/src/wcl.zig` |

## Shared Shape (all WASM bindings)

Every WASM binding exposes the same three operations with idiomatic naming:

1. `parse(source, options?)` → `Document`
2. `parse_file(path, options?)` → `Document` (host-side file read; sets `rootDir` from the file's parent)
3. `Document` methods: `values`, `has_errors`, `errors`, `diagnostics`, `query(str)`, `blocks()`, `blocks_of_type(kind)`, close/free.

## Common ParseOptions (WASM bindings)

Options go as JSON across the WASM boundary with these keys:

| Key | Type | Purpose |
|-----|------|---------|
| `rootDir` | string | Directory for resolving imports |
| `allowImports` | bool | Disable `import` entirely |
| `maxImportDepth` | u32 | Default 32 |
| `maxMacroDepth` | u32 | Default 64 |
| `maxLoopDepth` | u32 | Nested for-loop cap |
| `maxIterations` | u32 | Per-loop iteration cap |
| `variables` | object | Injected `$VAR` values (override `let`) |
| `libPaths` | string[] | Extra library search dirs |
| `noDefaultLibPaths` | bool | Skip XDG/system paths |

Rust exposes these as fields on `wcl_lang::ParseOptions` directly. C/Zig pass them as a JSON string. .NET and JVM wrap them in a `ParseOptions` builder.

## Value Type Mapping

| WCL | Rust (`Value`) | Python | JS/TS | Go | C# (`WclValue`) | Java (`WclValue`) | Ruby |
|-----|---------------|--------|-------|----|-----------------|-------------------|------|
| string | `Value::String` | `str` | `string` | `string` | string | `String` | `String` |
| int | `Value::Int` | `int` | `number` (or `bigint`) | `int64` / `float64` | `long` | `long` | `Integer` |
| float | `Value::Float` | `float` | `number` | `float64` | `double` | `double` | `Float` |
| bool | `Value::Bool` | `bool` | `boolean` | `bool` | `bool` | `boolean` | `true`/`false` |
| null | `Value::Null` | `None` | `null` | `nil` | `null` | `null` | `nil` |
| list | `Value::List` | `list` | `Array` | `[]any` | `List<WclValue>` | `List<WclValue>` | `Array` |
| map | `Value::Map` | `IndexMap` | `object` | `map[string]any` | `Dictionary<string,WclValue>` | `Map<String,WclValue>` | `Hash` |
| block | `Value::Block` / `BlockRef` | `BlockRef` | plain object | `BlockRef` struct | `WclValue.Block` | `WclValue.Block` | `BlockRef` |
| date | `Value::Date` | ISO string | ISO string | string | `DateTime`-ish | `LocalDate` | `Date` (string) |
| duration | `Value::Duration` | ISO string | ISO string | string | `TimeSpan`-ish | string | string |
| symbol | `Value::Symbol` | string prefixed `:` | string | string | string | string | string |

Note: WASM bindings serialise values as JSON, so large integers are lossy in JavaScript (use `bigint`/strings for values > 2^53). Rust keeps `i64`/`f64` faithfully.

## Choosing a Binding

- **Need full fidelity (no JSON round-trip)?** → Rust or C.
- **Serverless / browser?** → JavaScript (`wcl_wasm`).
- **Python data pipelines?** → Python (`pywcl`).
- **Embedded in a JVM/CLR app?** → JVM or .NET.
- **Small static binary?** → Go or Zig (both bundle WASM in-process; Go uses wazero, Zig links `libwcl` directly).

If the host isn't obvious, check the project for `Cargo.toml` (Rust), `package.json` (JS), `pyproject.toml` (Python), `go.mod` (Go), `*.csproj` (.NET), `pom.xml`/`build.gradle` (JVM), `*.gemspec` (Ruby), `build.zig.zon` (Zig), or `wcl.h` / CMakeLists (C).
