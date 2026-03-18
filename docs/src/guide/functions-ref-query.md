# Reference & Query Functions

WCL provides four functions for working with blocks: `ref()` for resolving a block by ID, `query()` for selecting sets of blocks using a pipeline, `has()` for checking whether a block has a named attribute or child, and `has_decorator()` for checking whether a block carries a decorator. These are the bridge between the functional expression layer and the block-oriented document model.

The query engine is covered in depth in the [Query Engine](./query-engine.md) chapter. This page focuses on the function call syntax and common patterns.

## Reference

| Function | Signature | Description |
|---|---|---|
| `ref` | `ref(id: string) -> block` | Resolve a block by its ID; error if not found |
| `query` | `query(pipeline) -> list` | Execute a query pipeline; return matching blocks |
| `has` | `has(block, name: string) -> bool` | True if `block` has an attribute or child block named `name` |
| `has_decorator` | `has_decorator(block, name: string) -> bool` | True if `block` carries the decorator `@name` |

## ref

`ref(id)` looks up a block by its inline ID and returns it as a value. Attribute access on the returned block uses dot notation.

```wcl
service web {
  port: 8080
  image: "nginx:latest"
}

config proxy {
  upstream_port: ref("web").port     // 8080
  upstream_image: ref("web").image   // "nginx:latest"
}
```

If the ID does not exist, `ref` raises a compile-time error. `ref` can resolve any block type, not just `service`.

```wcl
database primary {
  host: "db.internal"
  port: 5432
}

service api {
  db_host: ref("primary").host
  db_port: ref("primary").port
}
```

## query

`query(pipeline)` runs a query pipeline against the document and returns a list of matching blocks. The pipeline syntax is described fully in the [Query Engine](./query-engine.md) chapter.

Basic form — select all blocks of a given type:

```wcl
let all_services = query(service)
let all_workers  = query(worker)
```

With a filter:

```wcl
let prod_services = query(service | where env == "prod")
```

With multiple pipeline stages:

```wcl
let names = query(service | where env == "prod" | select name)
```

Query results are lists. Use collection and higher-order functions on them:

```wcl
let ports = map(query(service), s => s.port)
let total_replicas = sum(map(query(service), s => s.replicas))
```

Use query results in for loops:

```wcl
for svc in query(service | where has(@public)) {
  ingress ingress-${svc.name} {
    target: svc.name
    port: svc.port
  }
}
```

## has

`has(block, name)` tests whether a block contains a named attribute or child block. It returns `false` rather than erroring when the name is absent, making it safe to use in conditionals and filters.

```wcl
service api {
  port: 8080
  // no "tls" attribute
}

service secure {
  port: 443
  tls: true
}
```

```wcl
let has_tls = has(ref("secure"), "tls")     // true
let api_tls = has(ref("api"), "tls")        // false
```

Use in a filter to find blocks that have a particular attribute:

```wcl
let tls_services = filter(query(service), s => has(s, "tls"))
```

Use in a conditional to selectively generate configuration:

```wcl
for svc in query(service) {
  if has(svc, "port") {
    health_check check-${svc.name} {
      url: "http://${svc.name}:${svc.port}/health"
    }
  }
}
```

## has_decorator

`has_decorator(block, name)` tests whether a block was annotated with the decorator `@name`. The name is passed without the `@` prefix.

```wcl
@public
service web {
  port: 80
}

service internal-api {
  port: 8080
}
```

```wcl
let is_public = has_decorator(ref("web"), "public")           // true
let api_public = has_decorator(ref("internal-api"), "public") // false
```

Filter to all publicly exposed services:

```wcl
let public_services = filter(query(service), s => has_decorator(s, "public"))
```

Generate ingress rules only for `@public` blocks:

```wcl
for svc in query(service) {
  if has_decorator(svc, "public") {
    ingress ingress-${svc.name} {
      host: "${svc.name}.example.com"
      port: svc.port
    }
  }
}
```

## Combining ref, query, has, and has_decorator

These four functions compose with the rest of WCL's expression language:

```wcl
// All services that have a port attribute and are marked @public
let exposed = filter(
  query(service | where has(@public)),
  s => has(s, "port")
)

// Generate a summary block
config fleet_summary {
  total_services: len(query(service))
  public_count: count(query(service), s => has_decorator(s, "public"))
  total_replicas: sum(map(query(service), s => s.replicas))
  primary_db_host: ref("primary").host
}
```

For advanced query pipeline syntax including multi-stage pipelines, recursive queries, and the `select` and `order_by` stages, see the [Query Engine](./query-engine.md) chapter.
