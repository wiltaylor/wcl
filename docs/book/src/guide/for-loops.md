# For Loops

For loops let you generate repeated blocks, attributes, or values by iterating over a collection. They expand during the control flow phase into concrete nodes — by the time evaluation runs, all loops have already been unrolled.

## Syntax

### Basic form

```wcl
for item in expression {
  // body: any WCL statements
}
```

### With index

```wcl
for item, index in expression {
  // index is 0-based
}
```

`expression` must evaluate to a list. Ranges, query results, and literal lists are all valid.

## Iterating Over Lists

```wcl
let regions = ["us-east-1", "eu-west-1", "ap-southeast-1"]

for region in regions {
  deployment deploy-${region} {
    region: region
    replicas: 2
  }
}
```

## Iterating Over Ranges

Use the `range(start, end)` built-in to produce a list of integers:

```wcl
for i in range(0, 5) {
  worker worker-${i} {
    id: i
  }
}
```

`range(0, 5)` produces `[0, 1, 2, 3, 4]` (end is exclusive).

## Using the Index

```wcl
for name, i in ["alpha", "beta", "gamma"] {
  shard shard-${i} {
    label: name
    position: i
  }
}
```

## Iterating Over Map Keys

Use `keys()` to iterate over the keys of a map:

```wcl
let limits = { cpu: "500m", memory: "256Mi" }

for key in keys(limits) {
  resource_limit lim-${key} {
    name: key
    value: limits[key]
  }
}
```

## Iterating Over Query Results

Query results are lists of blocks, so they can be used directly as the loop source:

```wcl
for svc in query(service | where has(@public)) {
  ingress ingress-${svc.name} {
    target: svc.name
    port: svc.port
  }
}
```

## Identifier Interpolation in Inline IDs

When a block is declared inside a for loop, its inline ID can interpolate loop variables using `${variable}` syntax:

```wcl
for name in ["web", "api", "worker"] {
  service svc-${name} {
    image: "app/${name}:latest"
  }
}
```

This expands to three separate `service` blocks with IDs `svc-web`, `svc-api`, and `svc-worker`. Interpolation is resolved at expansion time, not evaluation time, so the resulting IDs are static strings in the evaluated document.

Multiple variables and arbitrary expressions are supported inside `${}`:

```wcl
for env in ["staging", "prod"] {
  for tier in ["web", "db"] {
    group ${env}-${tier} {
      label: "${env}/${tier}"
    }
  }
}
```

## Nested For Loops

For loops can be nested up to the global nesting depth limit (default 32):

```wcl
for region in regions {
  for zone in zones {
    node node-${region}-${zone} {
      region: region
      zone: zone
    }
  }
}
```

The total iterations across all loops combined must not exceed 10,000, and each individual loop must not exceed 1,000 iterations.

## For Loops Inside Blocks

A for loop can appear inside a block body to generate multiple child blocks or repeated attributes:

```wcl
cluster main {
  for region in regions {
    node node-${region} {
      region: region
    }
  }
}
```

## Composition with Macros

For loops can appear inside macro bodies and macros can be called inside for loops:

```wcl
macro base_service(name, port) {
  service ${name} {
    port: port
    health_check: "/${name}/health"
  }
}

for svc in services {
  @base_service(svc.name, svc.port)
}
```

## Scoping

Each iteration of a for loop creates a new child scope. The loop variable is bound in that scope and shadows any outer variable with the same name without producing a warning. Variables defined inside the loop body are not visible outside the loop.

```wcl
let name = "outer"

for name in ["a", "b", "c"] {
  // name refers to the iteration variable here, not "outer"
  item x-${name} { label: name }
}

// name is "outer" again here
```

## Empty Lists

Iterating over an empty list produces zero iterations and no output — it is not an error:

```wcl
for item in [] {
  // never expanded
}
```

## Limits

| Limit | Default |
|---|---|
| Iterations per loop | 1,000 |
| Total iterations across all loops | 10,000 |
| Maximum nesting depth | 32 |

Exceeding any limit is a compile-time error.
