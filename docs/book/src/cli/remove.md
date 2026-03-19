# wcl remove

Remove a block or attribute from a WCL document.

## Usage

```bash
wcl remove <file> <path>
```

## Description

`wcl remove` deletes a block or attribute identified by `<path>` from the given document. The file is modified in place. The surrounding content, including comments and blank lines adjacent to the removed item, is cleaned up.

## Path Syntax

The path syntax determines whether a block or an attribute is removed.

| Path | What is removed |
|------|-----------------|
| `service#svc-old` | The entire `service` block with ID `svc-old` |
| `service#svc-api.debug` | The `debug` attribute inside `service#svc-api` |
| `database#primary.port` | The `port` attribute inside `database#primary` |

A path with no trailing `.attribute` removes the whole block. A path with a trailing `.attribute` removes only that attribute from the block.

## Examples

Remove an entire block:

```bash
wcl remove config.wcl service#svc-old
```

Remove a single attribute from a block:

```bash
wcl remove config.wcl service#svc-api.debug
```

Remove a nested attribute:

```bash
wcl remove config.wcl database#primary.tls.cert_path
```

## Notes

- Removing a block removes all of its contents, including nested blocks and attributes.
- If the path does not exist in the document, an error is reported and the file is not modified.
- The file is modified in place; no backup is created automatically.
- To remove a partial block, all partial declarations sharing the same type and ID must be addressed individually.
