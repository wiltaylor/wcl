# wcl set

Set an attribute value by path in a WCL document.

## Usage

```bash
wcl set <file> <path> <value>
```

## Description

`wcl set` edits a WCL document by locating the attribute identified by `<path>` and replacing its value with `<value>`. The modified document is written back to the file in place. Formatting and comments in the rest of the file are preserved.

The value is parsed as a WCL expression, so string values must be quoted.

## Path Syntax

Paths use dot notation to navigate through the document structure:

| Path | Meaning |
|------|---------|
| `service#svc-api.port` | Attribute `port` inside block `service` with ID `svc-api` |
| `database#primary.host` | Attribute `host` inside block `database` with ID `primary` |
| `service#svc-api.tls.enabled` | Nested attribute access |

The `type#id` portion identifies the block. The `.attribute` portion names the attribute within that block.

## Examples

Set an integer value:

```bash
wcl set config.wcl service#svc-api.port 9090
```

Set a string value:

```bash
wcl set config.wcl service#svc-api.host '"0.0.0.0"'
```

Set a boolean value:

```bash
wcl set config.wcl service#svc-api.tls.enabled true
```

Set a list value:

```bash
wcl set config.wcl service#svc-api.tags '["prod", "api"]'
```

## Notes

- The path must refer to an existing attribute. To add a new attribute, use `wcl add` or edit the file directly.
- String values must include their quotes: use `'"value"'` in shell to pass a quoted string.
- The file is modified in place; no backup is created automatically.
