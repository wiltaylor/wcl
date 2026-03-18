# Query Engine

The query engine lets you search, filter, and project over all blocks in a document using a concise pipeline syntax. Queries can appear anywhere an expression is expected — inside `let` bindings, attribute values, and validation blocks.

## Pipeline Syntax

```wcl
query(selector | filter | filter | ... | projection?)
```

A pipeline is a `selector` followed by zero or more `filter` steps and an optional trailing `projection`. Each step is separated by `|`.

## Selectors

Selectors determine the initial set of blocks the pipeline operates on.

| Selector | Matches |
|---|---|
| `service` | All blocks of kind `service` |
| `service#svc-auth` | The block of kind `service` with ID `svc-auth` |
| `service."auth-service"` | The block of kind `service` with label `"auth-service"` |
| `config.server.listener` | A nested attribute path — the `listener` attribute inside `server` inside `config` |
| `..health_check` | Recursive descent — all blocks named `health_check` at any depth |
| `.` | The document root |
| `*` | All top-level blocks |
| `table."name"` | The table with label `"name"` |
| `table#id` | The table with the given ID |

### Examples

```wcl
// All services
let all_services = query(service)

// A specific service by ID
let auth = query(service#svc-auth)

// All tables
let all_tables = query(table.*)

// Every health_check block anywhere in the document
let checks = query(..health_check)
```

## Filters

Filters narrow the result set. Multiple filters are AND-combined — every filter in the chain must match.

### Attribute Comparison

```wcl
query(service | .port > 8080)
query(service | .env == "production")
query(service | .env != "staging")
query(service | .workers >= 4)
```

### Regex Match

```wcl
query(service | .name =~ "^api-")
query(config  | .region =~ "us-.*")
```

### Existence Check: `has`

```wcl
// Blocks that have an 'auth' attribute
query(service | has(.auth))

// Blocks that carry a specific decorator
query(service | has(@sensitive))
query(service | has(@validate))
```

### Decorator Argument Filtering

Filter on the arguments of a decorator:

```wcl
// Services whose @validate decorator has min > 0
query(service | @validate.min > 0)

// Blocks with @retry where attempts >= 3
query(service | @retry.attempts >= 3)
```

### Compound Filters

Chain multiple filters to AND them together:

```wcl
query(service | .env == "production" | .port > 8000 | has(@tls))
```

## Projections

A projection extracts a single attribute value from each matched block, producing a list of values instead of a list of block references. The projection step must be the **last** step in the pipeline.

```wcl
// List of port numbers from all production services
let prod_ports = query(service | .env == "production" | .port)

// List of names from all services
let names = query(service | .name)
```

Without a projection, `query()` returns a `list(block_ref)`. With a projection, it returns a `list(value)`.

## Result Types

| Pipeline | Return type |
|---|---|
| `query(service)` | `list(block_ref)` |
| `query(service \| .port)` | `list(value)` (the projected attribute values) |

## Aggregate Functions

Aggregate functions operate on query results:

| Function | Description |
|---|---|
| `len(query(...))` | Number of matched items |
| `sum(query(... \| .attr))` | Sum of numeric projection |
| `avg(query(... \| .attr))` | Average of numeric projection |
| `distinct(query(... \| .attr))` | Deduplicated list of projected values |

```wcl
let service_count  = len(query(service))
let total_workers  = sum(query(service | .workers))
let avg_port       = avg(query(service | .port))
let all_envs       = distinct(query(service | .env))
```

## Using Queries in Validation

```wcl
validation "all-prod-services-have-tls" {
    let prod = query(service | .env == "production")
    check   = every(prod, s => has(s.tls))
    message = "all production services must have TLS configured"
}

validation "unique-ports" {
    let ports = query(service | .port)
    check   = len(ports) == len(distinct(ports))
    message = "each service must use a unique port"
}
```

## CLI Usage

The `wcl query` subcommand runs a pipeline against a file from the command line:

```sh
wcl query file.wcl 'service'
wcl query file.wcl 'service | .env == "prod" | .port'
wcl query file.wcl 'service | has(@tls)'
```

### Output Formats

| Flag | Output |
|---|---|
| `--format json` | JSON array |
| `--format text` | One item per line (default) |
| `--format csv` | Comma-separated values |
| `--format wcl` | WCL block representation |

### Additional Flags

| Flag | Description |
|---|---|
| `--count` | Print the number of results instead of the results themselves |
| `--recursive` | Descend into imported files |

### Examples

```sh
# Count production services
wcl query --count infra.wcl 'service | .env == "production"'

# Export port list as JSON
wcl query --format json infra.wcl 'service | .env == "prod" | .port'

# Find all services without TLS
wcl query infra.wcl 'service | !has(@tls)'

# Recursively find all health_check blocks across imports
wcl query --recursive infra.wcl '..health_check'
```
