# Timelines and dopesheets

Two unrelated blocks that share a chapter because both are time-shaped. A `timeline` draws a
real calendar axis. A `dopesheet` plays a range of frames out of a sprite sheet.

Both are **diagram shapes**. Put each inside a `diagram { }` and place it with `x` / `y` or
anchors. Both are native blocks: Rust draws the calendar arithmetic and the image cropping,
because WCL cannot express either.

## timeline

```wcl
diagram { width = 560  height = 220
  timeline { width = 560.0  height = 220.0
    title = "2026 roadmap"
    start = "2026-01-01"
    end   = "2026-12-31"
    unit  = :months
    items = [
      { label: "Kickoff", on: "2026-02-20" },
      { label: "Beta",    on: "2026-05-10" },
      { label: "Release", on: "2026-09-20", side: :far },
    ]
    phases = [
      { label: "Build",  from: "2026-02-01", to: "2026-06-15" },
      { label: "Polish", from: "2026-06-15", to: "2026-11-01" },
    ]
  }
}
```

A timeline tracks **real calendar time**. Ticks land on calendar boundaries — month firsts,
Mondays, midnights — not on evenly divided fractions of the span. The renderer handles month
lengths and leap years.

### Fields

| Field | Default | Meaning |
| --- | --- | --- |
| `x` / `y` | `0.0` | Position within the enclosing diagram. |
| `width` / `height` | `480.0` / `200.0` | Box size. Match the diagram. |
| `title` | — | Title drawn with the axis. |
| `direction` | `:horizontal` | `:vertical` reads top to bottom. |
| `unit` | auto | `:minutes` / `:hours` / `:days` / `:weeks` / `:months` / `:years`. |
| `start` / `end` | auto-fit | ISO scale bounds. |
| `every` | auto | Tick interval, counted in `unit`s. An `i64`. |
| `items` | — | Dated point events. |
| `phases` | — | Named date spans. |
| `cards` | — | `card` child blocks pinned to a date. |
| `id` / `class` / `connect_points` | — | As for any shape. |

### Dates

Dates are ISO strings, and the time part is optional:

```
"2026-03-15"          "2026-03-15 14:30"          "2026-03-15T14:30"
```

Omit `start` and `end`, and the scale auto-fits to the events you gave it.

Omit `unit`, and the span picks one:

| Span | Unit |
| --- | --- |
| over 2 years | `:years` |
| over 75 days | `:months` |
| over 21 days | `:weeks` |
| over 2 days | `:days` |
| over 2 hours | `:hours` |
| shorter | `:minutes` |

Omit `every`, and the interval comes from a per-unit ladder. The axis then carries roughly six
to twelve ticks.

### items — dated points

Each item is a `{ label, on }` record, drawn as a marker, a lead line and a text label.

```wcl
items = [
  { label: "Kickoff", on: "2026-02-20" },
  { label: "Release", on: "2026-09-20", side: :far },
]
```

Items **auto-alternate** sides of the axis, so their labels do not collide. Add `side: :near` or
`side: :far` to pin one. The record with a `side` is a second variant of the same union. The
shape of the record picks the variant, so you name neither.

### phases — named spans

```wcl
phases = [
  { label: "Discovery", from: "2026-01-01", to: "2026-03-01" },
  { label: "Build",     from: "2026-03-01", to: "2026-10-01" },
]
```

A phase is a `{ label, from, to }` record drawn as a divider at each boundary with a heading
naming the band. Phases group the point items into stages. Each phase cycles the `wdoc-series-N`
palette, so adjacent bands differ. Redeclare `class "wdoc-series-1" { … }` to recolour them, the
same way you recolour a chart series.

### cards — rich event boxes

A timeline also accepts `card` child blocks. A card holds formatted wdoc content — paragraphs,
lists, callouts — and pins to a date.

```wcl
diagram { width = 720  height = 320
  timeline { width = 720.0  height = 320.0
    title = "Release timeline"
    unit  = :months
    start = "2026-01-01"
    end   = "2026-12-31"
    card { on = "2026-02-01"  title = "1.0"  width = 210.0  height = 104.0
      p "First **stable** release. APIs frozen."
    }
    card { on = "2026-05-15"  side = :far  title = "1.4"  width = 210.0  height = 104.0
      p "Plugin system and the wdoc generator."
    }
  }
}
```

`on` pins the card to a date, `side` forces which side of the axis it sits on, and
`width` / `height` size the box. [`wdoc_tree.md`](wdoc_tree.md) describes the `card` block
itself. `on` and `side` are the two fields a card uses only as a timeline child.

### Vertical

```wcl
diagram { width = 300  height = 320
  timeline { width = 300.0  height = 320.0
    direction = :vertical
    title     = "Release history"
    unit      = :months
    items = [
      { label: "1.0 alpha", on: "2025-10-15" },
      { label: "1.0",       on: "2026-01-10" },
      { label: "2.0 beta",  on: "2026-05-01" },
    ]
  }
}
```

The axis runs top to bottom and the items alternate left and right of it. Everything else is
unchanged.

## dopesheet

```wcl
diagram { width = 96  height = 96
  dopesheet "assets/pixel-coin.png" {
    frame_width  = 12
    frame_height = 12
    scale        = 6.0
    fps          = 12.0
    x            = 12.0
    y            = 12.0
  }
}
```

A `dopesheet` windows over a sprite sheet and animates the window in the browser. Name the sheet
as the inline label, describe its frame grid, and pick a speed.

### Frame geometry

| Field | Default | Meaning |
| --- | --- | --- |
| inline label | required | Sheet image path, relative to the build entry file. |
| `frame_width` / `frame_height` | required | Size of one frame, in source pixels. |
| `offset_x` / `offset_y` | `0` | Pixels to the first frame's left and top edge. |
| `stride_x` / `stride_y` | frame size | Origin-to-origin step between frames. |
| `columns` | fits the sheet width | Frames per sheet row. |

Set `offset_*` and `stride_*` for a sheet with padding or gutters. For a flush-packed strip,
the defaults are right.

**These are all `i64`.** Write `frame_width = 12`, never `12.0`.

### Playback

| Field | Default | Meaning |
| --- | --- | --- |
| `from` / `to` | whole sheet | Inclusive frame-index range to play. |
| `fps` | `12.0` | Playback rate, an `f64`. |
| `loop` | `true` | Restart at the end. |
| `autoplay` | `true` | Start playing on load. |
| `controls` | `true` | Click to toggle play and pause. |
| `smooth` | `false` | Anti-alias instead of the default pixelated scaling. |
| `scale` | `1.0` | Display scale. |

Sprite art scales with `image-rendering: pixelated` by default, which is what keeps retro art
crisp. Set `smooth = true` for a non-pixel source. With `loop = false`, playback stops on the
last frame and the overlay glyph becomes a replay arrow.

Playback is a browser behaviour. A PDF or Markdown build renders the frame the sheet starts on
and nothing moves.

The build copies the sheet into `_wdoc/` and refers to it by URL. The frames therefore resolve
when the output is served, not when a page is opened directly from disk.

## Gotchas

- Both blocks are shapes. Each needs an enclosing `diagram`. Give each the same size as that
  diagram.
- The block sizes are `f64` (`width = 560.0`); the diagram's are `i64` (`width = 560`).
- `every` on a timeline and the whole dopesheet frame geometry are `i64`. A decimal there reads
  as absent, and the renderer falls back to its default without a word.
- Keep every timeline date to `YYYY-MM-DD`, with an optional `HH:MM` after a space or a `T`.
- `on` and `side` on a `card` mean something only inside a timeline. A plain diagram ignores
  them.
- A dopesheet with a wrong sheet path fails the build at the asset copy, not at render time.
  The message names the path it looked for.
