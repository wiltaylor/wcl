# wcl validate

Parse and validate a WCL document through all pipeline phases.

## Usage

```bash
wcl validate <file> [options]
```

## Options

| Flag | Description |
|------|-------------|
| `--strict` | Treat warnings as errors |
| `--schema <file>` | Load an additional external schema file |
| `--var KEY=VALUE` | Set an external variable (may be repeated) |

## Description

`wcl validate` runs the document through the full 11-phase pipeline:

1. Parse
2. Macro collection
3. Import resolution
4. Macro expansion
5. Control flow expansion
6. Partial merge
7. Scope construction and evaluation
8. Decorator validation
9. Schema validation
10. ID uniqueness
11. Document validation

All diagnostics (errors and warnings) are printed to stderr. If any errors are produced, the command exits with a non-zero status code.

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Document is valid |
| `1` | One or more errors (or warnings with `--strict`) |
| `2` | Argument error |

## Examples

Validate a file:

```bash
wcl validate config.wcl
```

Validate strictly (warnings are errors):

```bash
wcl validate --strict config.wcl
```

Validate against an external schema:

```bash
wcl validate --schema schemas/service.wcl config.wcl
```

Validate with external variables:

```bash
wcl validate --var PORT=8080 config.wcl
```

## Diagnostic Output

```
error[E070]: missing required field `port`
  --> config.wcl:12:3
   |
12 |   service svc-api {
   |   ^^^^^^^^^^^^^^^ missing field `port`
   |
   = required by schema `ServiceSchema`
```
