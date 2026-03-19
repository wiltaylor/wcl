# If/Else Conditionals

If/else conditionals let you include or exclude blocks, attributes, and children based on boolean expressions. Like for loops, they are declarative: the conditional is fully resolved during the control flow expansion phase, and only the matching branch appears in the evaluated document.

## Syntax

```wcl
if expression {
  // branch body
} else if expression {
  // branch body
} else {
  // fallback body
}
```

Any number of `else if` clauses may be chained. The `else` clause is optional. All branches are syntactically identical to block bodies and can contain any WCL statements.

## Conditions Must Be Boolean

WCL does not perform implicit coercion on condition expressions. The expression must evaluate to a `bool`. Passing a non-bool value is a compile-time error:

```wcl
let count = 5

// Correct
if count > 0 { ... }

// Error: count is an int, not a bool
if count { ... }
```

## Conditionals Inside Block Bodies

The most common use of `if/else` is to selectively include attributes or child blocks inside a block body:

```wcl
service api {
  port: 8080

  if env == "prod" {
    replicas: 3
    tls: true
  } else {
    replicas: 1
    tls: false
  }
}
```

## Inline Attribute Values

`if/else` can appear as an expression to select between two values:

```wcl
service api {
  replicas: if env == "prod" { 3 } else { 1 }
  log_level: if debug { "debug" } else { "info" }
}
```

When used as an expression, each branch must contain exactly one value expression.

## Using Queries as Conditions

The result of `has()` or a boolean query expression can drive a conditional:

```wcl
for svc in query(service) {
  if has(svc, "port") {
    ingress ingress-${svc.name} {
      target: svc.name
      port: svc.port
    }
  }
}
```

```wcl
if has_decorator(myblock, "public") {
  expose myblock {
    external: true
  }
}
```

## Chained Conditions

```wcl
let tier = "gold"

config limits {
  max_requests: if tier == "gold" {
    10000
  } else if tier == "silver" {
    5000
  } else {
    1000
  }
}
```

## Composition with For Loops

`if/else` and `for` loops compose freely. A conditional can appear inside a for loop body, and a for loop can appear inside a conditional branch:

```wcl
for env in ["staging", "prod"] {
  service web-${env} {
    replicas: if env == "prod" { 3 } else { 1 }

    if env == "prod" {
      alerts {
        pagerduty: true
      }
    }
  }
}
```

```wcl
if enable_workers {
  for i in range(0, worker_count) {
    worker worker-${i} {
      id: i
    }
  }
}
```

## Discarded Branches Are Not Validated

Only the matching branch is included in the expanded AST. The other branches are discarded before evaluation and schema validation. This means a discarded branch can reference names or produce structures that would otherwise be invalid, as long as the condition that guards it is false:

```wcl
if false {
  // This block is never evaluated, so undefined_var is not an error
  item x { value: undefined_var }
}
```

Use this carefully — relying on discarded branches to suppress errors can make configs harder to understand.

## Nesting Depth

If/else conditionals count toward the global nesting depth limit (default 32), shared with for loops.

## Expansion Phase

Conditionals are expanded during **phase 5: control flow expansion**, after macro expansion. At the end of this phase the AST contains only the winning branch's content and no `if`/`else` nodes remain.
