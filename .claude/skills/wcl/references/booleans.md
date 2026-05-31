# Booleans

The `bool` type has exactly two values: `true` and `false`.

```wcl
enabled = true
strict  = false
```

## In expressions

Booleans are produced by every comparison and combined with the logical operators `&&`, `||`, and `!`. See [Expressions](../references/expressions.md) for the full set.

```wcl
ready    = !pending && enabled
oversize = width > 100u32 || height > 100u32
```

> [!NOTE]
> **Branching on a bool**
> An if expression takes a bool condition. See Control Flow for the if / else syntax.
