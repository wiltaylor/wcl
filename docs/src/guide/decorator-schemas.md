# Decorator Schemas

Decorator schemas let you define custom decorators with typed parameters, target restrictions, and constraints. Once defined, WCL validates every use of the decorator against its schema — checking argument types, required parameters, and applicable targets.

## Syntax

```wcl
decorator_schema "name" {
    target = [block, attribute, table, schema]

    param_name: type
    param_name: type @optional
    param_name: type @default(value)
}
```

The `target` field is a list of one or more of: `block`, `attribute`, `table`, `schema`. It controls where the decorator may legally appear.

## Example: Custom @rate_limit Decorator

```wcl
decorator_schema "rate_limit" {
    target = [attribute, block]

    requests:  int
    window_ms: int @default(1000)
    burst:     int @optional
}
```

This decorator can now be used on attributes and blocks:

```wcl
service "api" {
    calls_per_second = 100 @rate_limit(requests = 100, window_ms = 500)
    upload_endpoint  = "/upload" @rate_limit(requests = 10)
}
```

Applying `@rate_limit` to a schema field or table would produce a validation error because `schema` and `table` are not listed in `target`.

## Parameters

Each parameter is declared as a field with a name, type, and optional decorators.

### Required Parameters

Parameters without `@optional` or `@default` must be supplied at every call site:

```wcl
decorator_schema "retry" {
    target       = [block]
    max_attempts: int
    backoff_ms:   int
}
```

```wcl
service "api" @retry(max_attempts = 3, backoff_ms = 200) { ... }
// error: missing required parameter backoff_ms would be caught if omitted
```

### Optional Parameters

```wcl
decorator_schema "cache" {
    target = [attribute, block]
    ttl_ms: int
    key:    string @optional
}
```

### Parameters with Defaults

```wcl
decorator_schema "timeout" {
    target     = [block, attribute]
    seconds:    int
    on_timeout: string @default("error")
}
```

## Positional Parameter Mapping

The first non-optional parameter of a decorator schema is the **positional parameter**. When the decorator is called with a single bare argument (no `key =`), the value is mapped to this parameter:

```wcl
decorator_schema "doc" {
    target = [block, attribute, schema]
    text:   string
}
```

Both forms below are equivalent:

```wcl
port = 8080 @doc("The service port")
port = 8080 @doc(text = "The service port")
```

## Constraints

Use `@constraint` on the `decorator_schema` block itself to express relationships between parameters:

```wcl
decorator_schema "validate_range" @constraint(requires = ["min", "max"]) {
    target = [attribute, schema]
    min:    int @optional
    max:    int @optional
}
```

Available constraint kinds:

| Kind       | Description                                              |
|------------|----------------------------------------------------------|
| `any_of`   | At least one of the listed parameters must be provided   |
| `all_of`   | All listed parameters must be provided together          |
| `one_of`   | Exactly one of the listed parameters must be provided    |
| `requires` | If this decorator is present, the listed params are required |

```wcl
decorator_schema "alert" @constraint(any_of = ["email", "pagerduty", "slack"]) {
    target    = [block]
    email:     string @optional
    pagerduty: string @optional
    slack:     string @optional
}
```

## Validation

When a decorator is used in a document, WCL validates:

1. **Name match** — A `decorator_schema` with the decorator's name must exist.
2. **Valid target** — The decorated item's kind must be listed in `target`.
3. **Required parameters** — All non-optional, non-default parameters must be supplied.
4. **Type checking** — Each argument's value must match the declared parameter type.
5. **Constraints** — Any `@constraint` conditions are checked against the supplied arguments.

Errors from decorator schema validation are included in accumulative error reporting alongside schema validation errors.

## Embedded Examples with @example

You can embed a usage example directly in a `decorator_schema` using the `@example` decorator on the body:

```wcl
decorator_schema "rate_limit" {
    target    = [attribute, block]
    requests:  int
    window_ms: int @default(1000)

    @example {
        service "api" @rate_limit(requests = 500, window_ms = 1000) {
            port = 8080
        }
    }
}
```

The `@example` body is not evaluated as live configuration; it is stored as documentation metadata.

## Full Example

```wcl
decorator_schema "slo" {
    target       = [block]
    availability: float
    latency_p99:  int   @optional
    error_budget: float @default(0.001)

    @example {
        service "payments" @slo(availability = 0.999, latency_p99 = 200) {
            port = 443
        }
    }
}

service "payments" @slo(availability = 0.999, latency_p99 = 200) {
    port = 443
}

service "internal-tools" @slo(availability = 0.99) {
    port = 8080
}
```

WCL will validate that every `@slo` use targets a block, provides `availability`, and that `availability` and `error_budget` are floats.
