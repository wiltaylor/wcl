# Interfaces

_Structural contracts — a set of fields a type must have, satisfied automatically._

An `interface` declares a structural contract — a set of fields a type must have to satisfy it. Any type with the right fields satisfies it automatically, with no explicit `implements`.

## Declaring an interface

```wcl
interface Drawable {
  x: f64
  y: f64
}

interface Sized extends Drawable {
  width:  f64
  height: f64
}
```

> [!NOTE]
> **Structural, not nominal**
> A type need not be declared as a parent or implement anything explicitly. If it happens to have all the interface's fields with compatible types, it satisfies the interface.

Interfaces have one common consumer: **reference fields** (`&T`), which accept any value that structurally satisfies the interface. See [References](../references/concept_references.md).

## Examples

### A structural interface

Any type with `x` and `y` of type `f64` satisfies `Drawable` — no explicit implements.

```wcl
interface Drawable {
  x: f64
  y: f64
}

interface Sized extends Drawable {
  width:  f64
  height: f64
}
```

**Expected:** Any record with the listed fields satisfies the interface automatically.

## Related

- [Records](../references/concept_records.md)

- [References](../references/concept_references.md)

[← Back to SKILL.md](../SKILL.md)
