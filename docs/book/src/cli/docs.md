# wcl docs

Generate schema documentation as an mdBook.

## Usage

```bash
wcl docs <files...> [options]
```

## Options

| Flag | Description |
|------|-------------|
| `--output <dir>` | Output directory (default: `docs-out`) |
| `--title <title>` | Book title (default: `WCL Schema Reference`) |
| `--lib-path <dir>` | Extra library search path (may be repeated) |
| `--no-default-lib-paths` | Disable default library search paths |

## Description

`wcl docs` reads one or more WCL files, extracts all schema definitions, and generates a browsable mdBook with:

- A page for each schema showing its fields, types, decorators, and constraints
- A summary page listing all schemas
- Cross-references between schemas that reference each other

## Examples

Generate docs from a single file:

```bash
wcl docs schemas.wcl --output docs-out
```

Generate from multiple files with a custom title:

```bash
wcl docs schemas/*.wcl --title "My Project Schemas" --output schema-docs
```

Then serve the book:

```bash
cd docs-out && mdbook serve
```
