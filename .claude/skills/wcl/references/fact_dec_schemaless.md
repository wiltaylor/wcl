# @schemaless

`@schemaless` switches schema validation off. On a **type** it short-circuits every check for
instances of that block: unknown fields, disallowed nested kinds, child quotas, table-row
arity. On a **field** it exempts only that field from the value-versus-type check, leaving the
rest of the block validated.


Reach for it when a slot legitimately holds a value the declared type can't describe — a
computed reference, or cells whose scalar type is decided at render time. It is an escape
hatch, not a convenience: a `@schemaless` type gets no help from `wcl check`, so every mistake
in it surfaces later, at consume time.


```wcl
// Whole block: `from` resolves to a body reference, not a static scalar.
@block("project") @schemaless
type Project {
  from: utf8
}

// Single field: rows may hold any scalar, but the rest of the block
// is still checked.
@block("table") type Table {
  header: list<utf8>?
  @schemaless rows: list<list<utf8>>?
}
```

[← Back to SKILL.md](../SKILL.md)
