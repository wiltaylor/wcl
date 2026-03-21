# Attribute Macros

Attribute macros are invoked as decorators on a block and **transform** the block in place. Rather than generating new top-level content, they modify the block they are attached to — injecting child content, setting or removing attributes, and applying conditional changes.

## Definition

```wcl
macro @name(param1, param2 = default) {
    // transform operations
}
```

The `macro` keyword is followed by `@name` (with the leading `@`) to indicate this is an attribute macro, then a parameter list and a body.

## Invocation

Attribute macros are called by placing them as a decorator on a block declaration:

```wcl
service "api" @with_monitoring(alert_channel = "sre") {
    port = 8080
}
```

The macro receives the block as its implicit `self` target and executes its transform operations against it.

## Transform Operations

### inject { }

Adds child content to the block. All declarations inside `inject` are merged into the block's body:

```wcl
macro @with_defaults() {
    inject {
        region   = "us-east-1"
        env      = "production"
        replicas = 1
    }
}

service "api" @with_defaults() {
    port = 8080
}
// result: service "api" { port = 8080, region = "us-east-1", env = "production", replicas = 1 }
```

Injected values do not overwrite attributes already present on the block. Use `set` for that.

### set { }

Sets or overwrites attributes on the block. Unlike `inject`, `set` will replace an existing value:

```wcl
macro @force_tls() {
    set {
        tls      = true
        protocol = "https"
    }
}

service "api" @force_tls() {
    port     = 8080
    protocol = "http"   // will be overwritten to "https"
}
```

### remove [targets]

Removes attributes, child blocks, or tables from the block. Each target in the list uses a different syntax to specify what to remove:

| Syntax | Removes |
|---|---|
| `name` | Attribute with that name |
| `kind#id` | Child block of `kind` with inline ID `id` |
| `kind#*` | All child blocks of `kind` |
| `kind[n]` | The nth child block of `kind` (0-based) |
| `table#id` | Table with inline ID `id` |
| `table#*` | All tables |
| `table[n]` | The nth table (0-based) |

```wcl
macro @strip_debug() {
    remove [debug_port, verbose_logging, trace_id]
}

service "api" @strip_debug() {
    port         = 8080
    debug_port   = 9999
    verbose_logging = true
}
// result: service "api" { port = 8080 }
```

Removing child blocks and tables:

```wcl
macro @secure() {
    remove [endpoint#debug, table#metrics]
}

@secure()
service main {
    port = 8080
    endpoint health { path = "/health" }
    endpoint debug  { path = "/debug" }
    table metrics {
        key : string
        | "requests" |
    }
}
// result: endpoint debug and table metrics are removed
```

Wildcard and index removal:

```wcl
macro @clean() {
    remove [endpoint#*, table[0]]
}
```

### when condition { }

Applies a set of transform operations only when `condition` is true. The condition is a WCL boolean expression and may reference `self` properties:

```wcl
macro @environment_defaults(env) {
    when env == "production" {
        set {
            replicas = 3
            tls      = true
        }
    }
    when env == "development" {
        set {
            replicas = 1
            tls      = false
        }
    }
    inject {
        env = env
    }
}

service "api" @environment_defaults(env = "production") {
    port = 8080
}
```

### update selector { }

Applies transform directives to child blocks or row operations to tables. The selector identifies which child or table to target.

#### Updating child blocks

```wcl
macro @secure() {
    update endpoint#health {
        set { tls = true }
    }
    update endpoint {                // targets ALL endpoint children
        inject { auth = true }
    }
    update endpoint[0] {             // targets first endpoint by index
        set { primary = true }
    }
}
```

The body of a block update contains the same directives as the top-level macro body (`inject`, `set`, `remove`, `when`, `update`).

#### Updating tables (row operations)

When the selector targets a table (`table#id` or `table[n]`), the body contains **table directives** instead of transform directives:

| Directive | Effect |
|---|---|
| `inject_rows { \| val \| ... \| }` | Append rows to the table |
| `remove_rows where <expr>` | Remove rows where condition is true |
| `update_rows where <expr> { set { col = val } }` | Update cells in matching rows |
| `clear_rows` | Remove all data rows (columns preserved) |

```wcl
macro @filter_guests() {
    update table#users {
        remove_rows where role == "guest"
        inject_rows {
            | "admin" | "admin" |
        }
    }
}

@filter_guests()
service main {
    table users {
        name : string
        role : string
        | "alice" | "admin" |
        | "bob"   | "guest" |
    }
}
// result: bob/guest row removed, admin/admin row added
```

Row conditions reference column names directly. Only literal comparisons are supported at macro expansion time (`==`, `!=`, `>`, `<`, `>=`, `<=`, `&&`, `||`).

#### Composition with when

`update` directives can be nested inside `when` blocks for conditional transforms:

```wcl
macro @env_config(env) {
    when env == "production" {
        update endpoint#health {
            set { tls = true }
        }
        update table#config {
            remove_rows where key == "debug"
        }
    }
}
```

## The `self` Reference

Inside an attribute macro body, `self` refers to the block the macro is applied to. It exposes the block's properties as readable values for use in conditions and injected content.

| Expression           | Returns                                          |
|----------------------|--------------------------------------------------|
| `self.name`          | The block's type name (e.g. `"service"`)         |
| `self.kind`          | The block's kind string                          |
| `self.id`            | The block's ID label (e.g. `"api"`)              |
| `self.attr(name)`    | The value of the named attribute                 |
| `self.has(name)`     | `true` if the attribute exists on the block      |
| `self.labels`        | List of all label strings on the block           |
| `self.decorators`    | List of decorator names applied to the block     |

`self` is **only available in attribute macros**. It is not defined in function macro bodies.

### Examples Using self

Conditional injection based on an existing attribute:

```wcl
macro @with_monitoring(alert_channel = "ops") {
    inject {
        metrics_port = 9100
        health_path  = "/healthz"
    }
    set {
        monitoring_channel = alert_channel
    }
    when self.has("debug_port") {
        inject {
            debug_monitoring = true
        }
    }
}
```

Using `self.id` in injected values:

```wcl
macro @with_log_config() {
    inject {
        log_prefix = self.name + "/" + self.id
    }
}

service "api" @with_log_config() {
    port = 8080
}
// result: service "api" { port = 8080, log_prefix = "service/api" }
```

Branching on block type:

```wcl
macro @common_tags(team) {
    set {
        team = team
    }
    when self.name == "service" {
        inject {
            service_mesh = true
        }
    }
    when self.name == "job" {
        inject {
            retry_policy = "exponential"
        }
    }
}

service "api" @common_tags(team = "platform") {
    port = 8080
}

job "nightly-backup" @common_tags(team = "data") {
    schedule = "0 2 * * *"
}
```

## Full Example: @with_monitoring

```wcl
macro @with_monitoring(
    alert_channel:  string = "ops",
    metrics_port:   int    = 9100,
    health_path:    string = "/healthz"
) {
    inject {
        metrics_port = metrics_port
        health_path  = health_path
    }
    set {
        monitoring_channel = alert_channel
    }
    when self.has("env") {
        when self.attr("env") == "production" {
            set {
                alert_severity = "critical"
            }
        }
    }
}

service "api" @with_monitoring(alert_channel = "sre") {
    port = 8080
    env  = "production"
}

service "internal-tools" @with_monitoring() {
    port = 3000
    env  = "staging"
}
```

After expansion, `service "api"` will have `metrics_port = 9100`, `health_path = "/healthz"`, `monitoring_channel = "sre"`, and `alert_severity = "critical"` merged in. `service "internal-tools"` will have the same fields except `alert_severity` is absent (its `env` is not `"production"`).
