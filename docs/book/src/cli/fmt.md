# wcl fmt

Format a WCL document according to the standard style.

## Usage

```bash
wcl fmt <file> [options]
```

## Options

| Flag | Description |
|------|-------------|
| `--write` | Format the file in place instead of printing to stdout |
| `--check` | Check whether the file is already formatted; exit non-zero if not |

## Description

`wcl fmt` applies canonical formatting to a WCL document. By default, it prints the formatted output to stdout, leaving the source file unchanged.

The formatter preserves:

- All comments (line, block, and doc comments)
- Blank line grouping within blocks
- The logical structure and ordering of the document

The formatter normalises:

- Indentation (2 spaces per level)
- Spacing around operators and delimiters
- Trailing commas in lists and maps
- Consistent quote style for string literals

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success (or file is already formatted when using `--check`) |
| `1` | File would be reformatted (only with `--check`) |
| `2` | Argument or parse error |

## Examples

Print formatted output to stdout:

```bash
wcl fmt config.wcl
```

Format file in place:

```bash
wcl fmt --write config.wcl
```

Check formatting in CI (no changes written):

```bash
wcl fmt --check config.wcl
```

Check all WCL files in a project:

```bash
find . -name '*.wcl' | xargs wcl fmt --check
```
