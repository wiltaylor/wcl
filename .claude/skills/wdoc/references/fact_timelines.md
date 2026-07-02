# timeline

`timeline` is a **real-time axis** — dates are ISO strings (`"2026-03-15"` / `"…14:30"`) and tick boundaries land on real calendar dates. A `unit` (`:minutes`..`:years`) sets granularity; omit it for auto-fit from the event span. It is a diagram shape, so it lives inside a `diagram` sharing its size.

A real-time axis placed inside a `diagram`; dates are ISO strings, a `unit` sets granularity, and it holds point `items`, labelled `phases`, and rich event `card` children.

```wcl
diagram {
  width = 560
  height = 220
  timeline {
    width = 560.0
    height = 220.0
    title = "2026 roadmap"
    start = "2026-01-01"
    end = "2026-12-31"
    unit = :months
    items = [{ label: "Kickoff", on: "2026-02-20" }, { label: "Beta", on: "2026-05-10" }, { label: "Release", on: "2026-09-20", side: :far }]
    phases = [{ label: "Build", from: "2026-02-01", to: "2026-06-15" }, { label: "Polish", from: "2026-06-15", to: "2026-11-01" }]
  }
}
```

![diagram](../_wdoc/fact_timelines-diagram-1.svg)

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | `f64` | no | Timeline x position within the enclosing `diagram`. |
| `y` | `f64` | no | Timeline y position within the enclosing `diagram`. |
| `width` | `f64` | no | Timeline width — match the enclosing `diagram`. |
| `height` | `f64` | no | Timeline height — match the enclosing `diagram`. |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Optional style classes. |
| `title` | `utf8` | no | Timeline title. |
| `direction` | `symbol` | no | Axis direction: `:horizontal` (default) or `:vertical`. |
| `unit` | `symbol` | no | Tick granularity: `:minutes` / `:hours` / `:days` / `:weeks` / `:months` / `:years` (auto from the span when omitted). |
| `start` | `utf8` | no | ISO date scale start (auto-fits from the items when omitted). |
| `end` | `utf8` | no | ISO date scale end (auto-fits from the items when omitted). |
| `every` | `i64` | no | Override the tick interval in `unit`s (auto ~6–12 ticks when omitted). |
| `phases` | `list<TimelinePhase>` | no | Dated bands — `list<TimelinePhase>`, each `{ label, from, to }`. |
| `items` | `list<TimelineItem>` | no | Dated point events — `list<TimelineItem>`, each `{ label, on }` (auto-alternates side; add `side: :near\|:far` to pin one). |
| `connect_points` | `list<AnchorSide>` | no | Diagram edge-attach sides, like any shape. |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `cards` | `card` | yes | Rich-text event cards, each pinned to a date via `on`. |

## Items and phases

`items` are point events, each `{ label, on }` (with optional `side: :near|:far`). `phases` are labelled spans, each `{ label, from, to }`, drawn as a band that cycles the `wdoc-series-N` palette. Set `direction = :vertical` to read top-to-bottom with items alternating sides.

## Event cards

A timeline also accepts rich-text `card` children — each pinned to a date with `on` and filled with formatted wdoc content (text, lists, callouts, images). `width` / `height` size the box and `side: :near|:far` forces which side of the axis it sits on.

```wcl
diagram {
  width = 720
  height = 320
  timeline {
    width = 720.0
    height = 320.0
    title = "Release timeline"
    unit = :months
    start = "2026-01-01"
    end = "2026-12-31"
    card {
      on = "2026-02-01"
      title = "1.0"
      width = 210.0
      height = 104.0
      text {
        span "First "
        span "stable" {
          class = ["accent"]
        }
        span " release. APIs frozen."
      }
    }
  }
}
```

![diagram](../_wdoc/fact_timelines-diagram-2.svg)

## Related

- [charts](../references/fact_charts.md)

- [diagram](../references/fact_diagrams.md)

[← Back to SKILL.md](../SKILL.md)
