# wcl query

Execute a query expression against a WCL document.

## Usage

```bash
wcl query <file> <query> [options]
wcl query --recursive <dir> <query> [options]
```

## Options

| Flag | Description |
|------|-------------|
| `--format <fmt>` | Output format: `text`, `json`, `csv`, or `wcl` (default: `text`) |
| `--count` | Print the number of matching results instead of the results themselves |
| `--recursive` | Search recursively across all `.wcl` files in a directory |

## Description

`wcl query` evaluates a query pipeline against the resolved document and prints matching results. The query syntax is the same as the inline `query(...)` expression in WCL source.

A query pipeline is a selector followed by zero or more filters separated by `|`.

## Selectors

| Syntax | Selects |
|--------|---------|
| `service` | All blocks of type `service` |
| `service#svc-api` | Block with type `service` and ID `svc-api` |
| `..service` | All `service` blocks at any depth |
| `*` | All top-level items |
| `.` | The root document |

## Filters

| Syntax | Meaning |
|--------|---------|
| `.port` | Has attribute `port` |
| `.port == 8080` | Attribute `port` equals `8080` |
| `.name =~ "api.*"` | Attribute `name` matches regex |
| `has(.port)` | Has attribute `port` |
| `has(@deprecated)` | Has decorator `@deprecated` |
| `@tag.env == "prod"` | Decorator `@tag` has named arg `env` equal to `"prod"` |

## Examples

Select all services:

```bash
wcl query config.wcl 'service'
```

Select a specific block by ID:

```bash
wcl query config.wcl 'service#svc-api'
```

Filter by attribute value:

```bash
wcl query config.wcl 'service | .port == 8080'
```

Filter by regex match:

```bash
wcl query config.wcl 'service | .name =~ ".*-api"'
```

Count matching blocks:

```bash
wcl query config.wcl 'service' --count
```

Output as JSON:

```bash
wcl query config.wcl 'service | .port > 1024' --format json
```

Query recursively across a directory:

```bash
wcl query --recursive ./configs 'service | has(@deprecated)'
```

Filter by decorator argument:

```bash
wcl query config.wcl 'service | @tag.env == "prod"'
```
