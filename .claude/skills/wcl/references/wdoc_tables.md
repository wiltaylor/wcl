# Tables

`table { rows: | … | }` renders a tabular data grid using WCL's native pipe-row syntax. The first row becomes the `<thead>`; the remaining rows go in `<tbody>`.

## Authoring

A table previewed inside a card — note the inline-formatted cells:

![diagram](../_wdoc/wdoc_tables-diagram-1.svg)

```wcl
table {
  rows:
    | "Name"  | "Role"     | "Years" |
    | "Alice" | "**Dev**"  | 3       |
    | "Bob"   | "_Ops_"    | 5       |
}
```

## Cells

Cells are expressions in the row schema's field positions. utf8 cells run through the inline pattern engine, so bold, italic, inline code, links, icons, and math all work inside cells. Numeric / boolean / symbol cells stringify.

> [!WARNING]
> **Pipes in cells**
> A literal pipe (|) outside a quoted cell will split the row. Wrap any cell that contains pipes — e.g. an inline `code` example showing ||  — in a string literal.

## Fields


