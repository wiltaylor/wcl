# Dopesheets

`dopesheet` plays a range of frames from a sprite sheet at a set `fps`. Like `tilemap` it's a placeable SVG block — a legal child of any `diagram` or `container` — and like other animated blocks it's special-cased in Rust because the geometry comes from cropping an external image. The sheet is referenced by URL, so frames resolve when the site is **served**, not when the page is opened directly from disk.

## A looping animation

Name the sheet (an `@inline(0)` source, like `image`), describe its frame grid, and pick an `fps`. Here the whole sheet — six 12×12 frames — plays at 12 fps, looped (both defaults). `scale` enlarges the tiny pixel art, and `columns` defaults to as many frames as fit across the sheet (`72 / 12 = 6`).

![diagram](../_wdoc/wdoc_dopesheets-diagram-1.svg)

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

Autoplay and loop are on by default — click the coin to pause or resume. The centred play glyph appears whenever it's stopped.

## Frame range + speed

`from` / `to` pick an inclusive sub-range, and `autoplay = false` leaves the animation stopped until clicked. `offset_x` / `offset_y` and `stride_x` / `stride_y` are spelled out below to show how a sheet with padding or gaps is sliced — for this flush-packed strip they're just the defaults.

![diagram](../_wdoc/wdoc_dopesheets-diagram-2.svg)

```wcl
diagram { width = 96  height = 96
  dopesheet "assets/pixel-coin.png" {
    frame_width  = 12
    frame_height = 12
    stride_x     = 12
    stride_y     = 12
    columns      = 6
    from         = 0
    to           = 2
    fps          = 6.0
    autoplay     = false
    scale        = 6.0
    x            = 12.0
    y            = 12.0
  }
}
```

## Frame geometry



## Playback

By default, dopesheets are `autoplay`, `loop`, and `controls = true`. Click the centred play / pause glyph to toggle. With `loop = false`, playback stops at the last frame and the glyph flips to a replay arrow.

> [!TIP]
> **Smooth or pixelated**
> `image-rendering: pixelated` is the default, so retro pixel art stays crisp. Set `smooth = true` on the dopesheet to opt into the browser's smoothing for non-pixel sources.

> [!NOTE]
> **Asset credit**
> The sample sheet is truezipp's "Pixel Coins Asset", released under CC0 (public domain). Get it at [opengameart.org](https://opengameart.org/content/pixel-coins-asset).
