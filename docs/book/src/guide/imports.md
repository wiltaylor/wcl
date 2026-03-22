# Imports

Imports let you split a WCL configuration across multiple files and compose them at load time.

## Basic Syntax

```wcl
import "./relative/path.wcl"
```

`import` is a top-level statement only — it cannot appear inside a block or expression. The path must be a string literal.

## What Gets Merged

When a file is imported, the following items from the imported file are merged into the importing scope:

- Blocks (services, configs, etc.)
- Attributes defined at the top level
- Schemas and decorator schemas
- Macro definitions
- Tables
- Validation blocks
- Exported variables (`let` bindings declared with `export`)

**Private bindings are not merged.** A plain `let` binding is file-private and is never visible to the importing file. Use `export let` to make a variable available across the import boundary:

```wcl
// shared.wcl
export let environment = "production"
export let base_port   = 9000
```

```wcl
// main.wcl
import "./shared.wcl"

service "api" {
    env  = environment    // "production"
    port = base_port + 1  // 9001
}
```

## Library Imports

In addition to relative path imports, WCL supports well-known library imports using angle-bracket syntax:

```wcl
import <myapp.wcl>
```

Library files are searched in these directories (in order):

1. **User library**: `$XDG_DATA_HOME/wcl/lib/` (default: `~/.local/share/wcl/lib/`)
2. **System library**: each dir in `$XDG_DATA_DIRS` + `/wcl/lib/` (default: `/usr/local/share/wcl/lib/`, `/usr/share/wcl/lib/`)

Library imports **skip the jail check** since they are intentionally located outside the project root. All other rules (import-once, depth limit, recursive resolution) still apply.

Library files can contain schemas, `declare` stubs for host-registered functions, and any other WCL content:

```wcl
// ~/.local/share/wcl/lib/myapp.wcl
schema "server_config" {
    port: int
    host: string @optional
}

declare my_custom_fn(input: string) -> string
```

See the [Libraries guide](../guide/libraries.md) for how to create and manage library files.

## Path Rules

- **Relative paths only** (for quoted imports). Absolute paths and URLs are not accepted.
- Paths are resolved relative to the file containing the `import` statement.
- All resolved paths must remain inside the project root directory. Attempts to escape via `../../../` are rejected.
- Symlinks that point outside the root are not followed.
- Remote imports (HTTP/HTTPS) are not supported.

## Import-Once Semantics

WCL deduplicates imports by canonical path. If two files both import a third file, that third file is processed exactly once and its contents are merged a single time. Import graphs are therefore safe regardless of how many paths lead to the same file.

## Circular Imports

Circular import chains are detected and produce a clear error rather than looping infinitely. Restructure your files to break the cycle, typically by extracting shared definitions into a dedicated file that neither file in the cycle imports.

## Depth Limit

The default maximum import depth is **32**. This prevents runaway chains in generated or adversarial inputs. The limit can be raised programmatically when constructing the pipeline options, but the default is appropriate for all normal configurations.

## Merge Conflicts

Different item kinds have different conflict rules:

| Item | Conflict behavior |
|---|---|
| Blocks with distinct IDs | Merged without conflict |
| Blocks sharing an ID | Requires `partial` — see [Partial Declarations](./partials.md) |
| Duplicate schema name | Error (E001) |
| Duplicate decorator schema name | Error |
| Duplicate top-level attribute | Error |
| Macros with the same name | Error |

If you need to compose fragments of the same block across files, declare every fragment as `partial` and let the merge phase assemble them.

## Importing Non-WCL Files

### Raw Text: `import_raw`

`import_raw("path")` reads an arbitrary file and returns its contents as a string value. This is useful for embedding certificates, SQL, or other text assets:

```wcl
service "tls-frontend" {
    cert = import_raw("./certs/server.pem")
    key  = import_raw("./certs/server.key")
}
```

The same path rules apply: relative only, jailed to the root.

### CSV Data: `import_table`

`import_table("path.csv")` loads a CSV file as a table value:

```wcl
let acl_rows = import_table("./acl.csv")
let tsv_rows = import_table("./data.tsv", "\t")
```

Named arguments provide fine-grained control:

```wcl
# Custom separator
let tsv = import_table("./data.tsv", separator="\t")

# Skip the header row
let raw = import_table("./data.csv", headers=false)

# Explicit column names
let data = import_table("./data.csv", headers=false, columns=["name", "age"])
```

| Parameter | Type | Default | Description |
|---|---|---|---|
| `separator` | string | `","` | Field separator character |
| `headers` | bool | `true` | Whether the first row contains column headers |
| `columns` | list | — | Explicit column names (overrides headers) |

Tables can be populated directly from CSV using assignment syntax:

```wcl
table users : user_row = import_table("data.csv")
```

See the [Data Tables](./tables.md) chapter for full details.

## Security: `allow_imports`

When processing untrusted WCL input you can disable all file-loading operations by setting the `allow_imports` pipeline option to `false`. With this option disabled, any `import`, `import_raw`, or `import_table` statement produces an error rather than reading from disk. This is recommended for any context where the WCL source is not fully trusted.

```rust
let opts = PipelineOptions {
    allow_imports: false,
    ..Default::default()
};
let doc = Document::from_str_with_options(src, opts)?;
```

## Example: Multi-File Layout

```
config/
  main.wcl
  shared/
    constants.wcl
    schemas.wcl
  services/
    auth.wcl
    gateway.wcl
```

```wcl
// main.wcl
import "./shared/constants.wcl"
import "./shared/schemas.wcl"
import "./services/auth.wcl"
import "./services/gateway.wcl"
```

```wcl
// shared/constants.wcl
export let region      = "us-east-1"
export let log_level   = "info"
export let base_domain = "example.internal"
```

```wcl
// services/auth.wcl
import "../shared/constants.wcl"

service svc-auth "auth-service" {
    region = region
    domain = "auth." + base_domain
    log    = log_level
}
```

Because `constants.wcl` is imported by both `main.wcl` and `auth.wcl`, it is evaluated once. The `export let` bindings are available wherever the file is imported.

## Glob Imports

A glob pattern can be used in place of a literal path to import multiple files at once:

```wcl
import "./schemas/*.wcl"
import "./modules/**/*.wcl"
```

- `*` matches any file name within a single directory.
- `**` matches any number of directory segments (recursive).
- Matched files are processed in **alphabetical order** by resolved path, ensuring deterministic merge order regardless of filesystem traversal order.
- If the pattern matches no files, error **E016** is reported. Use an optional import (see below) to suppress this.

Each matched file is subject to the same path rules as a regular import: relative paths only, jailed to the project root, import-once deduplication, and depth limit enforcement.

```wcl
// Import every schema defined under the schemas/ directory
import "./schemas/**/*.wcl"

// Import all service definitions from a flat directory
import "./services/*.wcl"
```

## Optional Imports

Prefix the `import` keyword with `?` to make the import silently succeed when the target does not exist:

```wcl
import? "./local-overrides.wcl"
```

If `local-overrides.wcl` is absent the statement is a no-op. If the file exists, it is imported normally.

Optional imports compose with glob patterns:

```wcl
import? "./env/*.wcl"
```

When glob and optional are combined, a pattern that matches no files is not an error — the statement is silently skipped. This is useful for environment-specific overlay directories that may not exist in every deployment.

**Security errors are always reported.** A path that would escape the project root, exceed the depth limit, or violate another security constraint produces an error even when `?` is present. Optional only suppresses "file not found" and "no glob matches"; it does not suppress policy violations.
