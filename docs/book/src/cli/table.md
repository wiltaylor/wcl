# wcl table

Manipulate data tables in WCL files.

## Usage

```bash
wcl table insert <file> <table> <values>
wcl table remove <file> <table> --where <condition>
wcl table update <file> <table> --where <condition> --set <assignments>
```

## Subcommands

### insert

Insert a row into a table.

| Argument | Description |
|------|-------------|
| `<file>` | WCL file containing the table |
| `<table>` | Table name (inline ID) |
| `<values>` | Row values as pipe-delimited: `'"alice" \| 25'` |

### remove

Remove rows matching a condition.

| Argument / Flag | Description |
|------|-------------|
| `<file>` | WCL file containing the table |
| `<table>` | Table name (inline ID) |
| `--where <condition>` | WCL expression to match rows, e.g. `'name == "alice"'` |

### update

Update cells in rows matching a condition.

| Argument / Flag | Description |
|------|-------------|
| `<file>` | WCL file containing the table |
| `<table>` | Table name (inline ID) |
| `--where <condition>` | WCL expression to match rows |
| `--set <assignments>` | Comma-separated assignments: `'age = 26, role = "admin"'` |

## Examples

Given a WCL file with:

```wcl
table users {
    name:  string
    role:  string
    admin: bool
    | "alice" | "engineering" | true  |
    | "bob"   | "marketing"  | false |
}
```

Insert a row:

```bash
wcl table insert config.wcl users '"carol" | "engineering" | true'
```

Remove rows:

```bash
wcl table remove config.wcl users --where 'name == "bob"'
```

Update rows:

```bash
wcl table update config.wcl users --where 'name == "alice"' --set 'role = "management"'
```

All commands modify the file in place and print the updated table.
