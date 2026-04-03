# Data Tables

WCL's `table` construct provides structured, typed tabular data inside your configuration. Tables are first-class values: they can be validated, queried, and deserialized just like any other block.

## Basic Syntax

```wcl
table id {
    column_name : type
    another_col : type

    | value1 | value2 |
    | value3 | value4 |
}
```

The block contains two sections in order: column declarations followed by rows. Column declarations must appear before any row.

## Column Declarations

Each column is declared as `name : type`. The supported types are the same primitive types used elsewhere in WCL (`string`, `i64`, `f64`, `bool`).

Columns accept the following decorators:

| Decorator | Purpose |
|---|---|
| `@validate(expr)` | Constraint expression applied to every cell in this column |
| `@doc("text")` | Human-readable description of the column |
| `@sensitive` | Marks column values as sensitive (redacted in output) |
| `@default(value)` | Fallback value when a row omits this column |

```wcl
table user_roles {
    username  : string  @doc("Login name")
    role      : string  @validate(one_of(["admin", "viewer", "editor"]))
    max_items : i64     @default(100)
    api_key   : string  @sensitive

    | "alice" | "admin"  | 500 | "key-abc" |
    | "bob"   | "viewer" |     | "key-xyz" |
}
```

## Row Syntax

Rows are written as pipe-delimited expressions:

```wcl
| expr1 | expr2 | expr3 |
```

Each cell is a full WCL expression, so you can reference variables, call built-in functions, and perform arithmetic:

```wcl
let base_port = 8000

table services {
    name : string
    port : i64

    | "auth"    | base_port + 1 |
    | "gateway" | base_port + 2 |
    | "metrics" | base_port + 3 |
}
```

The number of values in every row must exactly match the number of declared columns. A mismatch is a parse error.

Each cell value is type-checked against its column's declared type. A type mismatch produces a validation error.

## Inline IDs

Tables require an inline ID:

```wcl
table perms_main {
    role     : string
    resource : string
    action   : string
    allow    : bool

    | "admin"  | "users"   | "delete" | true  |
    | "viewer" | "users"   | "read"   | true  |
    | "viewer" | "users"   | "write"  | false |
}
```

The ID `perms_main` can then be used in `@ref` decorators and query selectors such as `table#perms_main`.

## Schema Reference

You can apply an existing schema to a table instead of declaring columns inline. This is useful when multiple tables share the same structure.

### Colon syntax

```wcl
schema "user_row" {
    name : string
    age  : i64
}

table users : user_row {
    | "Alice" | 30 |
    | "Bob"   | 25 |
}
```

### Decorator syntax

```wcl
@schema("user_row")
table users {
    | "Alice" | 30 |
    | "Bob"   | 25 |
}
```

When a schema is applied, you cannot also declare inline columns. Doing so produces error E092.

## Multiline Cell Values (Heredocs)

When a table cell contains multiline text — such as documentation, scripts, or templates — use a heredoc instead of a regular string literal:

```wcl
table docs {
    title : string
    body  : string

    | "Getting Started" | <<-EOF
        Install the package:
        npm install wcl

        Then import it:
        const wcl = require("wcl")
        EOF
    |
    | "Configuration" | <<-EOF
        Create a config file and add your settings.
        EOF
    |
}
```

The closing `|` for a heredoc cell goes on its own line after the heredoc delimiter.

All heredoc variants work in table cells:

| Variant | Syntax | Behaviour |
|---|---|---|
| Standard | `<<EOF` | Content preserved as-is |
| Indented | `<<-EOF` | Strips leading whitespace based on closing delimiter indent |
| Raw | `<<'EOF'` | No escape processing, no `${…}` interpolation |
| Indented raw | `<<-'EOF'` | Both indented and raw |

Use `<<-TAG` (indented) for readability — the content aligns with the surrounding table indent and leading whitespace is stripped automatically.

Heredoc cells support interpolation (except in raw mode):

```wcl
let pkg = "wcl"

table steps {
    name : string
    cmd  : string

    | "install" | <<-EOF
        npm install ${pkg}
        EOF
    |
}
```

> **Tip:** Put heredoc cells in the last column for the cleanest visual layout.

## Loading Tables from CSV

Use `import_table("path.csv")` to load a CSV file as a table value.

```wcl
let acl = import_table("./acl.csv")
```

### Options

`import_table` accepts named arguments for fine-grained control:

| Parameter | Type | Default | Description |
|---|---|---|---|
| `separator` | string | `","` | Field separator character |
| `headers` | bool | `true` | Whether the first row contains column headers |
| `columns` | list | — | Explicit column names (overrides headers) |

```wcl
# Tab-separated (legacy positional syntax still works)
let tsv = import_table("./data.tsv", "\t")

# Named separator argument
let tsv = import_table("./data.tsv", separator="\t")

# No header row — columns are named "0", "1", ...
let raw = import_table("./data.csv", headers=false)

# No header row with explicit column names
let data = import_table("./data.csv", headers=false, columns=["name", "age"])
```

### Table assignment syntax

You can populate a table directly from a CSV file using assignment syntax:

```wcl
table users = import_table("data.csv")
```

Combine with a schema reference to validate imported data:

```wcl
table users : user_row = import_table("data.csv")
```

The first row of the CSV is treated as the column header by default. All cell values are imported as strings; apply a schema if you need typed validation.

`import_table` follows the same path rules as `import`: relative paths only, resolved from the importing file, jailed to the project root.

### Let-bound Tables

You can assign an `import_table` call to a `let` binding. The table data becomes a list of row maps that can be used in expressions, for loops, and function calls:

```wcl
let data = import_table("users.csv")

// Iterate over rows
for row in data {
  service ${row.name}-svc {
    role = row.role
  }
}
```

Let-bound tables are not included in the serialized output (like all let bindings), but their data is available for use in expressions and control flow.

## Table Manipulation Functions

WCL provides built-in functions for working with table data (lists of row maps):

### find(table, key, value)

Returns the first row where `key` equals `value`, or `null` if not found:

```wcl
let data = import_table("users.csv")
let admin = find(data, "role", "admin")
admin_name = admin.name
```

### filter(table, predicate)

Returns all rows matching the predicate (a lambda):

```wcl
let data = import_table("users.csv")
let admins = filter(data, (r) => r.role == "admin")
admin_count = len(admins)
```

### insert_row(table, row)

Returns a new list with the given row map appended:

```wcl
let data = import_table("users.csv")
let extended = insert_row(data, {name = "charlie", role = "viewer"})
```

These functions work on any list of maps, not just `import_table` results.

## Evaluation

Tables are evaluated into a list of row maps. Each row becomes a map from column name to cell value. For example:

```wcl
table users {
    name : string
    age  : i64
    | "alice" | 25 |
    | "bob"   | 30 |
}
```

evaluates to a value equivalent to:

```json
[
    {"name": "alice", "age": 25},
    {"name": "bob", "age": 30}
]
```

Cell expressions are fully evaluated, so references, function calls, and arithmetic all work:

```wcl
let base = 100

table config {
    key   : string
    value : i64
    | "port"  | base + 80 |
    | "debug" | 0          |
}
// config evaluates to [{"key": "port", "value": 180}, {"key": "debug", "value": 0}]
```

Tables inside blocks appear in the block's attributes map, keyed by the table's inline ID. Tables at the top level appear as top-level values.

## Deserialization

When deserializing a document into Rust types, a table maps to `Vec<T>` where `T` is a struct whose fields correspond to the column names:

```rust
#[derive(Deserialize)]
struct PermRow {
    role: String,
    resource: String,
    action: String,
    allow: bool,
}

let rows: Vec<PermRow> = doc.get_table("permissions")?;
```

## Querying Tables

Use `query()` to filter rows. The selector `table#id` targets a specific table; filters then match on column values:

```wcl
validation "no-admin-deletes-on-prod" {
    let dangerous = query(table#permissions | .role == "viewer" | .allow == true | .action == "delete")
    check   = len(dangerous) == 0
    message = "viewers must not have delete permission"
}
```

The full query pipeline syntax is described in the [Query Engine](./query-engine.md) chapter. Key points for tables:

- `.col == val` — exact match on a column value
- `.col =~ "pattern"` — regex match
- `.col > val` — numeric comparison
- `has(.col)` — column exists and is non-null
- Append `| .col` at the end to project a single column as a list of values

### Example: Permissions Table

```wcl
table perms_main {
    role     : string  @doc("Subject role")
    resource : string  @doc("Target resource type")
    action   : string  @validate(one_of(["read", "write", "delete"]))
    allow    : bool    @doc("Whether the action is permitted")

    | "admin"  | "users"    | "read"   | true  |
    | "admin"  | "users"    | "write"  | true  |
    | "admin"  | "users"    | "delete" | true  |
    | "editor" | "posts"    | "read"   | true  |
    | "editor" | "posts"    | "write"  | true  |
    | "editor" | "posts"    | "delete" | false |
    | "viewer" | "posts"    | "read"   | true  |
    | "viewer" | "posts"    | "write"  | false |
    | "viewer" | "posts"    | "delete" | false |
}
```

Fetch all actions allowed for `editor`:

```wcl
let editor_allowed = query(table#perms_main | .role == "editor" | .allow == true | .action)
```

Count total denied rules:

```wcl
let denied_count = len(query(table#perms_main | .allow == false))
```
