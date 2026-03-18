# CLI Quickstart

The `wcl` binary is the primary interface to the WCL toolchain. This page gives a quick tour of the most commonly used commands.

## Evaluate to JSON

```bash
wcl eval config.wcl
```

Runs the full WCL pipeline (parse, macro expansion, evaluation, schema validation) and prints the result as JSON to stdout. Use `--pretty` for indented output (the default) or `--compact` for a single line.

## Validate

```bash
wcl validate config.wcl
```

Runs the pipeline including schema validation and reports any diagnostics. Exits with a non-zero status code if there are errors. Useful as a pre-commit check or in CI.

## Format

```bash
wcl fmt config.wcl
```

Prints the formatted version of the file to stdout. To write the result back in place:

```bash
wcl fmt config.wcl --write
```

To check whether a file is already formatted (useful in CI):

```bash
wcl fmt config.wcl --check
```

## Query

```bash
wcl query config.wcl "server | .port > 8000"
```

Runs a query against the evaluated document and prints matching blocks as JSON. The query syntax selects blocks by type, filters by attribute values, and supports chaining. Some examples:

```bash
# All server blocks
wcl query config.wcl "server"

# Server blocks where port is greater than 8000
wcl query config.wcl "server | .port > 8000"

# A specific block by ID
wcl query config.wcl "server#web-prod"
```

## Convert

Convert between WCL and other formats:

```bash
# JSON to WCL
wcl convert data.json --to wcl

# WCL to YAML
wcl convert config.wcl --to yaml

# WCL to JSON
wcl convert config.wcl --to json
```

Supported target formats: `wcl`, `json`, `yaml`, `toml`.

## Set a Value

Update a single attribute value in a WCL file:

```bash
wcl set config.wcl "server#web.port" "9090"
```

The path syntax is `type#id.attribute`. The file is updated in place.

## Add a Block

```bash
wcl add config.wcl "server new-svc"
```

Appends a new empty block of the given type and ID to the file. You can also pipe in a block body:

```bash
wcl add config.wcl "server new-svc" --body '{ port = 8081 }'
```

## Remove a Block

```bash
wcl remove config.wcl "server#old-svc"
```

Removes the block with type `server` and ID `old-svc` from the file.

## Inspect

Inspect internal representations for debugging or tooling development:

```bash
# Print the AST as pretty-printed text
wcl inspect --ast config.wcl

# Print the evaluated scope as JSON
wcl inspect --scope config.wcl

# Print all macros collected from the file
wcl inspect --macros config.wcl
```

## Start the Language Server

```bash
wcl lsp
```

Starts the WCL Language Server over stdio. This is normally invoked automatically by your editor extension or LSP client, not manually. See [Editor Setup](./editor-setup.md) for configuration instructions.
