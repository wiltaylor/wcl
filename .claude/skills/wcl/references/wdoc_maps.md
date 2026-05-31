# Maps

A `map` is a zoomable, pinned image placed inside a `diagram` — built for game-guide and reference maps. Drop `pin`s at coordinates and give each one a card of wdoc content. A diagram holding a map is **automatically interactive** (wheel to zoom, drag to pan, plus `+` / `−` / `⟲` controls) and loads the bundled map player; you don't need `pan_zoom = true`.

## A single-image map

The common case: `source` is the whole map image, resolved relative to the build entry file. `width` / `height` set the map's coordinate space — a `pin`'s `x` / `y` are pixel coordinates within it. Lift `zoom_max` on the diagram so the markers are readable up close. **Scroll to zoom, drag to pan, then click a marker.**

![diagram](../_wdoc/wdoc_maps-diagram-1.svg)

```wcl
diagram {
  width    = 640
  height   = 320
  zoom_max = 8.0
  map "earth" {
    source = "assets/map/blue-marble.png"
    width  = 1280
    height = 640

    pin "newyork" {
      x = 377  y = 175
      icon  = "lucide.building-2"
      class = ["pin-city"]
      title = "New York"
      text { span "Financial capital on the US east coast." {} }
      callout "Tip" { class = ["tip"]  body = "Cards accept any wdoc content." }
    }
    pin "london" {
      x = 640  y = 137
      icon  = "lucide.landmark"
      class = ["pin-city"]
      title = "London"
      text { span "On the prime meridian (longitude 0)." {} }
    }
    // One-off colour, no class.
    pin "danger" {
      x = 1177  y = 440
      icon  = "lucide.triangle-alert"
      color = "#ef4444"
      size  = 30.0
      title = "Here be dragons"
      text { span "A quick one-off marker colour." {} }
    }
  }
}
```

## Pins & cards

Each `pin` is an icon dropped at `x` / `y` in the map's coordinate space (its `id` is the inline label and must be unique on the page). Style the marker with a `class` (recommended — themable, supports `dark` / `light`) or the one-off `color`. A pin's child blocks become a floating **card** anchored to the marker: click to open, click the `✕` or outside to close. The card body is arbitrary wdoc content — text, lists, callouts, code, images.

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

## Level-of-detail layers

Maps support **different levels of detail**. Omit `layer`s entirely and the map's `source` is the only image. For large maps, supply several `layer`s and the player shows the **sharpest layer that suits the current zoom**: a low-resolution whole image when zoomed out, a higher-resolution one once you magnify in.

A layer is a single image when `cols` / `rows` are `1` (the default), or a grid of tiles otherwise — `source` is then a folder and each tile's filename comes from `pattern` (default `{x}_{y}.png`, 0-based). **Zoom in** on the map below: at the fitted view it draws a 1024px image; magnify it and the player swaps in a 2048px layer assembled from eight 512px tiles, so detail sharpens instead of pixelating.

![diagram](../_wdoc/wdoc_maps-diagram-2.svg)

```wcl
diagram {
  width    = 640
  height   = 320
  zoom_max = 6.0
  map "earth-detail" {
    width     = 2048
    height    = 1024
    tile_size = 512
    // Low-res whole image (zoomed-out view).
    layer { source = "assets/map/lod/low.jpg" }
    // 4x2 grid of 512px tiles (zoomed-in view).
    layer { source = "assets/map/lod"  cols = 4  rows = 2  pattern = "{x}_{y}.jpg" }

    pin "amazon" {
      x = 683  y = 540
      icon  = "lucide.trees"
      class = ["pin-city"]
      title = "Amazon basin"
      text { span "The world's largest rainforest." {} }
    }
    pin "sahara" {
      x = 1109  y = 381
      icon  = "lucide.sun"
      color = "#f59e0b"
      title = "Sahara"
      text { span "The largest hot desert." {} }
    }
  }
}
```

Each `layer` takes:

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `source` | `utf8` | yes | Image file, or the tile folder when tiled. |
| `cols` | `i64` | no | Tiles across (default `1` = a single image). |
| `rows` | `i64` | no | Tiles down (default `1`). |
| `pattern` | `utf8` | no | Tile filename pattern (default `{x}_{y}.png`, 0-based). |
| `tile_size` | `i64` | no | Override the map's `tile_size` for this layer. |

## Fields

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

## Theming

Maps ride the `class` system. A pin's `class` themes its marker (`fill` / `stroke` / `color`, with `dark` / `light`) and `card_class` themes its popup; the one-off `color` recolours a single marker without a class. The built-in look ships as bare-class defaults that read the `--wdoc-*` theme variables, so a `site` `theme` styles maps for free — no per-map CSS needed.
