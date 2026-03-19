# wcl convert

Convert between WCL and other configuration formats.

## Usage

```bash
wcl convert <file> --to <format>
wcl convert <file> --from <format>
```

## Options

| Flag | Description |
|------|-------------|
| `--to <format>` | Convert WCL to the target format: `json`, `yaml`, or `toml` |
| `--from <format>` | Convert the source format to WCL: `json` |

## Description

`wcl convert` handles bidirectional conversion between WCL and common configuration formats.

**WCL to another format** (`--to`): the document is fully evaluated through the pipeline and the resolved output is serialized. The result is equivalent to `wcl eval --format <format>`.

**Another format to WCL** (`--from`): the input file is parsed as the given format and a WCL document is generated that represents the same data. The generated WCL uses plain attribute assignments and blocks — no macros, expressions, or schemas are introduced.

Output is written to stdout. Redirect to a file to save the result.

## Supported Formats

| Format | To WCL (`--from`) | From WCL (`--to`) |
|--------|:-----------------:|:-----------------:|
| JSON   | Yes               | Yes               |
| YAML   | No                | Yes               |
| TOML   | No                | Yes               |

## Examples

Convert WCL to JSON:

```bash
wcl convert config.wcl --to json
```

Convert WCL to YAML:

```bash
wcl convert config.wcl --to yaml
```

Convert WCL to TOML:

```bash
wcl convert config.wcl --to toml
```

Convert JSON to WCL:

```bash
wcl convert config.json --from json
```

Save the result to a file:

```bash
wcl convert config.wcl --to json > config.json
wcl convert config.json --from json > config.wcl
```
