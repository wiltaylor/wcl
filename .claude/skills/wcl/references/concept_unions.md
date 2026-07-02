# Unions

_Tagged variant sets — a value that is exactly one of several alternatives._

A `union` is a tagged set of variants — a value that is exactly one of several alternatives. Each variant carries its own payload (or none), and pattern matching dispatches on the tag.

## Variant body forms

Three shapes are accepted: a **record** body with named fields, a **typeref** body for a single positional payload, or a **unit** body with no payload.

```wcl
union Shape {
  Circle { radius: f64, stroke: f64 }   // record variant
  Polygon i32                            // typeref variant
  Empty                                  // unit variant
}
```

## Constructing variants

Use `Union::Variant` syntax — record payloads in braces, typeref payloads in parentheses, unit constructors bare.

```wcl
a = Shape::Circle { radius: 5.0, stroke: 0.5 }
b = Shape::Polygon(7)
c = Shape::Empty
```

## Inferring the variant from shape

When the expected type is a union — a union-typed field, an element of a `list<Union>`, or a function parameter — you can drop the `Union::Variant` tag and write a bare record. The variant is inferred from the record's field shape.

```wcl
// `series: list<ChartSeries>` — the variant is inferred per element.
series = [
  { name: "North", values: [42.0, 55.0] },   // inferred ChartSeries::Of
  { name: "South", values: [30.0, 48.0] },
]
```

The match is by field-name set (and field types when two variants share a name set), so it only works when the shape is unambiguous; a bare record matching no variant is a build error. Reach for the explicit `Union::Variant { ... }` form to disambiguate.

## extends

A union can `extends` another, inheriting its variants and adding more — useful when a host wants to extend an open vocabulary without modifying the base declaration.

```wcl
union BaseShape {
  Empty
}

union Shape extends BaseShape {
  Circle { radius: f64 }
  Square { side: f64 }
}
```

## Examples

### A union with three variant forms

Variants may carry a record body, a single typeref payload, or nothing.

```wcl
union Shape {
  Circle { radius: f64, stroke: f64 }   // record variant
  Polygon i32                            // typeref variant
  Empty                                  // unit variant
}

a = Shape::Circle { radius: 5.0, stroke: 0.5 }
b = Shape::Polygon(7)
c = Shape::Empty
```

**Expected:** Three values, each a different variant of `Shape`.

## Related

- [Records](../references/concept_records.md)

- [Symbols](../references/concept_symbols.md)

[← Back to SKILL.md](../SKILL.md)
