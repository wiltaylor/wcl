# wcl inspect

Inspect the internal representation of a WCL document.

## Usage

```bash
wcl inspect <file> [options]
```

## Options

| Flag | Description |
|------|-------------|
| `--ast` | Print the raw Abstract Syntax Tree |
| `--hir` | Print the resolved High-level Intermediate Representation |
| `--scopes` | Print the scope tree |
| `--deps` | Print the dependency graph |

## Description

`wcl inspect` exposes the internal structure that WCL builds when processing a document. It is primarily useful for debugging WCL documents, understanding how the pipeline transforms source, and for tooling authors.

Multiple flags may be combined. If no flag is given, `--hir` is used by default.

### `--ast`

Prints the raw parse tree produced by the parser, before any macro expansion, import resolution, or evaluation. Spans and token positions are included.

### `--hir`

Prints the fully resolved document after all pipeline phases complete. This corresponds to the same data that `wcl eval` serializes, but in WCL's internal tree format rather than a target serialization format.

### `--scopes`

Prints the scope tree showing each scope, the names it defines, and their resolved values. Useful for understanding name resolution and spotting shadowing.

### `--deps`

Prints the dependency graph between attributes and let bindings. Shows which names each expression depends on, and the topological evaluation order.

## Examples

Inspect the AST of a file:

```bash
wcl inspect config.wcl --ast
```

Inspect the resolved HIR:

```bash
wcl inspect config.wcl --hir
```

View the scope tree:

```bash
wcl inspect config.wcl --scopes
```

View the dependency graph:

```bash
wcl inspect config.wcl --deps
```

Combine multiple views:

```bash
wcl inspect config.wcl --scopes --deps
```
