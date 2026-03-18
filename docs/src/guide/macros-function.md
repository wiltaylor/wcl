# Function Macros

Function macros are named templates that expand into blocks, attributes, or other declarations at statement level. They are the primary way to eliminate repetitive block definitions in WCL.

## Definition

```wcl
macro name(param1, param2, param3 = default_value) {
    // body: blocks, attributes, let bindings, other macro calls
}
```

The `macro` keyword is followed by the macro name, a parameter list, and a body enclosed in `{ }`.

## Parameters

Parameters are positional by default. Each parameter may optionally have:

- A **type annotation**: `name: string`
- A **default value**: `name = "default"` or `name: string = "default"`

Parameters with defaults are optional at the call site. Parameters without defaults are required.

```wcl
macro service_endpoint(
    name:     string,
    port:     int,
    region:   string = "us-east-1",
    env:      string = "production",
    replicas: int    = 1
) {
    service name {
        port     = port
        region   = region
        env      = env
        replicas = replicas
    }
}
```

## Invocation

Function macros are called at **statement level** — the same positions where you can write a block or `let` binding. They are not valid as expression values.

```wcl
service_endpoint("api", 8080)
service_endpoint("worker", 9090, region = "eu-west-1")
service_endpoint("batch", 7070, env = "staging", replicas = 4)
```

Arguments can be positional, named, or mixed. Named arguments may appear in any order and can follow positional ones:

```wcl
service_endpoint("api", 8080, env = "staging", replicas = 2)
```

## Body

The body of a function macro can contain:

- **Block declarations** — the most common use case
- **Attribute assignments** — when expanding into attribute context
- **`let` bindings** — scoped to the macro, not visible outside
- **Other macro calls** — for composition

```wcl
macro health_check(service_name, path = "/healthz", interval_s = 30) {
    let check_id = service_name + "-health"

    monitor check_id {
        target   = service_name
        path     = path
        interval = interval_s
        timeout  = 5
    }

    alert check_id + "-alert" {
        monitor = check_id
        channel = "ops"
    }
}

health_check("api")
health_check("worker", path = "/health", interval_s = 60)
```

This expands to two `monitor` blocks and two `alert` blocks.

## Hygiene

`let` bindings defined inside a macro are scoped to the macro. They are resolved at the **definition site**, not the call site. This means:

- The macro cannot accidentally read variables from the caller's scope (unless passed as arguments).
- The macro's internal variables do not leak into the caller's scope.

```wcl
let prefix = "global"

macro make_db(name) {
    let prefix = "db"           // shadows outer "prefix" inside this macro only
    let full_name = prefix + "-" + name

    database full_name {
        host = "db.internal"
    }
}

make_db("primary")   // expands to database "db-primary" { ... }

// "prefix" here is still "global" — the macro did not mutate it
```

## Composition

Function macros can call other function macros. This lets you build complex templates from simpler ones:

```wcl
macro base_service(name, port) {
    service name {
        port   = port
        region = "us-east-1"
        env    = "production"
    }
}

macro web_service(name, port, domain, tls = true) {
    base_service(name, port)

    dns_record name {
        cname = domain
        tls   = tls
    }

    health_check(name)
}

macro health_check(name) {
    monitor name + "-check" {
        target = name
        path   = "/healthz"
    }
}

web_service("api", 8080, "api.example.com")
web_service("dashboard", 3000, "dash.example.com", tls = false)
```

## Full Example

```wcl
macro service_endpoint(
    name:     string,
    port:     int,
    env:      string = "production",
    replicas: int    = 1,
    region:   string = "us-east-1"
) {
    service name {
        port     = port
        env      = env
        replicas = replicas
        region   = region
    }
}

macro health_check(name, path = "/healthz", interval_s = 30) {
    monitor name + "-health" {
        target   = name
        path     = path
        interval = interval_s
    }
}

macro monitored_service(name, port, env = "production") {
    service_endpoint(name, port, env = env)
    health_check(name)
}

monitored_service("api",    8080)
monitored_service("worker", 9090, env = "staging")
monitored_service("batch",  7070, env = "development")
```

This expands to three `service` blocks and three `monitor` blocks, each correctly parameterized.
