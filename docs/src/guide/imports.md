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

## Path Rules

- **Relative paths only.** Absolute paths and URLs are not accepted.
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

`import_table("path.csv")` loads a CSV file as a table value. An optional second argument overrides the separator:

```wcl
let acl_rows = import_table("./acl.csv")
let tsv_rows = import_table("./data.tsv", "\t")
```

The first CSV row is used as column headers. See the [Data Tables](./tables.md) chapter for details on working with table values.

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
