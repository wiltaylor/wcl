# Validation Blocks

Validation blocks let you encode invariants about your configuration that go beyond per-field type and constraint checking. They run after all other pipeline phases — including imports, macro expansion, partial merging, and schema validation — so they can reason about the fully assembled document.

## Syntax

```wcl
validation "name" {
    let intermediate = some_expr

    check   = bool_expression
    message = "human-readable error text"
}
```

- `check` must evaluate to a `bool`. If it is `false`, a validation error (code E080) is emitted with the text from `message`.
- `message` is a string expression and may use interpolation.
- `let` bindings inside the block are local — they are computed before `check` is evaluated and are not visible outside the block.
- Multiple `let` bindings are allowed; they are evaluated in order.

## Execution Order

Validation blocks run in phase 11, after:

1. Parsing
2. Macro collection
3. Import resolution
4. Macro expansion
5. Control flow expansion
6. Partial merge
7. Scope construction and evaluation
8. Decorator validation
9. Schema validation
10. ID uniqueness checks
11. **Document validation** ← validation blocks run here

This means `query()` calls inside a validation block see the complete, merged, evaluated document.

## Warnings

Decorate a validation block with `@warning` to downgrade a failure from an error to a warning. The configuration is still accepted; the diagnostic is reported with warning severity.

```wcl
validation "prefer-tls" @warning {
    let non_tls = query(service | !has(@tls))
    check   = len(non_tls) == 0
    message = "some services are not using TLS (non-fatal)"
}
```

## Documentation

`@doc` adds a human-readable description surfaced by the LSP and tooling:

```wcl
validation "unique-ports" @doc("Each service must listen on a distinct port") {
    let ports = query(service | .port)
    check   = len(ports) == len(distinct(ports))
    message = "duplicate port assignments detected"
}
```

## Local Bindings for Intermediate Computation

Use `let` bindings to break complex checks into readable steps:

```wcl
validation "prod-services-have-health-checks" {
    let prod         = query(service | .env == "production")
    let prod_with_hc = query(service | .env == "production" | has(.health_check))
    check   = len(prod) == len(prod_with_hc)
    message = "every production service must define a health_check block"
}
```

## Common Patterns

### Unique Ports

```wcl
validation "unique-service-ports" {
    let ports = query(service | .port)
    check   = len(ports) == len(distinct(ports))
    message = "each service must use a unique port number"
}
```

### TLS Coverage

```wcl
validation "prod-tls-required" {
    let prod_no_tls = query(service | .env == "production" | !has(@tls))
    check   = len(prod_no_tls) == 0
    message = "all production services must be decorated with @tls"
}
```

### Service Existence Checks

```wcl
validation "gateway-references-valid-services" {
    let gateway_upstreams = query(gateway | .upstream)
    let service_names     = query(service | .name)
    check   = every(gateway_upstreams, u => contains(service_names, u))
    message = "gateway.upstream references a service that does not exist"
}
```

### Universal Quantifier with `every`

```wcl
validation "all-services-have-env" {
    let services = query(service)
    check   = every(services, s => has(s.env))
    message = "every service must declare an env attribute"
}
```

### Cross-Table Integrity

```wcl
validation "permission-roles-are-defined" {
    let perm_roles    = distinct(query(table."permissions" | .role))
    let defined_roles = query(table."roles" | .name)
    check   = every(perm_roles, r => contains(defined_roles, r))
    message = "permissions table references a role not present in the roles table"
}
```

### Counting and Thresholds

```wcl
validation "minimum-replica-count" @warning {
    let under_replicated = query(service | .env == "production" | .replicas < 2)
    check   = len(under_replicated) == 0
    message = "production services should run at least 2 replicas"
}
```

## Multiple Validation Blocks

You may define as many validation blocks as you need. Each is evaluated independently; all failures are collected and reported together.

```wcl
validation "unique-ports"          { ... }
validation "prod-tls-required"     { ... }
validation "all-services-have-env" { ... }
```

## Error Code

Validation block failures are reported under error code **E080**. Schema-level constraint failures (from `@validate` in a schema) use E073–E075. Both kinds appear in the same diagnostic output.
