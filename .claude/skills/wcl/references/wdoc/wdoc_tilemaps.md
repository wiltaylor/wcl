# Tilemaps and maps

Two image-backed diagram shapes:

- **`tilemap`** paints a grid of fixed-size tiles cropped out of one spritesheet. The sheet and
  its slice geometry are a separate root-level `tileset` declaration.
- **`map`** shows one large image you can pan and zoom, with clickable `pin` markers that open
  floating cards.

Both `extends SvgBlock`, so both live inside a `diagram` (or a `container`) and are placed by
`x` / `y` or by anchors. Both are **native** blocks — wdoc crops the images in Rust — so neither
declares a `lower`.

Both copy their image assets into the output's `_wdoc/` folder and reference them by URL. The
images therefore resolve when the output is **served**, not when a page is opened directly from
disk. That is the same limit `image` has.

## `tileset` — declaring a spritesheet

A `tileset` sits at the **document root**, beside `iconset` — not inside a page or a diagram.
It names an image plus the geometry needed to slice it:

```wcl
tileset platformer {
  source      = "assets/kenney-platformer.png"
  tile_width  = 64
  tile_height = 64
  columns     = 5
}
```

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `name` | `identifier` | yes | The reference name, written as the inline label. |
| `source` | `utf8` | yes | Image path, relative to the **build entry file** — not to the page that uses it. |
| `tile_width`, `tile_height` | `i64` | yes | Source tile size, in pixels. |
| `columns` | `i64` | no | Tiles per sheet row. Auto-fitted from the sheet width when omitted. |
| `margin` | `i64` | no | Pixel border around the whole sheet (default `0`). |
| `spacing` | `i64` | no | Pixel gap between tiles (default `0`). |
| `image_width`, `image_height` | `i64` | no | Override the sheet size wdoc reads from the file header. |

**Tile index N sits at sheet column `N % columns`, row `N / columns`**, counting from `0`. That
one rule is the whole addressing model. Get `columns` wrong and every tile is off by a shift.

wdoc reads the sheet's pixel size from the PNG / JPEG / GIF header. Supply `image_width` /
`image_height` only when that fails.

## `tilemap` — painting the grid

Author the grid one of two ways. `map` wins when a block sets both.

**Symbolic** — `tile` legend children bind one glyph each, and `map` is a list of rows:

```wcl
diagram {
  width = 280  height = 230
  tilemap {
    set   = "platformer"
    scale = 0.5
    tile "#" { index = 25 }   // ground
    tile "~" { index = 1 }    // water surface
    tile "=" { index = 6 }    // deep water
    tile "T" { index = 16 }   // torch
    tile "G" { index = 28 }   // grass
    map = [
      "........",
      "........",
      ".T.G....",
      "########",
      "~~~~~~~~",
      "========",
    ]
  }
  // Drawn after the tilemap, so it sits on top of the tiles.
  label "Level 1" { x = 128.0  y = 146.0  font_size = 16.0  fill = "#003a8c" }
}
```

A glyph with no legend entry draws nothing. That is how `.` stays empty above.

**Numeric** — raw index rows, one inner list per row:

```wcl
tilemap {
  set   = "platformer"
  scale = 0.5
  tiles = [
    [ 16, -1, 28 ],
    [ 25, 25, 25 ],
    [  1,  1,  1 ],
  ]
}
```

`-1` is the default `empty` index and leaves a cell blank.

| Field | Type | Meaning |
| --- | --- | --- |
| `set` | `identifier` | The `tileset` to slice from. Write it as a string: `set = "platformer"`. |
| `map` | `list<utf8>` | Symbolic rows, resolved through the `tile` legend. Wins over `tiles`. |
| `tiles` | `list<list<i64>>` | Numeric rows of tile indices. |
| `legend` | `@children("tile")` | The glyph legend: `tile "#" { index = 25 }`. |
| `empty` | `i64` | Index that means "no tile" (default `-1`). |
| `scale` | `f64` | Display scale (default `1.0`). |
| `smooth` | `bool` | Anti-alias instead of the default `image-rendering: pixelated`. |
| `x`, `y` | `f64` | Placement in the parent diagram. |
| `id`, `class` | | Edge target name and style classes. |
| `anchor_left`, `anchor_right`, `anchor_top`, `anchor_bottom` | `f64` | Anchor insets, like any shape. |
| `connect_points` | `list<AnchorSide>` | Edge-attach sides (`:north :east :south :west`). |

A `tile` legend entry has just two fields: the `@inline(0)` `glyph` and its `index`.

Pixel art reads best with nearest-neighbour scaling, so tiles render `pixelated` by default.
Set `smooth = true` for art that should be filtered. The `wdoc-tilemap` class carries both
rules.

Rows do not have to be equal length, but a ragged grid is almost always a typo.

## `map` — a pannable, pinned image

```wcl
diagram {
  width = 640  height = 320  zoom_max = 8.0
  map "earth" {
    source = "assets/blue-marble.png"
    width  = 1280  height = 640
    pin "newyork" {
      x = 377  y = 175
      icon  = "lucide.building-2"
      color = "#fbbf24"
      title = "New York"
      p "Financial capital on the US east coast."
      callout "Tip" { class = ["tip"]  body = "Cards accept any wdoc content." }
    }
  }
}
```

**A diagram holding a map is interactive on its own.** Wheel to zoom, drag to pan, plus `+` /
`−` / `⟲` controls. You do **not** set `pan_zoom = true`; the map loads the bundled player
itself. `zoom_max` on the enclosing diagram still raises the zoom ceiling (default `4.0`).

| `map` field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `name` | `identifier` | no | Reference name, the inline label. |
| `source` | `utf8` | no | The single whole-map image. This is the common case. |
| `width`, `height` | `f64` | yes | The map's **coordinate space** — also the space every pin's `x` / `y` is in. |
| `tile_size` | `i64` | no | Tile pixel size for tiled layers (default `256`). |
| `smooth` | `bool` | no | Smooth image scaling (default), or `pixelated` when `false`. |
| `class`, `id` | | | Class themes the map group; `id` is the shape's HTML id. |
| `x`, `y`, anchors, `connect_points` | | | Placement in the diagram, like any shape. |
| `layers` | `@children("layer")` | no | Level-of-detail images. Omit for a single `source`. |
| `pins` | `@children("pin")` | no | The clickable markers. |

`width` / `height` are the coordinate space, **not** the on-page size. A pin at `x = 377` means
377 units into that space. Change the space and every pin moves.

### Pins and cards

| `pin` field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `id` | `identifier` | yes | The inline label. It links the marker to its card and must be unique on the page. |
| `x`, `y` | `f64` | yes | Position in the map's coordinate space. |
| `icon` | `utf8` | no | Icon name (default `lucide.map-pin`). Use `set.name`, or pair a bare name with `set`. |
| `set` | `identifier` | no | Iconset for a bare `icon` name. |
| `size` | `f64` | no | Marker size in map units (default `24`). |
| `class` | `list<utf8>` | no | Themes the marker: `fill` / `stroke` / `color`, with `dark` / `light` variants. |
| `card_class` | `list<utf8>` | no | Themes the card popup: `background` / `color` / `border`. |
| `color` | `utf8` | no | One-off inline marker colour. Sugar for a class. |
| `title` | `utf8` | no | Card heading. |
| `card` | `@children(ContentBlock)` | | The card body. Any page blocks: text, lists, callouts, code, images. |

The pin's child blocks *are* the card. There is no separate `card` block to write.

### Level-of-detail layers

Omit `layer`s and the map's `source` is the only image. For a large map, supply several and the
player swaps in the sharpest one for the current zoom:

```wcl
map "dungeon" {
  width = 4096  height = 4096
  layer { source = "maps/dungeon-far.png" }
  layer { source = "maps/dungeon-near/"  cols = 16  rows = 16  pattern = "{x}_{y}.png" }
}
```

| `layer` field | Type | Meaning |
| --- | --- | --- |
| `source` | `utf8` | An image file, or the **tile folder** when the layer is tiled. |
| `cols`, `rows` | `i64` | Tiles across and down (default `1` each — a single image). |
| `pattern` | `utf8` | Tile filename pattern (default `"{x}_{y}.png"`, 0-based). |
| `tile_size` | `i64` | Overrides the map's `tile_size` for this layer. |

## Gotchas

- **A `tileset` is a root declaration.** Put it beside your `page` blocks, not inside one. A
  `tilemap` naming an undeclared set fails the build.
- **`source` resolves against the build entry file**, not against the file the block is written
  in. A page imported from a subfolder still writes the path as the entry sees it.
- **Get `columns` right.** It is what turns an index into a sheet coordinate. Everything shifts
  when it is wrong, and nothing errors.
- **`map` wins over `tiles`.** Setting both silently ignores the numeric grid.
- **A map's `width` / `height` are the pin coordinate space**, not a display size. The diagram's
  own `width` / `height` frame it on the page.
- **A map makes its diagram interactive by itself.** Adding `pan_zoom = true` is redundant.
- **Assets need a server.** Tiles, sheets and map images are copied into `_wdoc/` and fetched by
  URL, so they render when the output is served, and not from a page opened straight off disk.
- **Pin ids must be unique per page**, because they link a marker to its card.
