# Attributes

An attribute is a named value binding inside a block or at module scope. It is the primary way to attach data to a configuration node.

## Syntax

```
name = expression
```

The **name** must be a valid identifier: it may contain ASCII letters, digits, and underscores, and must not start with a digit. Hyphens are not allowed in attribute names (they are only permitted in inline block IDs).

```wcl
// Valid attribute names
host        = "localhost"
port_number = 3000
max_retries = 5
_internal   = true

// Invalid — hyphens are not allowed
// max-retries = 5   // parse error
```

## Duplicate Attributes

Declaring the same attribute name twice within the same block is an error:

```wcl
server {
  port = 8080
  port = 9090  // error: duplicate attribute "port"
}
```

## Value Types

The right-hand side of an attribute can be any WCL expression.

### Literals

```wcl
string_val  = "hello"
int_val     = 42
float_val   = 3.14
bool_val    = true
null_val    = null
```

### Variable References

```wcl
let base_port = 8000

service {
  port    = base_port
  alt_port = base_port + 1
}
```

### Arithmetic Expressions

```wcl
timeout     = 30 * 1000          // milliseconds
buffer_size = 4 * 1024 * 1024    // 4 MiB
half_port   = base_port / 2
remainder   = total % batch_size
```

### Function Calls

```wcl
name_upper = upper("my-service")
tag_count  = len(tags)
checksum   = sha256(payload)
combined   = concat("prefix-", name)
```

### String Interpolation

```wcl
greeting = "Hello, ${user_name}!"
url      = "https://${host}:${port}/api/v${version}"
```

See [String Interpolation](./string-interpolation.md) for full details.

### Ternary Expressions

```wcl
mode    = debug ? "verbose" : "quiet"
timeout = is_production ? 5000 : 30000
```

### Queries

Query expressions select blocks from the current scope and return lists or single values:

```wcl
all_servers  = query server
prod_servers = query server where env == "production"
first_host   = (query server)[0].host
```

See [Query Engine](./query-engine.md) for the complete query syntax.

### Refs

A `ref` creates a typed reference to another block by its inline ID:

```wcl
database #primary {
  host = "db1.internal"
  port = 5432
}

connection {
  target = ref(db-primary)
}
```

### Lists and Maps

```wcl
ports    = [8080, 8081, 8082]
labels   = ["web", "api", "public"]
env_vars = { HOST: "0.0.0.0", PORT: "8080" }
```

### Comparison and Logical Expressions

```wcl
is_valid   = port > 0 && port < 65536
is_dev     = env == "development" || env == "dev"
is_enabled = !disabled
matches    = name =~ "^api-"
```

## Summary

| Value Kind          | Example                                 |
|---------------------|-----------------------------------------|
| String literal      | `"hello"`                               |
| Integer literal     | `42`, `0xFF`, `0b1010`                  |
| Float literal       | `3.14`, `1.5e-3`                        |
| Boolean literal     | `true`, `false`                         |
| Null literal        | `null`                                  |
| Variable reference  | `base_port`                             |
| Arithmetic          | `base_port + 1`                         |
| Comparison/logical  | `port > 0 && port < 65536`              |
| Ternary             | `debug ? "verbose" : "quiet"`           |
| Function call       | `upper("hello")`                        |
| String interpolation| `"http://${host}:${port}"`              |
| Query               | `query(service \| .port)`               |
| Ref                 | `ref(svc-api)`                          |
| List                | `[1, 2, 3]`                             |
| Map                 | `{ key: "value" }`                      |
