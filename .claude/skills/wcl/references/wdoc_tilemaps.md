# Tilemaps

A `tilemap` draws a 2D grid of indexed tiles cropped from a shared spritesheet. Author it numerically (a list of int rows) or symbolically via a glyph legend.

## Declaring a tileset

`tileset` names a spritesheet image plus its slice geometry, and hangs off the document root (like `iconset`). The sheet's pixel dimensions are read from the image header at build time (PNG / JPEG / GIF); set `image_width` / `image_height` to override. Tile index N maps to sheet column `N % columns`, row `N / columns`.

```wcl
tileset platformer {
  source      = "assets/kenney-platformer.png"
  tile_width  = 64
  tile_height = 64
  columns     = 5
}
```



## Symbolic maps

For map-like authoring, declare a glyph legend with `tile` children and supply `map` — a list of utf8 rows. Each character resolves through the legend to a tile index; an unmapped glyph (here `.`) draws nothing, so the page shows through. The tilemap is a diagram shape, so anything drawn after it overlays on top.

![diagram](../_wdoc/wdoc_tilemaps-diagram-1.svg)

```wcl
diagram { width = 280  height = 230
  tilemap {
    set   = "platformer"
    scale = 0.5
    tile "#" { index = 25 }   // brown crate — ground
    tile "~" { index = 1 }    // water surface
    tile "=" { index = 6 }    // deep water
    tile "T" { index = 16 }   // torch
    tile "G" { index = 28 }   // grass tuft
    tile "r" { index = 27 }   // rock
    map = [
      "........",
      "........",
      ".T.G..r.",
      "########",
      "~~~~~~~~",
      "========",
    ]
  }
  // Overlaid after the tilemap, so it sits over the tiles.
  label "Level 1" { x = 128.0  y = 146.0  font_size = 16.0  fill = "#003a8c" }
}
```

## Numeric tiles

The same data can be written as raw index rows with `tiles` (one inner list per row). `-1` — the default `empty` index — leaves a cell blank. Here a single row doubles as a tile palette.

![diagram](../_wdoc/wdoc_tilemaps-diagram-2.svg)

```wcl
diagram { width = 320  height = 44
  tilemap {
    set   = "platformer"
    scale = 0.6
    tiles = [
      [ 25, 9, 1, 6, 16, 28, 27, 37 ],
    ]
  }
}
```

## Fields



`scale` sizes the display; `image-rendering: pixelated` is the default (via the always-injected `TILEMAP_CSS`), and `smooth = true` opts into the browser's default smoothing. The sheet is copied to `_wdoc/`, so tiles resolve when served.

> [!NOTE]
> **Asset credit**
> The sample spritesheet is Kenney's "Platformer Pack" (64×64 tiles), released under CC0 (public domain). Browse the packs at [kenney.nl/assets](https://kenney.nl/assets).
