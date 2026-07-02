# dopesheet

`dopesheet` plays a range of frames from a sprite sheet at a set `fps` — a placeable SVG block, a legal child of any `diagram` or `container`. Here the whole sheet loops at 12 fps (click the coin to pause):

```wcl
diagram {
  width = 96
  height = 96
  dopesheet "../../assets/pixel-coin.png" {
    frame_width = 12
    frame_height = 12
    scale = 6.0
    fps = 12.0
    x = 12.0
    y = 12.0
  }
}
```

![diagram](../_wdoc/fact_dopesheets-diagram-1.svg)

`dopesheet` plays a range of frames from a sprite sheet at a set `fps`. Like `tilemap` it's a placeable SVG block — a legal child of any `diagram` or `container`. The sheet is referenced by URL (an `@inline(0)` source, like `image`), so frames resolve when the site is **served**, not when the page is opened directly from disk.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `source` | `utf8` | yes | Spritesheet image path (the inline label), relative to the build entry file. |
| `frame_width` | `i64` | yes | Size of one frame in the sheet (pixel width). |
| `frame_height` | `i64` | yes | Size of one frame in the sheet (pixel height). |
| `offset_x` | `i64` | no | Pixel offset to the first frame's left edge (default `0`). |
| `offset_y` | `i64` | no | Pixel offset to the first frame's top edge (default `0`). |
| `stride_x` | `i64` | no | Origin-to-origin x step between frames (default = `frame_width`). |
| `stride_y` | `i64` | no | Origin-to-origin y step between frames (default = `frame_height`). |
| `columns` | `i64` | no | Frames per sheet row (default: fit from the sheet width). |
| `from` | `i64` | no | First frame index to play (default `0`). |
| `to` | `i64` | no | Last frame index to play, inclusive (default: the last frame). |
| `fps` | `f64` | no | Playback rate in frames/second (default `12`). |
| `loop` | `bool` | no | Restart at the end (default `true`). |
| `autoplay` | `bool` | no | Start playing on load (default `true`). |
| `controls` | `bool` | no | Click play/pause toggle (default `true`). |
| `smooth` | `bool` | no | Anti-alias instead of the default `image-rendering: pixelated`. |
| `scale` | `f64` | no | Display scale (default `1.0`). |
| `x` | `f64` | no | Position x within the enclosing `diagram` / `container`. |
| `y` | `f64` | no | Position y within the enclosing `diagram` / `container`. |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Optional style classes. |
| `anchor_left` | `f64` | no | Diagram anchor insets (left/right/top/bottom), like any `SvgBlock`. |
| `connect_points` | `list<AnchorSide>` | no | Diagram edge-attach sides, like any `SvgBlock`. |

## Frame geometry and playback

Describe the frame grid with `frame_width` / `frame_height`, plus `offset_x` / `offset_y` and `stride_x` / `stride_y` for padding or gaps; `columns` defaults to as many frames as fit across the sheet. `from` / `to` pick an inclusive sub-range. `scale` enlarges pixel art; `image-rendering: pixelated` is the default, so set `smooth = true` for non-pixel sources. Autoplay, loop, and controls are on by default — click to pause or resume. (The sample sheet is truezipp's CC0 "Pixel Coins Asset".)

Just the first three frames (`from` / `to`), spelt-out grid geometry, and `autoplay = false` so it starts paused — click to play:

```wcl
diagram {
  width = 96
  height = 96
  dopesheet "../../assets/pixel-coin.png" {
    frame_width = 12
    frame_height = 12
    stride_x = 12
    stride_y = 12
    columns = 6
    from = 0
    to = 2
    fps = 6.0
    autoplay = false
    scale = 6.0
    x = 12.0
    y = 12.0
  }
}
```

![diagram](../_wdoc/fact_dopesheets-diagram-2.svg)

## Related

- [tilemaps](../references/fact_tilemaps.md)

- [image](../references/fact_images.md)

- [diagram](../references/fact_diagrams.md)

[← Back to SKILL.md](../SKILL.md)
