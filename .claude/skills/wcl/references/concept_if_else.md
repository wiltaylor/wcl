# if / else

_An if is an expression; branches must agree on a type, else if chains for multi-way branches, and an omitted else yields none._

An `if` is an expression. The branches must agree on a type. `else if` chains for multi-way
branches.


```wcl
sign = if x < 0 { :neg } else if x > 0 { :pos } else { :zero }
```

## The else branch is optional

Omit `else` and the untaken branch evaluates to `none` — so an else-less `if` has type `T?`.
This is the concise form for a conditional list element or an optional field, where writing
`else { none }` at every site only adds noise. A chain may end without an `else` too; if no
branch is taken the whole expression is `none`.


```wcl
// Conditional list element — `none` when the condition is false.
// A `none` element is legal in a `list<utf8>`; consumers drop it.
class = ["nav-item", if entry.current { "current" }]

// A chain with no final else.
badge = if count == 0 { "empty" } else if count > 99 { "many" }

// An optional field, without the `else { none }` ceremony.
subtitle = if page.tagline != "" { page.tagline }
```

The `none` an untaken branch leaves behind is a _value_, so it has to land somewhere that
accepts absence: a field declared optional (`subtitle: utf8?`), or any list element. A
**required** field still rejects it — assigning an else-less `if` to `title: utf8` is a schema
violation whenever the condition is false. A `@non_empty` list needs at least one element that
is _not_ `none`, since that is what its readers will see.


> [!NOTE]
> **if let still needs an else**
>
> Only the plain `if` may omit its `else`. An `if let` still requires one — the shorthand is not supported there yet.

## Related

- [if let](../references/concept_if_let.md)

[← Back to SKILL.md](../SKILL.md)
