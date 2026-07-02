# match

_Tests a value against a sequence of patterns, returning the body of the first that matches._

`match` tests a value against a sequence of patterns, returning the body of the first that matches. It is WCL's primary tool for destructuring unions, optionals, and any shape with multiple cases.

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

An `if expr` after a pattern adds a runtime test; the arm fires only when both the pattern matches and the guard is true.

```wcl
classify = match n {
  k if k < 0  => :neg,
  0           => :zero,
  k if k > 10 => :big,
  _           => :small,
}
```

## Related

- [Patterns](../references/fact_patterns.md)

- [if let](../references/concept_if_let.md)

- [Unions](../references/concept_unions.md)

- [Optionals](../references/concept_optionals.md)

[← Back to SKILL.md](../SKILL.md)
