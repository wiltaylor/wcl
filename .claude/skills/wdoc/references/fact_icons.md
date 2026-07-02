# iconset / icon_def / icon

Full **Lucide** and **Bootstrap Icons** packs ship compiled into the binary — no runtime disk read, and only used icons land in the output sprite (`_wdoc/icons.svg`). Icons paint with `currentColor`, so colour follows the surrounding text or class.

An `icon` is a placeable block — a legal `diagram` / `container` child positioned by `x` / `y` like a `rect`. The same `icon` / `icon_size` / `icon_pos` / `icon_class` fields also make any box-like shape (`rect` / `circle` / `container` / `process` / `decision` / `terminator`) wear a badge.

```wcl
p "Status: :lucide.check: ready"
```

Status: :lucide.check: ready

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | `identifier` | yes | Icon name (the label slot), optionally `set.name` (e.g. `lucide.compass`). |
| `set` | `identifier` | no | Which declared iconset to read from (when more than one). |
| `size` | `utf8` | no | Icon size, as on an iconset. |
| `color` | `utf8` | no | Foreground colour, as on an iconset. |
| `fill` | `utf8` | no | Fill override, as on an iconset. |
| `background` | `utf8` | no | Background override, as on an iconset. |
| `class` | `list<utf8>` | no | Style classes, as on an iconset. |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `x` | `f64` | no | Top-left placement (or use anchors). |
| `y` | `f64` | no | Top-left placement (or use anchors). |
| `width` | `f64` | no | Size in diagram units (default 24×24). |
| `height` | `f64` | no | Size in diagram units (default 24×24). |
| `scale` | `f64` | no | Extra multiplier on the size. |
| `anchor_left` | `f64` | no | Diagram anchor insets, like any shape. |
| `connect_points` | `list<AnchorSide>` | no | Diagram edge-attach sides, like any shape. |

## Three ways to use an icon

Inline as a `:lucide.check:` pattern in a `p` or `span`; as a placeable `icon` block (a legal `diagram` / `container` child, positioned by `x` / `y` like a `rect`); or as a badge on a box-like shape (`rect` / `circle` / `container` / `process` / `decision` / `terminator`) via `icon` / `icon_size` / `icon_pos` / `icon_class`.

Inline, a bare `:lucide.check:` pattern reads from the named pack and flows with the prose:

A placeable `icon` block sits inside a `diagram`, positioned like any other shape; add it as a badge with `icon` / `icon_pos` on a shape:

```wcl
diagram {
  width = 80
  height = 80
  icon {
    name = "lucide.compass"
    x = 10.0
    y = 10.0
    width = 60.0
    height = 60.0
  }
}
```

![diagram](../_wdoc/fact_icons-diagram-1.svg)

```wcl
diagram {
  width = 160
  height = 80
  process "Validate" {
    id = v
    x = 20.0
    y = 15.0
    width = 120.0
    height = 50.0
    icon = "lucide.shield-check"
    icon_pos = :top_right
  }
}
```

![diagram](../_wdoc/fact_icons-diagram-2.svg)

## Iconsets

An `iconset` renames a pack or sets default styling (`pack` / `size` / `color`); a bare `:name:` then reads from that set.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | `identifier` | yes | Reference name (the label slot), e.g. `ui` — used by `set = ui` and `:ui.name:`. |
| `pack` | `utf8` | no | Bundled pack to read from (`lucide` / `bootstrap`); defaults to the set name. |
| `size` | `utf8` | no | Default inline size for icons in this set (e.g. `"1em"`, `"20px"`). |
| `color` | `utf8` | no | Default foreground — drives `currentColor`. |
| `fill` | `utf8` | no | Optional explicit fill override. |
| `background` | `utf8` | no | Optional background override. |
| `class` | `list<utf8>` | no | Classes added to every icon from this set. |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `icons` | `icon_def` | yes | Per-icon override children (see below). |

An `icon_def` child of an `iconset` overrides an individual icon within the set — for example giving one glyph its own `color`.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | `identifier` | yes | Icon name (the file stem in the pack) — the label slot. |
| `size` | `utf8` | no | Per-icon override of the set's default size. |
| `color` | `utf8` | no | Per-icon override of the set's default foreground colour. |
| `fill` | `utf8` | no | Per-icon override of the set's default fill. |
| `background` | `utf8` | no | Per-icon override of the set's default background. |
| `class` | `list<utf8>` | no | Per-icon classes added on top of the set defaults. |

Declare an `iconset` to rename a pack or set default styling; per-icon `icon_def` children override individual icons within the set. A bare `:name:` then reads from that set. (An `iconset` is a document-root declaration, so this one is shown as source rather than a live preview.)

```wcl
iconset ui {
  pack  = "lucide"
  size  = "1.15em"
  color = "var(--wdoc-accent)"
  icon_def "heart" { color = "#bf616a" }
}

p "We :heart: WCL."
```

As a shape badge, `icon_pos` accepts `:center`, `:top_left` (default), `:top_right`, `:bottom_left`, `:bottom_right`, `:left`, `:right`.

## Related

- [diagram](../references/fact_diagrams.md)

- [tree](../references/fact_tree.md)

[← Back to SKILL.md](../SKILL.md)
