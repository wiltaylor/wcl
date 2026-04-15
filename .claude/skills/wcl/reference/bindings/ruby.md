# Ruby Binding

WASM-based, using [wasmtime-rb](https://github.com/bytecodealliance/wasmtime-rb). Source: `bindings/ruby/lib/wcl.rb`, `lib/wcl/document.rb`.

## Install

```bash
gem install wcl
```

Or in a `Gemfile`:
```ruby
gem "wcl"
```

Module: `Wcl`.

## Minimal Example

```ruby
require "wcl"

doc = Wcl.parse(<<~WCL)
  server web {
    host = "0.0.0.0"
    port = 8080
  }
WCL

if doc.has_errors?
  doc.errors.each { |d| puts "#{d.code}: #{d.message}" }
else
  pp doc.values
end

doc.close
```

File:

```ruby
doc = Wcl.parse_file("./config.wcl")   # sets root_dir to the file's parent
```

## Core API

From `wcl.rb`:

| Method | Purpose |
|--------|---------|
| `Wcl.parse(source, **kwargs)` | Parse + evaluate |
| `Wcl.parse_file(path, **kwargs)` | Read file, default `root_dir` |

`Wcl::Document` (`document.rb:5`):

| Method | Description |
|--------|-------------|
| `values` | Evaluated top-level (memoized) |
| `has_errors?` | boolean |
| `errors` | `Array<Diagnostic>` (severity == error) |
| `diagnostics` | All |
| `query(str)` | Run a query; raises `Wcl::ValueError` on error |
| `blocks` | Top-level blocks |
| `blocks_of_type(kind)` | Filtered |
| `close` | Release WASM handle (also a finalizer); idempotent |
| `to_h` | `{ values:, has_errors:, diagnostics: }` |

## Parse Keyword Arguments

```ruby
Wcl.parse(src,
  root_dir:          "/path",
  allow_imports:     true,
  max_import_depth:  32,
  max_macro_depth:   64,
  max_loop_depth:    8,
  max_iterations:    10_000,
  functions:         { "upper_rev" => ->(args) { args[0].upcase.reverse } },
  variables:         { "PORT" => 8080 }
)
```

## Value Type Mapping

| WCL | Ruby |
|-----|------|
| string | `String` |
| int | `Integer` |
| float | `Float` |
| bool | `true` / `false` |
| null | `nil` |
| list | `Array` |
| map | `Hash` |
| block | `BlockRef` (from `blocks`) / `Hash` (from `values`) |
| date / duration | `String` (ISO) |
| symbol | `String` prefixed `:` |

## Error Handling

- `Wcl.parse` never raises for WCL errors — inspect `doc.has_errors?` and `doc.errors`.
- `Wcl.parse_file` raises `IOError` on missing/unreadable files.
- `doc.query` raises `Wcl::ValueError` on query/eval failure.
- `doc.values`, `doc.blocks`, etc. raise `RuntimeError("Document is closed")` after `close`.

## Gotchas

- Integer precision: the WASM boundary uses JSON — values > 2^53 may be truncated to `Float`.
- Custom functions live only for the `parse` call; their names are collected into a JSON list and registered globally on the WASM instance, then cleared in `ensure`.
- Finalizer (via `ObjectSpace.define_finalizer`) will eventually free the handle, but call `close` explicitly to release resources promptly.
