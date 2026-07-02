# map

A `map` is a zoomable, pinned image placed inside a `diagram` — built for game-guide and reference maps. A diagram holding a map is **automatically interactive** (wheel to zoom, drag to pan, `+` / `−` / `⟲` controls) and loads the bundled map player; you don't need `pan_zoom = true`. `source` is the map image; `width` / `height` set its coordinate space.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | `identifier` | no | Optional reference name (the inline label). |
| `source` | `utf8` | no | Single whole-map image (the common, layer-less case). |
| `width` | `f64` | yes | Map coordinate-space width — also the pin coordinate space. |
| `height` | `f64` | yes | Map coordinate-space height — also the pin coordinate space. |
| `tile_size` | `i64` | no | Tile pixel size for tiled layers (default `256`). |
| `smooth` | `bool` | no | `image-rendering: smooth` (default) vs `pixelated` when `false`. |
| `class` | `list<utf8>` | no | Themes the map group. |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `x` | `f64` | no | Placement x within the enclosing `diagram` / `container`. |
| `y` | `f64` | no | Placement y within the enclosing `diagram` / `container`. |
| `anchor_left` | `f64` | no | Diagram anchor insets (left/right/top/bottom), like any `SvgBlock`. |
| `connect_points` | `list<AnchorSide>` | no | Diagram edge-attach sides, like any `SvgBlock`. |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `layers` | `layer` | yes | Level-of-detail image layers (omit for a single `source`). |
| `pins` | `pin` | yes | Clickable markers with cards. |

## Pins and cards

Each `pin` is an icon dropped at `x` / `y` in the map's coordinate space (its `id` is the inline label, unique on the page). Style the marker with a `class` (themable, supports `dark` / `light`) or the one-off `color`. A pin's child blocks become a floating **card** anchored to the marker: text, lists, callouts, code, images all compose in.

A pin dropped at `x` / `y` in the map's coordinate space; its child blocks become a floating card anchored to the marker.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `id` | `identifier` | yes | Pin id (the inline label) — links the pin to its card; page-unique. |
| `x` | `f64` | yes | Marker x position, in the map's coordinate space. |
| `y` | `f64` | yes | Marker y position, in the map's coordinate space. |
| `icon` | `utf8` | no | Icon name (default `lucide.map-pin`); `set.name` or pair with `set`. |
| `set` | `identifier` | no | Iconset name for a bare `icon`. |
| `size` | `f64` | no | Marker size in map units (default `24`). |
| `class` | `list<utf8>` | no | Themes the marker (`fill` / `stroke` / `color`, with `dark` / `light`). |
| `card_class` | `list<utf8>` | no | Themes the card popup (`background` / `color` / `border`). |
| `color` | `utf8` | no | One-off inline marker colour (sugar for a class). |
| `title` | `utf8` | no | Optional card heading. |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `card` | `WdocBlock` | yes | The card body — any wdoc blocks. |

Live below — **scroll to zoom, drag to pan, then click a marker** to open its card. (A `map` is an interactive viewport, so it renders directly at full width rather than in a light/dark demo, and the diagram is automatically interactive — no `pan_zoom` needed.)

![diagram](../_wdoc/fact_maps-diagram-1.svg)

```wcl
diagram { width = 640  height = 320  zoom_max = 8.0
  map "earth" {
    source = "assets/blue-marble.png"
    width  = 1280  height = 640
    pin "newyork" {
      x = 377  y = 175
      icon  = "lucide.building-2"  color = "#fbbf24"  title = "New York"
      text { span "Financial capital on the US east coast." {} }
      callout "Tip" { class = ["tip"]  body = "Cards accept any wdoc content." }
    }
  }
}
```

## Level-of-detail layers

Omit `layer`s and the map's `source` is the only image. For large maps, supply several `layer`s and the player shows the sharpest layer that suits the current zoom. A layer is one image when `cols` / `rows` are `1`, or a grid of tiles otherwise — `source` is then a folder and each tile's filename comes from `pattern` (default `{x}_{y}.png`, 0-based).

A level-of-detail image (or grid of tiles) the map player swaps in at the zoom range it best suits.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `source` | `utf8` | yes | Image file, or the tile folder when tiled. |
| `cols` | `i64` | no | Tiles across (default `1` = a single image). |
| `rows` | `i64` | no | Tiles down (default `1`). |
| `pattern` | `utf8` | no | Tile filename pattern (default `{x}_{y}.png`, 0-based). |
| `tile_size` | `i64` | no | Override the map's `tile_size` for this layer. |

Maps ride the `class` system: a pin's `class` themes its marker, `card_class` themes its popup, and the built-in look reads the theme variables, so a site `theme` styles maps for free. See [styling](../references/concept_styling.md).

## Related

- [diagram](../references/fact_diagrams.md)

- [image](../references/fact_images.md)

[← Back to SKILL.md](../SKILL.md)
