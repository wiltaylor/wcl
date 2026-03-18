# CLI Overview

The `wcl` CLI provides tools for working with WCL documents — parsing, validating, evaluating, formatting, querying, and converting configuration files.

## Installation

```bash
cargo install wcl
```

## Usage

```bash
wcl <subcommand> [options] [args]
```

## Subcommands

| Subcommand | Description |
|------------|-------------|
| `validate` | Parse and validate a WCL document through all pipeline phases |
| `eval`     | Evaluate a document and print the resolved output |
| `fmt`      | Format a WCL document |
| `query`    | Execute a query expression against a document |
| `inspect`  | Inspect the AST, HIR, scopes, or dependency graph |
| `convert`  | Convert between WCL and other formats (JSON, YAML, TOML) |
| `set`      | Set an attribute value by path |
| `add`      | Add a new block to a document |
| `remove`   | Remove a block or attribute by path |
| `lsp`      | Start the WCL language server |

## Help

```bash
wcl --help
wcl <subcommand> --help
```

## Global Flags

| Flag | Description |
|------|-------------|
| `--help`, `-h` | Print help information |
| `--version`, `-V` | Print version information |

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | Validation or evaluation error |
| `2` | Usage / argument error |
