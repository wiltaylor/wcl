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

## Why a table

A table is just a `list<T>` underneath — the pipe-row syntax is shorthand for writing many [records](../references/concept_records.md) compactly. The same data could be authored as block instances, but the table is denser and reads like a spreadsheet.

> [!NOTE]
> **Cells are expressions**
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

- [Lists](../references/concept_lists.md)

- [Records](../references/concept_records.md)

[← All concepts](../references/concepts_ref.md)
