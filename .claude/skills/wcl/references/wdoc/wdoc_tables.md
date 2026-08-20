# Lists and tables

`list` / `li` and `table` are the two structural content blocks. Both are **`@native`**: they
have no `lower` and are rendered directly in Rust on every target, because valid nested list
HTML and a two-form cell grid are not expressible in WCL. So there is no lowering to read and
no `Content` payload to intercept — what the fields say is what each backend draws.

## `list` and `li`

```wcl
list {
  li "Plain item"
  li "With **bold**, `code` and a [link](wdoc_sites)"
}

list {
  style = :numbered
  li "First step"
  li "Second step"
}
```

`list` fields: `style` (`:bullet`, the default, or `:numbered`), `id`, `class`, and
`@children("li") items`.
`li` fields: `text` (the label slot, patterned), `id`, `class`, and
`@children(ListNode) children`.

`li` text runs through the inline-pattern engine, so `**bold**`, `_italic_`, `` `code` ``,
`[links](page)`, `:icons:` and `$math$` all work inside an item.

### Nesting

An `li`'s children are either more `li`s — a sublist in the **parent's** style — or a whole
`list` block, which brings its own style:

```wcl
list {
  style = :numbered
  li "Setup"
  li "Build" {
    li "Compile"          // 2.1
    li "Link"             // 2.2
  }
  li "Run" {
    list {                // a bulleted sublist inside a numbered list
      li "Foreground"
      li "Background"
    }
  }
}
```

The hierarchical `2.1` / `2.2` numbering is **pure CSS** — nested `ol.wdoc-list-numbered`
counters joined with `counters(…, ".")`. There is no per-item state in Rust, and no field to
set a start number, restart a counter or change the marker glyph. Markdown and PDF render
plain nested ordered lists, so the compound numbering is an HTML-only reading.

There is no "definition list", no checklist and no `li` continuation field. An item is one
line of patterned text plus sub-items.

## `table`

Two authoring forms, one block.

**Pipe rows** — WCL's own table syntax under a `rows:` header. The first row is the header:

```wcl
table {
  rows:
    | "Name"  | "Role"     | "Years" |
    | "Alice" | "**Dev**"  | 3       |
    | "Bob"   | "_Ops_"    | 5       |
}
```

**Computed rows** — a `rows` *field* holding a list of cell-lists, plus an optional `header`
list. This is the data-driven form, and the one a component slot or a repeater feeds:

```wcl
table {
  header = ["Name", "Role"]
  rows   = map(people, fn(p: Person) -> list<utf8> [p.name, p.role])
}
```

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `header` | `list<utf8>?` | no | Header row for the computed form. Omit for a header-less table. |
| `rows` | `list<list<utf8>>?` | no | The computed body rows. `@schemaless`, so any scalar is legal in a cell. |
| `id` | `identifier?` | no | Explicit HTML id. |
| `class` | `list<utf8>?` | no | Classes on the `<table>`, added to `wdoc-table`. |

**The two forms do not mix.** When a `rows` *field* is present the computed path wins and any
pipe rows in the same block are ignored silently. Pick one per block.

The type is named `TableBlock` rather than `Table` because `Table` is the reserved type name
for the language's `@table` decorator. The *block kind* is still `table` — the rename never
reaches what you write.

### Cells

A `utf8` cell runs through the inline-pattern engine — bold, italic, `code`, links, icons and
math all work. Any other scalar (number, bool, symbol) is stringified. A row that is not a
list degrades to a single cell rather than vanishing.

A table with no rows and no header renders as **nothing at all** — an empty string, not an
empty grid. A computed `rows` that evaluates to an empty list therefore makes the whole table
disappear; guard it with a `wdoc_repeater` if you want a fallback.

Gotcha: a literal `|` outside a quoted cell splits the row. Wrap any cell containing a pipe in
a string literal.

### What `table` does not have

- **No alignment field.** Column alignment is CSS: put a `class` on the table and declare a
  rule against it (see [`wdoc_styling.md`](wdoc_styling.md)).
- **No caption field.** Put a `p` above or below the table.
- No column widths, no spans, no sorting, no per-cell classes, no footer row.

### Styling

`table.wdoc-table` ships a default border, radius and header treatment, injected into every
page and declared before user rules, so a `class` on the block cascades over it. A
`wdoc-badge` class is available for pill-shaped cell contents.

## Related

- Schema-reflected tables — `type_table { type = X }` generates a property table straight from
  a type declaration, so the docs cannot drift from the schema. See
  [`wdoc_data_views.md`](wdoc_data_views.md).
- The inline patterns cells and items run through: [`wdoc_formatting.md`](wdoc_formatting.md).
- A `node_table` is a *diagram* shape (a DB/class box with per-row ports), not this block. See
  [`wdoc_tree.md`](wdoc_tree.md).
