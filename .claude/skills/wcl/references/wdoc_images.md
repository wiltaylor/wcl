# Images

`image` embeds a static image. The source path resolves relative to the build entry file (`docs/`); the file is copied verbatim into `_wdoc/` at build time, so the output site is self-contained.

> [!NOTE]
> **Served, not opened directly**
> Images are referenced by a relative _wdoc/ URL, so they only resolve when the site is served or hosted — not when the page is opened directly from disk (same as icons and tilemaps).

## On a page

An `image` block directly under a `page` renders as a standalone `<img>`. The source is the inline label; set `width` / `height` to size it, `alt` for accessibility, and a `class` for styling. Live (a Kenney CC0 spritesheet, scaled down):

![Kenney platformer tile sheet](../_wdoc/image-kenney-platformer-75559923.png)

```wcl
page about { sites = [:demo]
  image "assets/hero.jpg" {
    alt   = "Our team"
    width = 480.0
    class = ["hero"]
  }
}
```

## Inside a diagram

`image` is also a placeable `SvgBlock`, so it can sit in a `diagram` / `container` alongside `rect` / `circle` / etc., positioned by `x` / `y` (or anchors). Live:

![diagram](../_wdoc/wdoc_images-diagram-1.svg)

```wcl
diagram {
  width = 280  height = 140
  image "assets/logo.png" {
    x = 20.0  y = 20.0  width = 100.0  height = 100.0
  }
  rect { x = 150.0  y = 45.0  width = 100.0  height = 50.0  fill = "#a3be8c" }
}
```

## Fields

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `source` | `utf8` | yes | Image source (the inline label): a doc-relative path, a URL, or a `data:` URI. |
| `alt` | `utf8` | no | Alt text for the `<img>` (page form). |
| `width` | `f64` | no | Display width; the natural size is used when omitted. |
| `height` | `f64` | no | Display height; the natural size is used when omitted. |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Optional style classes. |
| `x` | `f64` | no | Placement x when used inside a diagram (ignored on a page). |
| `y` | `f64` | no | Placement y when used inside a diagram (ignored on a page). |
| `scale` | `f64` | no | Extra size multiplier (diagram form). |
| `anchor_left` | `f64` | no | Diagram anchor insets (left/right/top/bottom), like any shape. |
| `connect_points` | `list<AnchorSide>` | no | Diagram edge-attach sides, like any shape. |
