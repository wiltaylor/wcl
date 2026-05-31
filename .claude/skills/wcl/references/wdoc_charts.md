# Charts

wdoc ships three chart kinds — `bar_chart`, `line_chart`, and `pie_chart` — that lower to SVG via pure-WCL geometry. Each is an `SvgBlock`, so place it inside a `diagram` sharing its size. Data is supplied as a list of records; the variant is inferred from each record's shape, so a bare `{ … }` is all you need.

## Bar chart

Both take a `series: list<ChartSeries>` — each series is a `{ name, values }` record. Multiple series produce grouped bars or multi-line plots; `categories` labels the x-axis. (You can still write the explicit `ChartSeries::Of { … }` form to disambiguate.)

![diagram](../_wdoc/wdoc_charts-diagram-1.svg)

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

`bar_chart` and `line_chart` share most fields; `line_chart` adds `point_labels` and `points`. The `line_chart` fields (the superset) are listed below — `bar_chart` is identical minus those two.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | `f64` | no | Chart x position within the enclosing `diagram`. |
| `y` | `f64` | no | Chart y position within the enclosing `diagram`. |
| `width` | `f64` | no | Chart width — match the enclosing `diagram`. |
| `height` | `f64` | no | Chart height — match the enclosing `diagram`. |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Optional style classes. |
| `title` | `utf8` | no | Chart title. |
| `x_label` | `utf8` | no | x-axis label. |
| `y_label` | `utf8` | no | y-axis label. |
| `categories` | `list<utf8>` | no | x-axis labels (one per value). |
| `y_min` | `f64` | no | Lower scale bound; auto-fits to the data (0) when omitted. |
| `y_max` | `f64` | no | Upper scale bound; auto-fits to the data when omitted. |
| `series` | `list<ChartSeries>` | yes | Series data — each a `{ name: utf8, values: list<f64> }` record. |
| `point_labels` | `bool` | no | Print each point's value above its marker. |
| `points` | `list<ChartPoint>` | no | Annotation markers — `list<ChartPoint>`, each `{ label, category, value }`. |
| `connect_points` | `list<AnchorSide>` | no | Diagram edge-attach sides, like any shape. |

## Line chart annotations

On a `line_chart`, set `point_labels = true` to print every data point's value above its marker. For author-named callouts, pass `points: list<ChartPoint>` — each a `{ label, category, value }` record where `category` is the 0-based x slot and `value` is the y in data units.

![diagram](../_wdoc/wdoc_charts-diagram-2.svg)

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

## Pie chart

A `pie_chart` takes `slices: list<ChartSlice>` — each a `{ label, value }` record. Slices are drawn as polygon-approximated arcs.

![diagram](../_wdoc/wdoc_charts-diagram-3.svg)

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

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | `f64` | no | Chart x position within the enclosing `diagram`. |
| `y` | `f64` | no | Chart y position within the enclosing `diagram`. |
| `width` | `f64` | no | Chart width — match the enclosing `diagram`. |
| `height` | `f64` | no | Chart height — match the enclosing `diagram`. |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Optional style classes. |
| `title` | `utf8` | no | Chart title. |
| `slices` | `list<ChartSlice>` | yes | Slice data — each a `{ label: utf8, value: f64 }` record. |
| `connect_points` | `list<AnchorSide>` | no | Diagram edge-attach sides, like any shape. |
