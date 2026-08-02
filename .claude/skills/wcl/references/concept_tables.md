# Tables

_Pipe-row syntax for writing many records of the same shape compactly._

Repeated rows of the same shape can be written with pipe-table syntax under a field name. Each `| ... |` row becomes one record.

## Authoring

The field's row type carries a `@table("kind")` decorator. The field declaration uses `name:` (colon, not `=`) and lists rows directly underneath.

```wcl
@table("user")
type User {
  name:    utf8
  age:     u32
  enabled: bool
}

@document
type Config {
  users: list<User>
}

users:
  | "alice" | 30 | true  |
  | "bob"   | 25 | false |
  | "cara"  | 42 | true  |
```

> [!NOTE]
> **Cells are expressions**
>
> Each cell is an expression in the row type's field position: strings, numbers, booleans, symbols, even computed values via let bindings.

## Examples

### Pipe-table rows

A `@table` row type plus a colon-introduced field lets rows be written like a spreadsheet.

```wcl
@table("user")
type User {
  name:    utf8
  age:     u32
  enabled: bool
}

users:
  | "alice" | 30 | true  |
  | "bob"   | 25 | false |
  | "cara"  | 42 | true  |
```

**Expected:** Each `| … |` row becomes one `User` record in `users: list<User>`.

## Related

- [Lists](../references/concept_lists.md) — Lists supports Tables: Ordered, homogeneous sequences — list<T> — and the collection builtins over them.

- [Records](../references/concept_records.md) — Records supports Tables: Named records via the type keyword — fixed sets of named, typed fields.

[← Back to SKILL.md](../SKILL.md)
