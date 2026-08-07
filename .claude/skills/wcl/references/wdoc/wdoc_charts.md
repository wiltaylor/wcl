# Charts

wdoc ships three chart kinds: `bar_chart`, `line_chart` and `pie_chart`. Each is a **diagram
shape**. Put it inside a `diagram { }` and give it the size of that diagram.

```wcl
diagram { width = 360  height = 200
  bar_chart { width = 360.0  height = 200.0
    title = "Revenue"  x_label = "Quarter"  y_label = "$k"
    categories = ["Q1", "Q2", "Q3", "Q4"]
    series = [
      { name: "2025", values: [42.0, 55.0, 61.0, 78.0] },
      { name: "2026", values: [30.0, 48.0, 52.0, 66.0] },
    ]
  }
}
```

The charts are pure WCL. Each computes its geometry with the math builtins and lowers to
primitive shapes. They therefore render on every backend, and the placement of every bar is
readable in the stdlib source.

**Size them twice.** The diagram's `width` / `height` are `i64`; the chart's are `f64` with a
decimal point. Give the chart the same numbers as its diagram, or it will not fill the canvas.

## The data shape

A bar or line chart takes `series`, a list of `{ name, values }` records:

```wcl
series = [
  { name: "p50", values: [12.0, 14.0, 11.0] },
  { name: "p99", values: [40.0, 44.0, 39.0] },
]
```

A pie chart takes `slices`, a list of `{ label, value }` records:

```wcl
slices = [
  { label: "Alpha", value: 42.0 },
  { label: "Beta",  value: 31.0 },
]
```

A bare record coerces to the right union variant by shape, so `ChartSeries::Of { … }` is legal
but never necessary. Values are `f64`, and an integer literal also works there:
`values: [42, 55]` is the same as `[42.0, 55.0]`.

Give at least one series. With `series = []` and no `categories`, the chart has no slot count to
read and the build fails with `'at': at: index 0 out of bounds`.

## Shared fields

`bar_chart` and `line_chart` take the same fields. `line_chart` adds two more.

| Field | Default | Meaning |
| --- | --- | --- |
| `x` / `y` | `0.0` | Position within the enclosing diagram. |
| `width` / `height` | `360.0` / `220.0` | Chart box size. |
| `title` | — | Centred title at the top of the box. |
| `x_label` / `y_label` | — | Axis titles. |
| `categories` | — | One x-axis label per value slot. |
| `y_min` | `0.0` | Lower scale bound. |
| `y_max` | data maximum | Upper scale bound. |
| `series` | required | The data. |
| `id` / `class` | — | HTML id and style classes. |
| `connect_points` | all sides | Edge-attach sides, like any shape. |
| `point_labels` | `false` | *line only* — print each value above its marker. |
| `points` | — | *line only* — author-placed annotation markers. |

`pie_chart` takes `x`, `y`, `width` (`240.0`), `height` (`240.0`), `title`, `slices`, `id`,
`class` and `connect_points`. It has no axes and therefore none of the axis fields.

## What the frame draws

- **The scale.** `y_min` defaults to 0 and `y_max` to the largest value across every series. The
  upper bound is always nudged above the lower, so a flat series still spans a visible range.
- **Y ticks.** Four divisions, each a gridline plus a value label, rounded to two decimals.
- **Category labels.** One under each slot. With `categories` omitted, the slots take the numbers
  `1`, `2`, `3`, …, and their count comes from the first series.
- **A legend.** Drawn **only when there is more than one series**, as a swatch and the series
  `name` above the plot. A single-series chart shows no legend, so its `name` appears nowhere —
  put the description in `title`.
- **Axis titles.** `x_label` under the category labels, `y_label` above the y spine.

## Line chart annotations

```wcl
diagram { width = 360  height = 200
  line_chart { width = 360.0  height = 200.0
    title = "Latency (ms)"  x_label = "Day"
    categories = ["Mon", "Tue", "Wed", "Thu", "Fri"]
    point_labels = true
    series = [ { name: "p50", values: [12.0, 14.0, 11.0, 18.0, 13.0] } ]
    points = [ { label: "spike", category: 3, value: 18.0 } ]
  }
}
```

`point_labels = true` prints every data point's value above its marker.

`points` is a list of `{ label, category, value }` records placed by hand:

- `category` is the **0-based index** of the x slot, and it is an `i64` — write `3`, not `3.0`.
- `value` is the y position in data units. It is independent of any series, so an annotation can
  sit anywhere on the scale.

Each one draws a marker plus its label, in the `wdoc-annotation` class.

## Pie chart

```wcl
diagram { width = 240  height = 240
  pie_chart { width = 240.0  height = 240.0
    title = "Market share"
    slices = [
      { label: "Alpha", value: 42.0 },
      { label: "Beta",  value: 31.0 },
      { label: "Other", value: 27.0 },
    ]
  }
}
```

The chart draws the slices clockwise from the top, in declaration order, as 48-segment polygon
arcs — there is no arc primitive. Each slice's share is its `value` over the total, so the
values need no normalising. Raw counts work.

The slice label sits inside the slice. There is no legend and no percentage; write the number
into the label if a reader needs it.

## Colour

Every bar, line and slice carries a palette class — `wdoc-series-1` to `wdoc-series-8`, cycled
by index — and emits **no inline fill**. Colour therefore comes entirely from CSS, and you
recolour a series by redeclaring the class:

```wcl
class "wdoc-series-1" {
  fill   = "#bf616a"
  stroke = "#bf616a"
  dark  { fill = "#d08770" }
}
```

The frame — axes, gridlines, tick labels, title, legend — paints with `currentColor` and follows
the surrounding text. Its parts carry their own classes: `wdoc-axis`, `wdoc-grid`,
`wdoc-axis-label`, `wdoc-chart-title`, `wdoc-legend`, `wdoc-line`, `wdoc-point-label`,
`wdoc-annotation`.

The ninth series reuses `wdoc-series-1`. Beyond eight series, the palette repeats.

## Data-driven charts

`series`, `slices` and `categories` are ordinary list fields, so compute them:

```wcl
line_chart {
  width = 480.0  height = 240.0
  title = "Requests per day"
  categories = map(days, fn(d: Day) -> utf8 d.label)
  series = [ { name: "total", values: map(days, fn(d: Day) -> f64 d.count) } ]
}
```

## Gotchas

- The chart's `width` / `height` are `f64`; the enclosing diagram's are `i64`. Set both.
- **More categories than values is a build error**: `'at': at: index 2 out of bounds`. The plot
  walks one value per category. Fewer categories than values silently plots only the first few.
- Every series should hold the same number of values. Nothing checks that for you.
- `category` on an annotation point is a 0-based `i64` index into the categories, not an x value.
- A single-series chart draws no legend, so its `name` never appears.
- A chart is a shape, not a page block. It needs an enclosing `diagram`.
