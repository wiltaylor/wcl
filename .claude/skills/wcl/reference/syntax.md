# WCL Syntax Reference

Canonical authoring syntax for WCL. Source of truth: `crates/wcl_lang/src/lang/ast.rs` and `crates/wcl_lang/src/lang/parser/mod.rs`.

## Blocks

```wcl
# Bare block
server {
  host = "0.0.0.0"
  port = 8080
}

# With inline ID (bare identifier, not a string)
server web-1 {
  host = "0.0.0.0"
  port = 8080
}

# With inline arguments (any primary expressions)
server web 8080 "prod" {
  host = "localhost"
}
# Without a schema, inline args appear as `_args = [web, 8080, "prod"]`.
# With `@inline(N)` on schema fields, positional args bind to named fields.

# Nested blocks qualify IDs: the inner port gets qualified id `alpha.http`
service alpha {
  port http { weight = 100 }
}
```

Block IDs are globally unique within a scope regardless of kind (E030).

## Attributes

```wcl
name    = "acme-app"
port    = 8080
debug   = false
tags    = ["prod", "api"]
config  = { timeout = 30, retries = 3 }
```

## Literal Types

| Type | Example | Notes |
|------|---------|-------|
| `string` | `"hello"` | Escapes and `\uHHHH` supported |
| `i8/i16/i32/i64/i128` | `42`, `-100` | `i64` is default int |
| `u8/u16/u32/u64/u128` | `255` | |
| `f32/f64` | `3.14`, `2.998e8` | `f64` is default float |
| `bool` | `true`, `false` | |
| `null` | `null` | |
| `date` | `d"2026-03-15"` | ISO 8601 |
| `duration` | `dur"P1Y2M3D"` | ISO 8601 |
| `symbol` | `:GET`, `:POST` | Colon prefix |
| `list` | `[1, 2, 3]` | |
| `map` | `{ k1 = "v", k2 = 42 }` | |
| `identifier` | bare ident used where a name is needed | |

Integer literal forms: `1000000`, `0xFF`, `0o755`, `0b1010_1010`, `1_000_000` (underscores allowed).

## Strings

```wcl
# Standard
greeting = "Hello, ${name}!"   # ${...} interpolation
escaped  = "line\nnext\ttab"
quoted   = "He said \"hi\""

# Heredoc (preserves indent)
msg = <<EOF
  First line
  Second line
EOF

# Indented heredoc (strips common leading whitespace)
msg = <<-EOF
  First line
  Second line
  EOF

# Raw heredoc (no escapes, no interpolation)
tmpl = <<'EOF'
  Use ${placeholder} verbatim.
  EOF
```

## Let Bindings

```wcl
let environment = "production"
let is_prod     = environment == "production"

# Partial: multiple declarations concatenate into one list
partial let overrides = [{ svc = "api", port = 9090 }]
partial let overrides = [{ svc = "cache", port = 6379 }]

# Export: visible across import boundary
export let base_port = 8000
```

- E038: partial let value must be a list.
- E039: mixing `partial` and non-partial for the same name.

## Imports

```wcl
# Relative path
import "./schemas/service.wcl"
import "../shared/base.wcl"

# Library (XDG search paths)
import <wdoc.wcl>

# Optional (no error if missing / no glob match)
import? "./optional.wcl"
import? "./env/*.wcl"

# Glob (sorted, deduplicated)
import "./configs/*.wcl"

# Lazy: load only if namespace is referenced via `use`
import "./heavy.wcl" lazy(heavy_ns)
use heavy_ns::item

# Non-WCL
cert    = import_raw("./certs/server.pem")
rows    = import_table("./data.csv")                            # default headers=true
rows    = import_table("./data.tsv", "\t")
rows    = import_table("./data.csv", headers=false, columns=["a","b"])
```

Search paths for `<name.wcl>`: `$XDG_DATA_HOME/wcl/lib/` then `$XDG_DATA_DIRS/wcl/lib/`. Override with `--lib-path` and `--no-default-lib-paths`.

- E010 not found, E011 jail escape, E014 depth, E015 library not found, E016 glob no match, E017 lazy needs namespace.

## Namespaces and `use`

```wcl
# Braced namespace
namespace myapp {
  schema "service" { port: i64 }
  service api { port = 8080 }
}

# File-level (must appear before other items — E123)
namespace myapp
schema "service" { port: i64 }
service api { port = 8080 }

# Import a name from a namespace
use myapp::service
service api { port = 8080 }

# Aliased
use myapp::{service -> svc}
svc api { port = 8080 }
```

Namespaces do NOT participate in qualified ID hierarchy — only inline IDs.

## Control Flow

```wcl
# if/else (conditions must be bool — E026)
if is_prod {
  service api { port = 443 }
} else {
  service api { port = 8080 }
}

# Ternary
workers = is_prod ? 8 : 2

# For over list
let regions = ["us-east-1", "eu-west-1"]
for region in regions {
  deployment deploy-${region} { region = region }
}

# For with index
for region, i in regions {
  deployment deploy-${region} { position = i }
}

# Range
for i in range(0, 5) {
  worker worker-${i} { id = i }
}

# Iterate a block query
for svc in (..service) {
  p "Host ${svc.id} on port ${svc.port}"
}
```

## Macros

```wcl
# Function macro
macro service_endpoint(name, port, env = "production") {
  service name {
    port = port
    env = env
  }
}
service_endpoint("api", 8080)
service_endpoint("worker", 9090, env = "staging")

# Typed parameters
macro deploy(name: string, replicas: i64 = 1) {
  deployment name { replicas = replicas }
}

# Attribute macro (applies like a decorator, rewrites the block)
macro @with_monitoring(alert_channel = "ops") {
  inject {
    metrics_port = 9100
    health_path = "/healthz"
  }
  set {
    monitoring_channel = alert_channel
  }
}

service "api" @with_monitoring(alert_channel = "sre") { port = 8080 }
```

Hygiene: macro-local `let` bindings are not visible outside the macro. Max expansion depth 64 (E022). Recursion detected (E021).

## Queries / Selectors

Query expressions run at evaluation time and return lists of blocks or values:

```wcl
(..service)                    # all service blocks anywhere
(service)                      # all top-level services
(service#web)                  # the service with inline ID `web`
(service | ..port)             # all port blocks under any service
(service | .env == "prod")     # filter by attribute
(service | .port)              # project .port
(service | [.port > 8000])     # filter form
(service, database | .env)     # multiple kinds
```

## Refs

```wcl
ref(alpha)            # bare ID
ref("alpha.http")     # qualified path
ref("../alpha")       # relative, from inside a block
```

A ref resolves to the target block value. In schemas, use `@ref("schema_name")` to cross-reference a specific kind.

## Tables

```wcl
# Inline columns + rows
table users {
  name: string
  role: string
  admin: bool

  | "alice" | "engineering" | true  |
  | "bob"   | "marketing"   | false |
}

# Column decorators
table user_roles {
  username: string @doc("Login name")
  role: string     @validate(one_of = ["admin", "viewer"])
  max_items: i64   @default(100)
  api_key: string  @sensitive

  | "alice" | "admin"  | 500 | "key-abc" |
  | "bob"   | "viewer" |     | "key-xyz" |
}

# Schema reference — colon
schema "user_row" { name: string, age: i64 }
table users : user_row {
  | "Alice" | 30 |
  | "Bob"   | 25 |
}

# Schema reference — decorator
@schema("user_row")
table users {
  | "Alice" | 30 |
}

# Assignment / CSV import
table users = import_table("./users.csv")

# Heredoc cells
table docs {
  title: string
  body:  string

  | "Getting Started" | <<-EOF
      Install the package.
      EOF
  |
}
```

- E090: `@table_index` references nonexistent column.
- E091: duplicate value in unique index.
- E092: inline columns with a schema applied.

## Validation Blocks

```wcl
validation "port uniqueness" {
  let ports = (server | .port)
  check   = len(distinct(ports)) == len(ports)
  message = "All server ports must be unique"
}

validation "prefer-tls" @warning {
  let non_tls = (service | !has(@tls))
  check   = len(non_tls) == 0
  message = "Some services are not using TLS"
}
```

- E050: invalid validation expression.
- E080: document validation failed.

## Partial Blocks

```wcl
# In file1.wcl
partial server web-1 {
  host = "0.0.0.0"
}

# In file2.wcl — merged into one server block
partial server web-1 {
  port = 8080
}
```

- E030 duplicate non-partial ID.
- E031 attribute conflict on merge.
- E032 kind mismatch.
- E033 mixed partial / non-partial.

## Implicit Attributes

- `_args` — list of inline arg values when no schema applies.
- `_kind` — the block's kind string.
- `_id` — the block's inline ID (qualified when nested).

## Removed Forms (do not use)

- `label` / `labels` field on blocks — replaced by `inline_args`.
- `KindLabel` / `TableLabel` query selectors, kind+string-label path segments — removed.
- Quoted inline IDs like `service "alpha"` — inline IDs are bare identifiers only.
