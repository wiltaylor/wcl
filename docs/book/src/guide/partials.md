# Partial Declarations

Partials allow a single logical block to be defined across multiple fragments — in the same file or spread across imported files. All fragments are merged into one complete block before evaluation proceeds.

## Basic Syntax

```wcl
partial service svc-api "api-service" {
    port = 8080
}

partial service svc-api "api-service" {
    env = "production"
}
```

After merging, the result is equivalent to:

```wcl
service svc-api "api-service" {
    port = 8080
    env  = "production"
}
```

Rules:
- Every fragment sharing a type and ID must be marked `partial`. A non-partial block with the same type/ID as a partial fragment is an error.
- All fragments must have identical inline IDs.
- The merged block is placed at the position of the **first** fragment encountered.

## Attribute Merge Rules

By default, WCL uses **strict** conflict mode: if two fragments both define the same attribute, the merge fails with an error. This is the safest default — it catches accidental duplication.

```wcl
partial service svc-api "api-service" { port = 8080 }
partial service svc-api "api-service" { port = 9090 }  // Error: duplicate attribute 'port'
```

**Last-wins mode** relaxes this: the last fragment's value for a given attribute wins. Enable it by applying `@merge_strategy("last_wins")` to the fragments, or by configuring `ConflictMode::LastWins` programmatically:

```wcl
partial service svc-api "api-service" @merge_strategy("last_wins") {
    port    = 8080
    timeout = 30
}

partial service svc-api "api-service" @merge_strategy("last_wins") {
    port = 9090   // overrides the first fragment's value
}
```

## Child Block Merging

Child blocks nested inside partial fragments are merged recursively:

- Child blocks **with an ID** are matched by (type, ID) and merged by the same rules.
- Child blocks **without an ID** are appended in order.

```wcl
partial service svc-api "api-service" {
    endpoint ep-health "/health" {
        method = "GET"
    }
}

partial service svc-api "api-service" {
    endpoint ep-health "/health" {
        timeout = 5
    }
    endpoint "/metrics" {
        method = "GET"
    }
}
```

Merged result has `ep-health` with both `method` and `timeout`, plus the anonymous `/metrics` endpoint appended.

## Decorator Merging

Decorators from all fragments are combined. Duplicate decorator names are deduplicated — if the same decorator appears on multiple fragments, it is included once. The order follows fragment order.

```wcl
partial service svc-api "api-service" @doc("Main API") { ... }
partial service svc-api "api-service" @validate { ... }
// Merged block has both @doc and @validate
```

## Explicit Ordering: `@merge_order`

By default, fragments are merged in the order they are encountered (depth-first import order, then source order). Use `@merge_order(n)` to assign an explicit integer priority. Lower numbers sort first.

```wcl
partial service svc-api "api-service" @merge_order(10) {
    port = 8080
}

partial service svc-api "api-service" @merge_order(1) {
    // This fragment is applied first despite appearing second
    log_level = "debug"
}
```

This is useful when the merge order would otherwise depend on import order, which can be fragile.

## Documenting Dependencies: `@partial_requires`

`@partial_requires(["field1", "field2"])` is a documentation decorator that declares which attributes a fragment expects another fragment to supply. It has no effect on merge behaviour but is surfaced by the LSP and validation tooling to help detect incomplete configurations:

```wcl
partial service svc-api "api-service" @partial_requires(["port", "env"]) {
    // This fragment uses port and env but does not define them.
    // Another fragment must supply them.
    healthcheck_url = "http://localhost:" + str(port) + "/health"
}
```

If the merged block does not contain all fields listed in `@partial_requires`, a warning is emitted.

## Cross-File Composition

The most common use of partials is assembling a block from fragments in separate files:

```
services/
  api-base.wcl      — core attributes
  api-tls.wcl       — TLS configuration
  api-observability.wcl — metrics and tracing
```

```wcl
// api-base.wcl
partial service svc-api "api-service" {
    port    = 8443
    env     = "production"
    workers = 4
}
```

```wcl
// api-tls.wcl
partial service svc-api "api-service" {
    tls {
        cert = import_raw("./certs/server.pem")
        key  = import_raw("./certs/server.key")
    }
}
```

```wcl
// api-observability.wcl
partial service svc-api "api-service" {
    metrics {
        path = "/metrics"
        port = 9090
    }
    tracing {
        endpoint = "http://jaeger:14268/api/traces"
        sampling = 0.1
    }
}
```

```wcl
// main.wcl
import "./services/api-base.wcl"
import "./services/api-tls.wcl"
import "./services/api-observability.wcl"
```

The final document contains a single, fully merged `svc-api` block.

## ConflictMode Reference

| Mode | Behaviour on duplicate attribute |
|---|---|
| `ConflictMode::Strict` (default) | Error — duplicate attributes are forbidden |
| `ConflictMode::LastWins` | The value from the last fragment in merge order is kept |

Conflict mode is applied per-merge operation. When using the Rust API, pass `ConflictMode` to the merge phase options. When using decorators, `@merge_strategy("last_wins")` activates last-wins mode on that block.
