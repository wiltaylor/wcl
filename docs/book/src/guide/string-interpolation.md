# String Interpolation

String interpolation lets you embed expressions directly inside string literals. The interpolated value is converted to a string and spliced into the surrounding text at runtime.

## Syntax

Use `${expression}` inside a double-quoted string or a heredoc:

```wcl
name = "api-gateway"
port = 8080

url = "http://${name}:${port}/health"
// → "http://api-gateway:8080/health"
```

The expression inside `${...}` can be any valid WCL expression.

## Supported Expression Types

### Variable References

```wcl
let env = "production"
label = "env:${env}"
// → "env:production"
```

### Arithmetic

```wcl
let base = 8000
addr = "port ${base + 80} is in use"
// → "port 8080 is in use"
```

### Function Calls

```wcl
name = "my-service"
display = "Service: ${upper(name)}"
// → "Service: MY-SERVICE"

items = ["a", "b", "c"]
summary = "Items: ${join(", ", items)}"
// → "Items: a, b, c"
```

### Ternary Expressions

```wcl
debug = true
mode = "Running in ${debug ? "debug" : "release"} mode"
// → "Running in debug mode"
```

### Member Access

```wcl
config {
  version = "2.1.0"
}

banner = "WCL config v${config.version}"
// → "WCL config v2.1.0"
```

### Nested Interpolation

Interpolations can be nested when the inner expression itself contains a string with interpolation:

```wcl
let prefix = "api"
let version = 2
path = "/v${version}/${prefix}-${to_string(version * 10)}"
// → "/v2/api-20"
```

## Type Coercion in Interpolation

When an interpolated expression evaluates to a non-string type, it is automatically converted:

| Type         | Conversion rule                          | Example result       |
|--------------|------------------------------------------|----------------------|
| `string`     | Used as-is                               | `"hello"` → `hello`  |
| `int`        | Decimal representation                   | `42` → `42`          |
| `float`      | Decimal representation                   | `3.14` → `3.14`      |
| `bool`       | `true` or `false`                        | `true` → `true`      |
| `null`       | The literal string `"null"`              | `null` → `null`      |
| `identifier` | The identifier's name                    | `foo` → `foo`        |
| `list`       | **Runtime error** — not auto-converted   |                      |
| `map`        | **Runtime error** — not auto-converted   |                      |
| `function`   | **Runtime error** — not auto-converted   |                      |

To embed a list or map in a string, use an explicit conversion function such as `join()` or `to_string()`.

## Escaping

To include a literal `${` in a string without triggering interpolation, escape the dollar sign with a backslash:

```wcl
template = "Use \${variable} in your templates."
// → "Use ${variable} in your templates."
```

## Interpolation in Heredocs

Standard and indented heredocs support interpolation. Raw heredocs (using single-quoted delimiters) do not.

Standard heredoc with interpolation:

```wcl
let host = "db.internal"
let port = 5432

dsn = <<EOF
postgresql://${host}:${port}/mydb
EOF
```

Indented heredoc with interpolation:

```wcl
let name = "my-service"
let version = "1.0.0"

banner = <<-EOF
  Service: ${name}
  Version: ${version}
  EOF
```

Raw heredoc (interpolation disabled):

```wcl
example = <<'EOF'
  Use ${variable} to embed values at runtime.
EOF
// → "  Use ${variable} to embed values at runtime.\n"
```

## Practical Examples

### Building URLs

```wcl
let scheme  = "https"
let host    = "api.example.com"
let version = 2

base_url     = "${scheme}://${host}/v${version}"
health_check = "${base_url}/health"
```

### Log Format Strings

```wcl
let service = "auth"
let level   = "INFO"

log_prefix = "[${upper(level)}] ${service}:"
```

### Configuration File Paths

```wcl
let app_name = "my-app"
let env      = "production"

config_path = "/etc/${app_name}/${env}/config.yaml"
log_path    = "/var/log/${app_name}/${env}.log"
```

### Dynamic Labels

```wcl
let region  = "us-east-1"
let zone    = "a"

availability_zone = "${region}${zone}"       // "us-east-1a"
resource_tag      = "zone:${region}-${zone}" // "zone:us-east-1-a"
```
