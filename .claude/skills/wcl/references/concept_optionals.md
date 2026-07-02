# Optionals

_Values that may be present or absent — the none literal and the ? type suffix._

An **optional** is a value that may be present or absent. The literal `none` represents absence; a `?` suffix on a type makes its values optional. Together they let a field be omitted, a function return no answer, or a slot be cleared explicitly.

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

`none` is the value that fills an absent optional. It is also the result of builtins that do not return a useful value (`error`, `panic`, `assert`) — those have type `none` and cannot meaningfully be used in a value position.

```wcl
note: utf8? = none
```

## Working with optionals

Pattern-match an optional to handle the two cases. The `if let` shorthand is convenient when you only care about the present case. See [match](../references/concept_match_expr.md) and [if let](../references/concept_if_let.md).

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

## Defaults with ??

The `??` operator picks the left value unless it is `none`, in which case it returns the right side — the concise way to give an optional a default. It chains left-to-right and binds looser than every other operator, and the right side only evaluates when needed.

```wcl
width  = box.width ?? 480.0
theme  = page.theme ?? site.theme ?? :nord
label  = trim(raw_label) ?? "untitled"   // (trim(raw_label)) ?? "untitled"
```

> [!NOTE]
> **Optionals vs unions**
> Use T? when the only states are present and absent. When absence carries information (a reason, a fallback, multiple shapes), reach for a union instead.

## Examples

### Optional fields and the ?? default

A `?` suffix makes a field optional; `??` supplies a fallback when it is `none`.

```wcl
type Profile {
  name: utf8       // required
  bio:  utf8?      // optional — may be none
}

partial = Profile { name: "Bob", bio: none }
theme   = page.theme ?? site.theme ?? :nord
```

**Expected:** `partial.bio` is `none`; `theme` falls back to `site.theme`, then `:nord`.

## Related

- [Unions](../references/concept_unions.md)

- [match](../references/concept_match_expr.md)

- [if let](../references/concept_if_let.md)

[← Back to SKILL.md](../SKILL.md)
