# match

_Tests a value against a sequence of patterns, returning the body of the first that matches._

`match` tests a value against a sequence of patterns, returning the body of the first that
matches. It is WCL's primary tool for destructuring unions, optionals, and any shape with
multiple cases.


```wcl
area = match shape {
  Shape::Circle { radius, .. } => pi() * radius * radius,
  Shape::Polygon(n) if n > 2   => 0.0,
  Shape::Empty                 => 0.0,
  _                            => 0.0,
}
```

See [Patterns](../references/fact_patterns.md) for the full pattern syntax.

## Guards

An `if expr` after a pattern adds a runtime test; the arm fires only when both the pattern
matches and the guard is true.


```wcl
classify = match n {
  k if k < 0  => :neg,
  0           => :zero,
  k if k > 10 => :big,
  _           => :small,
}
```

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

- [Patterns](../references/fact_patterns.md) — Patterns supports match: Patterns destructure and test values in `match`, `if let`, and guard arms.

- [Unions](../references/concept_unions.md) — Unions supports match: Tagged variant sets — a value that is exactly one of several alternatives.

- [Optionals](../references/concept_optionals.md) — Optionals supports match: Values that may be present or absent — the none literal and the ? type suffix.

[← Back to SKILL.md](../SKILL.md)
