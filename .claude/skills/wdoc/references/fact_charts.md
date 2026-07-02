# charts

wdoc ships three chart kinds — `bar_chart`, `line_chart`, and `pie_chart` — that lower to SVG via pure-WCL geometry. Each is an `SvgBlock`, so place it inside a `diagram` sharing its size. Data is a list of records and the variant is inferred from each record's shape, so a bare `{ … }` is all you need.

## Bar and line charts

Both take a `series: list<ChartSeries>` — each series a `{ name, values }` record; multiple series produce grouped bars or multi-line plots, and `categories` labels the x-axis. `line_chart` adds `point_labels = true` (print every point's value) and `points` (author-named callouts, each `{ label, category, value }`).

A grouped `bar_chart` — two series over four quarters:

```wcl
diagram {
  width = 360
  height = 200
  bar_chart {
    width = 360.0
    height = 200.0
    title = "Revenue"
    x_label = "Quarter"
    y_label = "$k"
    categories = ["Q1", "Q2", "Q3", "Q4"]
    series = [{ name: "2025", values: [42.0, 55.0, 61.0, 78.0] }, { name: "2026", values: [30.0, 48.0, 52.0, 66.0] }]
  }
}
```

![diagram](../_wdoc/fact_charts-diagram-1.svg)

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
| `connect_points` | `list<AnchorSide>` | no | Diagram edge-attach sides, like any shape. |

A multi-line plot over a `series: list<ChartSeries>`; adds `point_labels` (print every point's value) and author-named `points` callouts (`{ label, category, value }`).

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

A `line_chart` with two series, every point labelled, and one author-named callout:

```wcl
diagram {
  width = 380
  height = 220
  line_chart {
    width = 380.0
    height = 220.0
    title = "Latency (ms)"
    x_label = "Day"
    categories = ["Mon", "Tue", "Wed", "Thu", "Fri"]
    point_labels = true
    series = [{ name: "p50", values: [12.0, 14.0, 11.0, 18.0, 13.0] }, { name: "p99", values: [28.0, 31.0, 26.0, 44.0, 30.0] }]
    points = [{ label: "spike", category: 3, value: 44.0 }]
  }
}
```

![diagram](../_wdoc/fact_charts-diagram-2.svg)

## Pie chart

A `pie_chart` takes `slices: list<ChartSlice>` — each a `{ label, value }` record, drawn as polygon-approximated arcs.

A pie chart over `slices: list<ChartSlice>`, each a `{ label, value }` record drawn as polygon-approximated arcs.

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

```wcl
diagram {
  width = 240
  height = 240
  pie_chart {
    width = 240.0
    height = 240.0
    title = "Market share"
    slices = [{ label: "Alpha", value: 42.0 }, { label: "Beta", value: 31.0 }, { label: "Other", value: 27.0 }]
  }
}
```

![diagram](../_wdoc/fact_charts-diagram-3.svg)

Charts cycle the `wdoc-series-1`..`wdoc-series-8` palette classes, so a `class` override or a site `theme` recolours them. See [styling](../references/concept_styling.md).

## Examples

### A grouped bar chart

A bar_chart is an SvgBlock, so it sits inside a diagram of the same size. Each series is a { name, values } record; multiple series produce grouped bars.

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

**Expected:** A grouped bar chart with two series across four quarters, axis labels, and a title.

## Related

- [diagram](../references/fact_diagrams.md)

- [timeline](../references/fact_timelines.md)

[← Back to SKILL.md](../SKILL.md)
