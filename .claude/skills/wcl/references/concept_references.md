# References

_&T fields that accept any value structurally satisfying an interface._

A `&T` field accepts any value that structurally satisfies interface `T`. It is how a record or function declares give-me-anything-that-looks-like-this without committing to a specific concrete type.

## Declaring a reference field

Use `&InterfaceName` as the field type. The accepted values are exactly those that satisfy [the interface](../references/concept_interfaces.md) — every required field, with compatible types.

```wcl
interface Drawable {
  x: f64
  y: f64
}

type Scene {
  focus: &Drawable     // any value with x and y of type f64
}
```

## Why a reference?

A reference field lets a schema be open-ended without being untyped. A renderer that consumes `&Drawable` can accept any future shape that satisfies the contract — a circle, an image, a container, a user's own type — without knowing those concrete types. Each value below carries extra fields the interface never mentions, and both are accepted because `x` and `y` are present.

```wcl
interface Drawable {
  x: f64
  y: f64
}

type Scene {
  focus: &Drawable
}

@document
type Gallery {
  s1: Scene
  s2: Scene
}

s1 = { focus: { x: 0.0,  y: 0.0,  radius: 5.0 } }   // circle-shaped
s2 = { focus: { x: 10.0, y: 10.0, side:   8.0 } }   // square-shaped
```

> [!NOTE]
> **Structural, not nominal**
> A type need not explicitly extend or implement the interface. If it has all the required fields with compatible types, a &T field accepts it.

## Related

- [Interfaces](../references/concept_interfaces.md)

- [Connections](../references/concept_connections.md)

[← Back to SKILL.md](../SKILL.md)
