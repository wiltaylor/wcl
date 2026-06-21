# Control Flow

_if/else, match with patterns, if let, let bindings, blocks, and try/catch._

WCL's expression-level constructs — `if`/`else`, `match` with patterns, `if let`, `let` bindings, block expressions, and `try`/`catch` — combine to compute values from values. Every construct here is an **expression**: it returns a value, never a statement.

## if / else

An `if` is an expression. The branches must agree on a type. `else if` chains for multi-way branches.

```wcl
sign = if x < 0 { :neg } else if x > 0 { :pos } else { :zero }
```

## match

`match` tests a value against a sequence of patterns, returning the body of the first that matches. It is WCL's primary tool for destructuring unions, optionals, and any shape with multiple cases.

```wcl
area = match shape {
  Shape::Circle { radius, .. } => pi() * radius * radius,
  Shape::Polygon(n) if n > 2   => 0.0,
  Shape::Empty                 => 0.0,
  _                            => 0.0,
}
```

### Patterns

| Pattern | Matches |
| --- | --- |
| `_` | Anything (wildcard) |
| `name` | Anything, binding the value to `name` |
| `name @ inner` | Match `inner`, also bind whole as `name` |
| Literal (`42`, `"hi"`, `:red`) | Equality with a literal |
| `Union::Variant {...}` | A specific variant; `..` ignores remaining fields |
| `Union::Variant(x)` | Typeref variant, binding payload to `x` |
| `Union::Variant` | Unit variant |
| `pat1 \| pat2` | Either pattern matches |

### Guards

An `if expr` after a pattern adds a runtime test; the arm fires only when both the pattern matches and the guard is true.

```wcl
classify = match n {
  k if k < 0  => :neg,
  0           => :zero,
  k if k > 10 => :big,
  _           => :small,
}
```

## if let

`if let pattern = value { ... } else { ... }` binds and branches in a single step. Use it when you only care about a single variant or pattern.

```wcl
label = if let Shape::Circle { radius, .. } = s {
  format("circle r={}", radius)
} else {
  "other"
}
```

## let bindings

A `let` names a reusable value or function. It resolves by name in sibling and descendant expressions but is **not** document data — it never appears in output, JSON, or schema validation. Lexically scoped, lazily evaluated, cycle-detected.

### Item form

At file scope or inside any block, `let name = expr` introduces a name. No terminator.

```wcl
let base_port = 8080u32

service "web" {
  port = base_port
}
service "api" {
  port = base_port + 1u32
}
```

Because a `let` can bind a function, it doubles as a composition helper. See [Functions](../references/concept_functions.md).

```wcl
let scale = fn(p: f64) -> f64 p * 2.0

a = scale(3.0)    // 6.0
b = scale(4.5)    // 9.0
```

## Block expressions

A `{ ... }` expression holds zero or more `let ... ;` bindings followed by a tail expression, which is the block's value.

```wcl
result = {
  let a = to_upper("x");
  let b = to_upper("y");
  len(a) + len(b)
}
```

## try / catch

`try body catch name => handler` evaluates the body; if evaluation fails — a builtin error, an `error()` call, a cycle, or a propagated field error — the rendered message binds to the catch name (a `utf8`) and the handler's value is the result. Both sides accept a `{ ... }` block.

```wcl
rate    = try parse_rate(raw) catch m => 1.0
summary = try {
  let r = risky()
  format("ok: {}", r)
} catch msg {
  format("failed: {}", msg)
}
```

> [!NOTE]
> **Catches everything**
> try makes any evaluation failure recoverable, including cycles and upstream field errors — use it where a fallback is meaningful, not to paper over schema mistakes (wcl check still reports those).

## Examples

### Matching a union with guards

`match` returns the body of the first pattern that matches; a guard adds a runtime test.

```wcl
area = match shape {
  Shape::Circle { radius, .. } => pi() * radius * radius,
  Shape::Polygon(n) if n > 2   => 0.0,
  Shape::Empty                 => 0.0,
  _                            => 0.0,
}
```

**Expected:** `area` is computed from whichever variant `shape` holds.

## Related

- [Expressions](../references/concept_expressions.md)

- [Optionals](../references/concept_optionals.md)

- [Unions](../references/concept_unions.md)

- [Functions](../references/concept_functions.md)

[← All concepts](../references/concepts_ref.md)
