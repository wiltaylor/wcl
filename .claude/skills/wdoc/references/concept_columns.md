# Columns

_The `column` layout block and its content (any wdoc block, per-slot widths)._

`column` lays wdoc content out in side-by-side columns on a page, instead of stacking it sequentially. The `widths` list gives a CSS percentage for each child slot — one entry per child, summing to about `100`.


```wcl
column {
  widths = [50.0, 50.0]
  p "**Left.** The first child fills the first 50% slot."
  p "**Right.** The second child fills the other half."
}
```

Any number of columns is allowed — here three equal thirds, each holding its own content.


```wcl
column {
  widths = [33.3, 33.3, 33.3]
  h4 "One"
  h4 "Two"
  h4 "Three"
}
```

## Children

Columns can hold any wdoc content — paragraphs, headings, callouts, code blocks, diagrams, charts, even other columns. Apply a shared `class` to the column itself to style the group.


> [!NOTE]
> **Page columns vs diagram columns**
> On a page, a `column` holds any block. Inside a wdoc `column` cell used as a book-step gutter, only paragraph / heading content fits — code, lists, callouts and `project` can't sit in such a cell.

## Block reference

A `column` block: side-by-side layout of its child blocks, one `widths` percentage per child slot.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `widths` | `list<f64>` | yes | One CSS percentage per child slot (e.g. `[50.0, 50.0]`). |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Optional class list applied to the whole column group. |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `children` | `WdocBlock` | yes | Any wdoc blocks — each consecutive block fills the next column slot. |

## Related

- [Formatting](../references/concept_formatting.md)

[← Back to SKILL.md](../SKILL.md)
