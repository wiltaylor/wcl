# C / C++ Binding (`wcl_ffi`)

Native C API, no WASM. Emits `libwcl` static and shared libraries plus `wcl.h`. Source: `crates/wcl_ffi/wcl.h`, `crates/wcl_ffi/src/lib.rs`.

## Build

From the repo root:

```bash
cargo build --release -p wcl_ffi
# Produces:
#   target/release/libwcl.a    (static)
#   target/release/libwcl.so   (dynamic, Linux)
#   target/release/libwcl.dylib (macOS)
# Header:
#   crates/wcl_ffi/wcl.h
```

There is also a `CMakeLists.txt` (`crates/wcl_ffi/CMakeLists.txt`) for CMake integration.

Link flags (typical):
```
-L<repo>/target/release -lwcl -lpthread -ldl -lm
```

## Minimal Example

```c
#include <stdio.h>
#include <stdlib.h>
#include "wcl.h"

int main(void) {
    const char *src =
        "server web {\n"
        "  host = \"0.0.0.0\"\n"
        "  port = 8080\n"
        "}\n";

    WclDocument *doc = wcl_ffi_parse(src, NULL);
    if (!doc) {
        char *err = wcl_ffi_last_error();
        fprintf(stderr, "parse failed: %s\n", err ? err : "(unknown)");
        wcl_ffi_string_free(err);
        return 1;
    }

    if (wcl_ffi_document_has_errors(doc)) {
        char *diags = wcl_ffi_document_errors(doc);
        fprintf(stderr, "%s\n", diags);
        wcl_ffi_string_free(diags);
    } else {
        char *values = wcl_ffi_document_values(doc);
        printf("%s\n", values);      // JSON
        wcl_ffi_string_free(values);
    }

    wcl_ffi_document_free(doc);
    return 0;
}
```

File:

```c
WclDocument *doc = wcl_ffi_parse_file("./config.wcl", NULL);
// root_dir is automatically set to the file's parent directory
```

## Core API (from `wcl.h`)

All strings returned by `wcl_ffi_*` functions must be freed with `wcl_ffi_string_free`. All documents must be freed with `wcl_ffi_document_free`.

| Function | Returns | Purpose |
|----------|---------|---------|
| `wcl_ffi_parse(source, options_json)` | `WclDocument*` | Parse + evaluate. `options_json` is a JSON string or `NULL` |
| `wcl_ffi_parse_file(path, options_json)` | `WclDocument*` | Reads file; `NULL` on I/O failure (see `wcl_ffi_last_error`) |
| `wcl_ffi_parse_with_functions(source, options_json, names, callbacks, contexts, count)` | `WclDocument*` | Parse with C callback functions |
| `wcl_ffi_last_error()` | `char*` | Last error message (or `NULL`) |
| `wcl_ffi_document_values(doc)` | `char*` | JSON string |
| `wcl_ffi_document_has_errors(doc)` | `bool` | |
| `wcl_ffi_document_errors(doc)` | `char*` | JSON array of error diagnostics |
| `wcl_ffi_document_diagnostics(doc)` | `char*` | JSON array of all diagnostics |
| `wcl_ffi_document_query(doc, query)` | `char*` | JSON `{"ok": value}` or `{"error": "msg"}` |
| `wcl_ffi_document_blocks(doc)` | `char*` | JSON array of top-level blocks |
| `wcl_ffi_document_blocks_of_type(doc, kind)` | `char*` | JSON array filtered by kind |
| `wcl_ffi_list_libraries()` | `char*` | JSON `{"ok": [paths]}` or `{"error": "..."}` |
| `wcl_ffi_call_function(doc, name, args_json)` | `char*` | Call an exported WCL function; returns JSON or `ERR:message` |
| `wcl_ffi_list_functions(doc)` | `char*` | JSON array of `{name, params}` |
| `wcl_ffi_document_free(doc)` | void | Safe with NULL |
| `wcl_ffi_string_free(s)` | void | Safe with NULL |

## Custom Functions

```c
typedef char *(*WclCallbackFn)(void *ctx, const char *args_json);

static char *my_upper(void *ctx, const char *args_json) {
    // Parse args_json, do the work, return a heap-allocated JSON string.
    // Caller (the FFI) frees the returned string.
    return strdup("\"HI\"");
}

const char *names[] = { "my_upper" };
WclCallbackFn cbs[] = { my_upper };
uintptr_t ctxs[] = { 0 };

WclDocument *doc = wcl_ffi_parse_with_functions(
    "x = my_upper(\"hi\")",
    NULL,
    names, cbs, ctxs, 1
);
```

The callback must return a heap-allocated `char*` containing either:
- a JSON-encoded result value, or
- a string starting with `ERR:` followed by an error message.

## Options JSON

```json
{
  "rootDir":           "/path",
  "allowImports":      true,
  "maxImportDepth":    32,
  "maxMacroDepth":     64,
  "maxLoopDepth":      8,
  "maxIterations":     10000,
  "variables":         { "PORT": 8080 },
  "libPaths":          ["/custom/lib"],
  "noDefaultLibPaths": false
}
```

## Value Representation

All rich values cross the boundary as JSON. The standard keys on a block:
```json
{ "kind": "server", "id": "web", "attributes": {...}, "children": [...], "decorators": [...] }
```

## Error Handling

- A `NULL` return from `wcl_ffi_parse` / `wcl_ffi_parse_file` means host-side failure; call `wcl_ffi_last_error` immediately.
- `wcl_ffi_document_has_errors` tells you if the document has WCL errors (even when parse returned a valid pointer).
- `wcl_ffi_document_query` wraps its result in `{"ok": ...}` or `{"error": ...}` — always inspect both keys.

## Gotchas

- **Always `wcl_ffi_string_free` every returned `char*`** — otherwise you leak.
- `wcl_ffi_document_free(NULL)` is safe, but calling it twice on the same pointer is undefined behavior.
- The FFI is thread-safe for independent documents; within one document, external synchronization is required if you call methods concurrently.
- C++ users: wrap handles in RAII types; the header is `extern "C"`-safe.
