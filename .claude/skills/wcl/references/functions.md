# Functions

Functions are first-class values. They can be bound by `let`, stored in fields, passed as arguments, and returned from other functions.

## Function literals

`fn(params) -> Return body`. The body is a single expression, or a `{ … }` block expression.

```wcl
double = fn(x: i32) -> i32 x * 2
sum_sq = fn(x: i32, y: i32) -> i32 { let s = x + y; s * s }
```

## fn items

A reusable function is declared once with the `fn` item form — sugar for `let name = fn(…)` with two extras: the name shows up in editor tooling (outline, hover, go-to-definition, rename) and it can carry a `@doc` decorator. Like a `let`, a `fn` item is a composition helper, not data: it never appears in `get`, JSON output, or schema validation, and it can be declared at file scope or inside a block (scoped to that block).

```wcl
@doc("Clamp a value into [lo, hi].")
fn clamp_to(x: f64, lo: f64, hi: f64) -> f64 {
  min(max(x, lo), hi)
}

gain = clamp_to(raw_gain, 0.0, 1.0)
```

## Higher-order functions

Functions can take and return other functions. This is how the collection builtins like `map`, `filter`, and `fold` are parameterised.

```wcl
adder = fn(x: i32) -> fn(i32) -> i32 fn(y: i32) -> i32 x + y
add3  = adder(3)
seven = add3(4)

doubled = map([1, 2, 3], fn(x: i64) -> i64 x * 2)
```

## Function types

The type `fn(T1, T2, …) -> R` describes a callable. Use it for fields that hold callbacks.

```wcl
type Step {
  apply: fn(i32) -> i32
}
```

> [!NOTE]
> **Lazy and cycle-checked**
> Function values participate in WCL's lazy field evaluation. Each call evaluates its body in a fresh context, and circular references between fields are detected and reported.
