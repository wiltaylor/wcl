# Using WCL from C and C++

WCL provides a C static library (`libwcl_ffi.a` / `wcl_ffi.lib`) and a header (`wcl.h`) that expose the full 11-phase WCL pipeline. All complex values cross the boundary as JSON strings — parse them with your preferred JSON library (e.g. cJSON, nlohmann/json, jansson).

Prebuilt libraries are available for:

| Platform | Architecture | Library |
|----------|-------------|---------|
| Linux | x86_64 | `lib/linux_amd64/libwcl_ffi.a` |
| Linux | aarch64 | `lib/linux_arm64/libwcl_ffi.a` |
| macOS | x86_64 | `lib/darwin_amd64/libwcl_ffi.a` |
| macOS | arm64 | `lib/darwin_arm64/libwcl_ffi.a` |
| Windows | x86_64 | `lib/windows_amd64/wcl_ffi.lib` |

## Using the Prebuilt Package

Download and extract the `wcl-ffi.tar.gz` archive. It contains everything you need:

```
wcl-ffi/
  CMakeLists.txt          # CMake config (creates wcl::wcl target)
  include/wcl.h           # C header
  lib/
    linux_amd64/libwcl_ffi.a
    linux_arm64/libwcl_ffi.a
    darwin_amd64/libwcl_ffi.a
    darwin_arm64/libwcl_ffi.a
    windows_amd64/wcl_ffi.lib
```

### CMake

Add the extracted directory as a subdirectory in your project:

```cmake
cmake_minimum_required(VERSION 3.14)
project(myapp LANGUAGES C)

add_subdirectory(path/to/wcl-ffi)

add_executable(myapp main.c)
target_link_libraries(myapp PRIVATE wcl::wcl)
```

The `wcl::wcl` target automatically handles the include path, selects the correct library for your platform, and links the required system dependencies.

Build:

```bash
cmake -B build
cmake --build build
```

### Without CMake (Linux)

```bash
gcc -o myapp main.c \
    -Ipath/to/wcl-ffi/include \
    -Lpath/to/wcl-ffi/lib/linux_amd64 \
    -lwcl_ffi -lm -ldl -lpthread
```

### Without CMake (macOS)

```bash
gcc -o myapp main.c \
    -Ipath/to/wcl-ffi/include \
    -Lpath/to/wcl-ffi/lib/darwin_arm64 \
    -lwcl_ffi -lm -ldl -lpthread -framework Security
```

### Without CMake (Windows / MSVC)

```bash
cl /Fe:myapp.exe main.c \
    /I path\to\wcl-ffi\include \
    path\to\wcl-ffi\lib\windows_amd64\wcl_ffi.lib \
    ws2_32.lib bcrypt.lib userenv.lib
```

## Building from Source

If you need to build the library yourself (requires a Rust toolchain):

```bash
# Native platform only
cargo build -p wcl_ffi --release
# Output: target/release/libwcl_ffi.a (or .lib on Windows)

# All platforms (requires cargo-zigbuild + zig)
just build ffi-all

# Package into an archive
just pack ffi
```

When building from the source tree, the CMake file also searches `target/release/` for the library, so you can use `add_subdirectory` directly on `crates/wcl_ffi/`:

```cmake
add_subdirectory(path/to/wcl/crates/wcl_ffi)
target_link_libraries(myapp PRIVATE wcl::wcl)
```

You can override the library location with `-DWCL_LIB_DIR`:

```bash
cmake -B build -DWCL_LIB_DIR=/custom/path
```

## C API Reference

All functions use null-terminated C strings. Strings returned by `wcl_ffi_*` functions are heap-allocated and **must** be freed with `wcl_ffi_string_free()`. Documents **must** be freed with `wcl_ffi_document_free()`.

```c
#include "wcl.h"
```

### Parsing

```c
// Parse a source string. options_json may be NULL for defaults.
WclDocument *wcl_ffi_parse(const char *source, const char *options_json);

// Parse a file. Returns NULL on I/O error (check wcl_ffi_last_error()).
WclDocument *wcl_ffi_parse_file(const char *path, const char *options_json);

// Free a document. Safe to call with NULL.
void wcl_ffi_document_free(WclDocument *doc);
```

### Accessing Values

```c
// Evaluated values as a JSON object string. Caller frees.
char *wcl_ffi_document_values(const WclDocument *doc);

// Check for errors.
bool wcl_ffi_document_has_errors(const WclDocument *doc);

// Error diagnostics as a JSON array string. Caller frees.
char *wcl_ffi_document_errors(const WclDocument *doc);

// All diagnostics as a JSON array string. Caller frees.
char *wcl_ffi_document_diagnostics(const WclDocument *doc);
```

### Queries and Blocks

```c
// Execute a query. Returns {"ok": ...} or {"error": "..."}. Caller frees.
char *wcl_ffi_document_query(const WclDocument *doc, const char *query);

// All blocks as a JSON array. Caller frees.
char *wcl_ffi_document_blocks(const WclDocument *doc);

// Blocks of a specific type. Caller frees.
char *wcl_ffi_document_blocks_of_type(const WclDocument *doc, const char *kind);
```

### Custom Functions

```c
// Callback signature: receives JSON args, returns JSON result (malloc'd).
// Return NULL on error, or prefix with "ERR:" for an error message.
typedef char *(*WclCallbackFn)(void *ctx, const char *args_json);

// Parse with custom functions.
WclDocument *wcl_ffi_parse_with_functions(
    const char *source,
    const char *options_json,
    const char *const *func_names,      // array of function name strings
    const WclCallbackFn *func_callbacks, // array of callback pointers
    const uintptr_t *func_contexts,      // array of context pointers
    uintptr_t func_count
);
```

### Utilities

```c
// Free a string returned by any wcl_ffi_* function. Safe with NULL.
void wcl_ffi_string_free(char *s);

// Last error message from a failed call. NULL if none. Caller frees.
char *wcl_ffi_last_error(void);

// List installed libraries. Returns JSON. Caller frees.
char *wcl_ffi_list_libraries(void);
```

## Parsing a WCL String

```c
#include <stdio.h>
#include "wcl.h"

int main(void) {
    WclDocument *doc = wcl_ffi_parse(
        "server web-prod {\n"
        "    host = \"0.0.0.0\"\n"
        "    port = 8080\n"
        "}\n",
        NULL
    );

    if (wcl_ffi_document_has_errors(doc)) {
        char *errors = wcl_ffi_document_errors(doc);
        fprintf(stderr, "Errors: %s\n", errors);
        wcl_ffi_string_free(errors);
    } else {
        char *values = wcl_ffi_document_values(doc);
        printf("Values: %s\n", values);
        wcl_ffi_string_free(values);
    }

    wcl_ffi_document_free(doc);
    return 0;
}
```

## Parsing a WCL File

```c
#include <stdio.h>
#include "wcl.h"

int main(void) {
    WclDocument *doc = wcl_ffi_parse_file("config/main.wcl", NULL);

    if (!doc) {
        char *err = wcl_ffi_last_error();
        fprintf(stderr, "Failed to open file: %s\n", err ? err : "unknown");
        wcl_ffi_string_free(err);
        return 1;
    }

    char *values = wcl_ffi_document_values(doc);
    printf("%s\n", values);
    wcl_ffi_string_free(values);
    wcl_ffi_document_free(doc);
    return 0;
}
```

## Running Queries

```c
WclDocument *doc = wcl_ffi_parse(
    "server svc-api { port = 8080 }\n"
    "server svc-admin { port = 9090 }\n",
    NULL
);

char *result = wcl_ffi_document_query(doc, "server | .port");
printf("Ports: %s\n", result);  // {"ok":[8080,9090]}
wcl_ffi_string_free(result);

wcl_ffi_document_free(doc);
```

## Working with Blocks

```c
WclDocument *doc = wcl_ffi_parse(
    "server web { port = 80 }\n"
    "database main { port = 5432 }\n",
    NULL
);

// All blocks
char *blocks = wcl_ffi_document_blocks(doc);
printf("All blocks: %s\n", blocks);
wcl_ffi_string_free(blocks);

// Blocks of a specific type
char *servers = wcl_ffi_document_blocks_of_type(doc, "server");
printf("Servers: %s\n", servers);
wcl_ffi_string_free(servers);

wcl_ffi_document_free(doc);
```

## Custom Functions

```c
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "wcl.h"

// Callback: double the first argument.
// Receives JSON array of args, returns JSON result (must be malloc'd).
char *double_fn(void *ctx, const char *args_json) {
    (void)ctx;
    // Minimal parsing: args_json is e.g. "[21]"
    int n = 0;
    sscanf(args_json, "[%d]", &n);
    char *result = malloc(32);
    snprintf(result, 32, "%d", n * 2);
    return result;
}

int main(void) {
    const char *names[] = {"double"};
    WclCallbackFn callbacks[] = {double_fn};
    uintptr_t contexts[] = {0};

    WclDocument *doc = wcl_ffi_parse_with_functions(
        "result = double(21)",
        NULL,
        names, callbacks, contexts, 1
    );

    char *values = wcl_ffi_document_values(doc);
    printf("%s\n", values);  // {"result":42}
    wcl_ffi_string_free(values);
    wcl_ffi_document_free(doc);
    return 0;
}
```

## Parse Options

Options are passed as a JSON string:

```c
WclDocument *doc = wcl_ffi_parse(source,
    "{"
    "  \"rootDir\": \"./config\","
    "  \"allowImports\": false,"
    "  \"maxImportDepth\": 32,"
    "  \"maxMacroDepth\": 64,"
    "  \"maxLoopDepth\": 32,"
    "  \"maxIterations\": 10000"
    "}"
);
```

Pass `NULL` for default options. When processing untrusted input, disable imports:

```c
WclDocument *doc = wcl_ffi_parse(untrusted, "{\"allowImports\": false}");
```

## Error Handling

The document collects all diagnostics from every pipeline phase. Each diagnostic in the JSON array has `severity`, `message`, and an optional `code`:

```c
WclDocument *doc = wcl_ffi_parse(
    "schema \"server\" { port: int }\n"
    "server web { port = \"bad\" }\n",
    NULL
);

if (wcl_ffi_document_has_errors(doc)) {
    char *diags = wcl_ffi_document_diagnostics(doc);
    // diags is a JSON array:
    // [{"severity":"error","message":"...","code":"E071"}]
    printf("Diagnostics: %s\n", diags);
    wcl_ffi_string_free(diags);
}

wcl_ffi_document_free(doc);
```

## C++ Usage

The header is plain C and works directly from C++:

```cpp
extern "C" {
#include "wcl.h"
}
```

All the examples above work identically in C++. The CMake target and compiler flags are the same — just compile your `.cpp` files instead of `.c`.

## Complete Example

```c
#include <stdio.h>
#include "wcl.h"

int main(void) {
    WclDocument *doc = wcl_ffi_parse(
        "schema \"server\" {\n"
        "    port: int\n"
        "    host: string @optional\n"
        "}\n"
        "\n"
        "server svc-api {\n"
        "    port = 8080\n"
        "    host = \"api.internal\"\n"
        "}\n"
        "\n"
        "server svc-admin {\n"
        "    port = 9090\n"
        "    host = \"admin.internal\"\n"
        "}\n",
        NULL
    );

    // 1. Check for errors
    if (wcl_ffi_document_has_errors(doc)) {
        char *errors = wcl_ffi_document_errors(doc);
        fprintf(stderr, "Errors: %s\n", errors);
        wcl_ffi_string_free(errors);
        wcl_ffi_document_free(doc);
        return 1;
    }

    // 2. Get evaluated values
    char *values = wcl_ffi_document_values(doc);
    printf("Values: %s\n", values);
    wcl_ffi_string_free(values);

    // 3. Query for all server ports
    char *ports = wcl_ffi_document_query(doc, "server | .port");
    printf("Ports: %s\n", ports);
    wcl_ffi_string_free(ports);

    // 4. Get server blocks
    char *servers = wcl_ffi_document_blocks_of_type(doc, "server");
    printf("Servers: %s\n", servers);
    wcl_ffi_string_free(servers);

    // 5. All diagnostics
    char *diags = wcl_ffi_document_diagnostics(doc);
    printf("Diagnostics: %s\n", diags);
    wcl_ffi_string_free(diags);

    wcl_ffi_document_free(doc);
    return 0;
}
```
