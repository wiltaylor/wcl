# Columns

`column` lays wdoc content out in side-by-side columns on a page, instead of stacking it sequentially. The `widths` list gives a CSS percentage for each child slot — one entry per child, summing to about `100`.

A two-column split, previewed inside a card:

![diagram](../_wdoc/wdoc_columns-diagram-1.svg)

```wcl
column {
  widths = [50.0, 50.0]
  p "**Left.** The first child fills the first 50% slot."
  p "**Right.** The second child fills the other half."
}
```

Any number of columns — here three equal thirds, each holding its own content:

![diagram](../_wdoc/wdoc_columns-diagram-2.svg)

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

## Fields

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `widths` | `list<f64>` | yes | One CSS percentage per child slot (e.g. `[50.0, 50.0]`). |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Optional class list applied to the whole column group. |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `children` | `WdocBlock` | yes | Any wdoc blocks — each consecutive block fills the next column slot. |
