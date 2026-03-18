# Control Flow

WCL provides two declarative control flow structures: **for loops** and **if/else conditionals**. Unlike imperative languages, these constructs do not execute at runtime — they expand into concrete blocks and attributes during the control flow expansion phase of the pipeline, before evaluation.

This design keeps WCL configs purely declarative: the final evaluated document contains no loops or conditionals, only the concrete values they produced.

## Structures

- **[For Loops](./for-loops.md)** — iterate over lists, ranges, or query results to generate repeated blocks or values.
- **[If/Else Conditionals](./if-else.md)** — conditionally include blocks, attributes, or children based on boolean expressions.

## Expansion Phase

Both constructs are processed during **phase 5: control flow expansion**, after macro expansion and before partial merge and evaluation. At the end of this phase, the AST contains only concrete nodes.

## Limits

To prevent runaway configs and protect tooling performance, WCL enforces the following hard limits:

| Limit | Default |
|---|---|
| Maximum nesting depth (loops + conditionals combined) | 32 |
| Maximum iterations per single for loop | 1,000 |
| Maximum total iterations across all for loops | 10,000 |

Exceeding any of these limits is a compile-time error. The limits exist to keep configs analyzable and prevent accidental exponential expansion.

## Composition

For loops and if/else can be freely composed:

```wcl
for env in ["staging", "prod"] {
  service web-${env} {
    replicas: if env == "prod" { 3 } else { 1 }
  }
}
```

See the individual pages for full syntax, scoping rules, and examples.
