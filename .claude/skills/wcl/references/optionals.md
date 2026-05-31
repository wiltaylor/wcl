# Optionals

An **optional** is a value that may be present or absent. The literal `none` represents absence; a `?` suffix on a type makes its values optional. Together they let a field be omitted, a function return "no answer", or a slot be cleared explicitly.

## Optional types

Suffix any type with `?` to make it optional. A `T?` field accepts either a `T` value or the literal `none`. A field without `?` is required and rejects `none`.

```wcl
type Profile {
  name:  utf8       // required
  bio:   utf8?      // optional — may be none
  age:   u32?       // optional
}

complete = Profile { name: "Alice", bio: "Author.", age: 34u32 }
partial  = Profile { name: "Bob",   bio: none,      age: none }
```

## The none literal

`none` is the value that fills an absent optional. It is also the result of builtins that don't return a useful value (`error`, `panic`, `assert`) — those have type `none` and cannot meaningfully be used in a value position.

```wcl
note: utf8? = none
```

## Working with optionals

Pattern-match an optional to handle the two cases. The `if let` shorthand is convenient when you only care about the present case.

```wcl
display = match maybe_name {
  none => "anonymous",
  n    => n,
}

shout = if let n = maybe_name {
  to_upper(n)
} else {
  "ANONYMOUS"
}
```

> [!NOTE]
> **Optionals vs unions**
> Use T? when the only states are present and absent. When absence carries information (a reason, a fallback, multiple shapes), reach for a union instead.
