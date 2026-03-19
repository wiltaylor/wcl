# wcl eval

Evaluate a WCL document and print the fully resolved output.

## Usage

```bash
wcl eval <file> [options]
```

## Options

| Flag | Description |
|------|-------------|
| `--format <fmt>` | Output format: `json`, `yaml`, or `toml` (default: `json`) |

## Description

`wcl eval` runs the full evaluation pipeline and serializes the resulting document to the requested format. All macros are expanded, all expressions evaluated, all imports merged, and all partial blocks resolved before output is produced.

The output represents the final, fully-resolved state of the document — suitable for consumption by tools that do not understand WCL natively.

## Examples

Evaluate and print as JSON (default):

```bash
wcl eval config.wcl
```

Evaluate and print as YAML:

```bash
wcl eval config.wcl --format yaml
```

Evaluate and print as TOML:

```bash
wcl eval config.wcl --format toml
```

Pipe output to another tool:

```bash
wcl eval config.wcl | jq '.service'
```

## Example Output

Given:

```wcl
let base_port = 8000

service svc-api {
  port = base_port + 80
  host = "localhost"
}
```

Running `wcl eval config.wcl` produces:

```json
{
  "service": {
    "svc-api": {
      "port": 8080,
      "host": "localhost"
    }
  }
}
```
