# wcl transform

Run data transformations defined in WCL files.

## Usage

```bash
wcl transform run <name> -f <file> [options]
```

## Subcommands

### run

Execute a named transform.

| Argument / Flag | Description |
|------|-------------|
| `<name>` | Transform name (the block's inline ID) |
| `-f, --file <path>` | WCL file containing the transform definition |
| `--input <path>` | Input data file (reads from stdin if omitted) |
| `--output <path>` | Output data file (writes to stdout if omitted) |
| `--param KEY=VALUE` | Set a parameter (may be repeated) |
| `--lib-path <dir>` | Extra library search path (may be repeated) |
| `--no-default-lib-paths` | Disable default XDG/system library search paths |

## Description

`wcl transform run` finds a `transform` block by its inline ID in the given WCL file, then reads input data, applies the field mappings defined in the transform's `map` blocks, and writes the transformed output.

The transform block specifies input and output codecs (currently `json`), and contains `map` sub-blocks that define how input fields are mapped to output fields. Inside map blocks, `in` refers to the current input record.

Transform statistics (records read, written, filtered) are printed to stderr.

## Examples

### Define a transform

```wcl
// transforms.wcl
transform rename-fields {
    input = "codec::json"
    output = "codec::json"

    map {
        user_name = in.name
        user_age  = in.age
    }
}
```

### Run it

```bash
wcl transform run rename-fields -f transforms.wcl --input data.json --output result.json
```

### Stream from stdin to stdout

```bash
cat data.json | wcl transform run rename-fields -f transforms.wcl
```

### With filtering

```wcl
transform active-users {
    input = "codec::json"
    output = "codec::json"

    @where(in.active == true)
    map {
        name = in.name
        email = lower(in.email)
    }
}
```

```bash
wcl transform run active-users -f transforms.wcl --input users.json
```

Output on stderr:
```
transform 'active-users': 100 records read, 42 written, 58 filtered
```
