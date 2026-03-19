# wcl add

Add a new block to a WCL document.

## Usage

```bash
wcl add <file> <block_spec> [options]
```

## Options

| Flag | Description |
|------|-------------|
| `--file-auto` | Automatically determine the best placement for the new block |

## Description

`wcl add` appends a new empty block to the specified WCL document. The block is inserted and the file is written back in place. You can then use `wcl set` to populate attributes, or edit the file directly.

The `<block_spec>` argument is a quoted string describing the block type and optional ID, matching WCL block declaration syntax.

## Block Spec Syntax

| Spec | Result |
|------|--------|
| `"service svc-new"` | `service svc-new { }` |
| `"database primary"` | `database primary { }` |
| `"config"` | `config { }` (no ID) |

## Examples

Add a new service block:

```bash
wcl add config.wcl "service svc-new"
```

Add a database block with auto-placement:

```bash
wcl add config.wcl "database replica" --file-auto
```

Add a block with a label (quoted string in spec):

```bash
wcl add config.wcl 'endpoint svc-api "v2"'
```

## Notes

- By default, the new block is appended at the end of the document.
- With `--file-auto`, the CLI attempts to place the new block near existing blocks of the same type.
- The new block is empty; use `wcl set` or direct editing to populate its attributes.
- The block spec must be quoted in the shell to avoid splitting on spaces.
