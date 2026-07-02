# styling shapes with classes

Every diagram shape takes a `class` list. A `class` emits SVG paint — `fill`, `stroke`, `stroke_width`, `opacity` — with `dark { }` / `light { }` overrides, so a shape follows the site theme and the reader's light/dark mode (instead of a baked-in `fill`). The stdlib ships ready-made shape classes — `wdoc-node`, `wdoc-process`, `wdoc-decision`, `wdoc-terminator` — that read the theme palette:

```wcl
diagram {
  width = 360
  height = 120
  rect {
    x = 20.0
    y = 25.0
    width = 100.0
    height = 70.0
    class = ["wdoc-node"]
  }
  rect {
    x = 140.0
    y = 25.0
    width = 100.0
    height = 70.0
    class = ["wdoc-process"]
  }
  circle {
    cx = 305.0
    cy = 60.0
    r = 35.0
    class = ["wdoc-decision"]
  }
}
```

![diagram](../_wdoc/fact_shape_styling-diagram-1.svg)

Declare your own with a `class` block carrying `fill` / `stroke` (and `dark { }` / `light { }`), then list it on any shape. Because the paint comes from the class — not a hard-coded `fill` — the same shape recolours for the theme and for the reader's mode. See the [styling](../references/concept_styling.md) concept for the full class system.

```wcl
diagram {
  width = 320
  height = 110
  rect {
    x = 20.0
    y = 25.0
    width = 120.0
    height = 60.0
    class = ["accent-box"]
  }
  rect {
    x = 180.0
    y = 25.0
    width = 120.0
    height = 60.0
    class = ["accent-box"]
  }
}
```

![diagram](../_wdoc/fact_shape_styling-diagram-2.svg)

```wcl
// A theme-aware shape class with light/dark overrides.
class "accent-box" {
  fill   = "#3b4252"
  stroke = "#88c0d0"
  light { fill = "#e5e9f0"  stroke = "#5e81ac" }
}
```

## Related

- [primitive shapes](../references/fact_primitive_shapes.md)

- [diagram](../references/fact_diagrams.md)

- [Styling](../references/concept_styling.md)

[← Back to SKILL.md](../SKILL.md)
