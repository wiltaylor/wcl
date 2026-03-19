# Macros

Macros are WCL's code reuse mechanism. They let you define named templates that expand into blocks and attributes at the point of use — eliminating repetition without sacrificing explicitness in the final resolved document.

WCL has two distinct macro types with different invocation styles and capabilities:

## Function Macros

Function macros are reusable templates invoked at statement level, like a function call. They expand into one or more blocks or attributes in place.

```wcl
macro service_endpoint(name, port, env = "production") {
    service name {
        port   = port
        env    = env
        region = "us-east-1"
    }
}

service_endpoint("api", 8080)
service_endpoint("worker", 9090, env = "staging")
```

See [Function Macros](./macros-function.md) for full syntax and examples.

## Attribute Macros

Attribute macros are invoked as decorators on a block and **transform** that block. They can inject child content, set or remove attributes, and apply conditional changes based on the block's own properties.

```wcl
macro @with_monitoring(alert_channel = "ops") {
    inject {
        metrics_port = 9100
        health_path  = "/healthz"
    }
    set {
        monitoring_channel = alert_channel
    }
}

service "api" @with_monitoring(alert_channel = "sre") {
    port = 8080
}
```

See [Attribute Macros](./macros-attribute.md) for full syntax and examples.

## Parameters

Both macro types support typed parameters with optional defaults:

```wcl
macro deploy(name: string, replicas: int = 1, env: string = "production") {
    deployment name {
        replicas = replicas
        env      = env
    }
}
```

Type annotations are optional. When provided, WCL checks argument types at expansion time.

## Hygiene

Variables defined inside a macro with `let` are scoped to the macro definition site and do not leak into the caller's scope:

```wcl
macro make_service(name, port) {
    let internal_host = "internal." + name + ".svc"

    service name {
        port = port
        host = internal_host
    }
}

make_service("api", 8080)

// internal_host is NOT visible here
```

This prevents macros from accidentally shadowing or polluting the surrounding scope.

## Composition

Macros can call other macros. A function macro can invoke another function macro; an attribute macro can invoke function macros from within its body:

```wcl
macro base_service(name, port) {
    service name {
        port   = port
        region = "us-east-1"
    }
}

macro web_service(name, port, domain) {
    base_service(name, port)
    dns_record name {
        cname = domain
    }
}

web_service("api", 8080, "api.example.com")
```

## No Recursion

Direct and indirect recursion in macros is detected and rejected at expansion time. WCL does not support self-referential macro expansion.

```wcl
macro bad(n) {
    bad(n)    // error: recursive macro call detected
}
```

## Expansion Depth Limit

Macro expansion has a maximum depth of **64**. Chains of macro calls that exceed this depth produce an expansion error. This prevents runaway expansion from deeply nested composition.

## Further Reading

- [Function Macros](./macros-function.md) — definition syntax, parameters, invocation, examples
- [Attribute Macros](./macros-attribute.md) — transform operations, the `self` reference, examples
