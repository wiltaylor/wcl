# table

A `table { rows: | … | }` renders a tabular data grid using WCL's native pipe-row syntax. The first row becomes the `<thead>`; the remaining rows go in `<tbody>`. (In a wskill data block the pipe-row form is the page-authoring shape; wskill's own reference tables use a `header` / `rows` field pair.)

```wcl
table {
  rows:
    | "Name" | "Role" | "Years" |
    | "Alice" | "**Dev**" | 3 |
    | "Bob" | "_Ops_" | 5 |
}
```

| Name | Role | Years |
| --- | --- | --- |
| Alice | **Dev** | 3 |
| Bob | _Ops_ | 5 |

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Optional class list applied to the `<table>` (e.g. to override `wdoc-table`). |
| `header` | `list<utf8>` | no | Optional header row for the computed-table form (omit for a header-less table). |
| `rows` | `list<list<utf8>>` | no | The pipe-row data (or computed rows) — with pipe syntax the first row is the header, the rest are body rows. |

## Cells

Cells are expressions in the row schema's field positions. utf8 cells run through the inline pattern engine, so **bold**, italic, `inline code`, links, icons, and math all work inside cells. Numeric, boolean, and symbol cells stringify. See [formatting](../references/concept_formatting.md).

> [!WARNING]
> **Pipes in cells**
> A literal pipe outside a quoted cell splits the row, so wrap any cell that contains a pipe in a string literal.

## Related

- [list / li](../references/fact_lists_block.md)

- [callout](../references/fact_callouts.md)

[← Back to SKILL.md](../SKILL.md)
