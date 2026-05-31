# Interfaces

An `interface` declares a structural contract — a set of fields a type must have to satisfy it. Any type with the right fields satisfies it automatically, no explicit `implements`.

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
> A type does not need to be declared as a parent or to implement anything explicitly. If it happens to have all the interface's fields with compatible types, it satisfies the interface.

Interfaces have one common consumer: **reference fields** (`&T`), which accept any value that structurally satisfies the interface. That's what the next chapter covers.
