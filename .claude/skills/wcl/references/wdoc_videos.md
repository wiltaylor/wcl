# Videos

`video` embeds a video on a page. The source is either a local file (typically `.mp4`, copied into `_wdoc/` at build time like an `image`) or a web URL — YouTube, Vimeo, or any other embeddable address. To keep a page light, a video first renders as a click-to-play \*facade\* (a poster thumbnail plus a play button); the real player loads only when the reader clicks.

> [!NOTE]
> **Served, not opened directly**
> A local video is referenced by a relative _wdoc/ URL, so it only resolves when the site is served or hosted — not when the page is opened directly from disk (same as images and icons).

## A local file

Give `video` a doc-relative path and an optional `poster` thumbnail. Set `width` to size it, `title` for an accessible label. Click the poster to play. Live (a short Big Buck Bunny excerpt, © Blender Foundation, CC-BY):

```wcl
page about { sites = [:demo]
  video "assets/intro.mp4" {
    poster = "assets/intro-thumb.jpg"
    title  = "Product intro"
    width  = 640.0
  }
}
```

> [!NOTE]
> **No frame extraction**
> wdoc never decodes a local video, so it can't auto-generate a thumbnail. Supply a `poster` image; without one, a plain play-button placeholder shows.

## YouTube & Vimeo

A YouTube or Vimeo URL is recognised automatically and embedded as a privacy-respecting click-to-play iframe. For YouTube, the poster thumbnail is derived from the video id when you don't supply your own. Live:

[Big Buck Bunny on YouTube](https://www.youtube.com/watch?v=aqz-KE-bpKQ)

```wcl
video "https://www.youtube.com/watch?v=aqz-KE-bpKQ" { title = "Trailer" }
video "https://vimeo.com/76979871" {
  poster = "assets/vimeo-thumb.jpg"   // Vimeo has no auto-thumbnail
  title  = "Our talk"
}
```

## Any other embed

Any other `http(s)` URL is embedded verbatim in an iframe (the \*generic\* form). A URL ending in a video extension (`.mp4`, `.webm`, …) instead plays natively in a `<video>` element.

```wcl
video "https://example.com/player/embed/abc" { title = "Hosted clip" }
video "https://cdn.example.com/clip.webm"     { title = "Direct file" }
```

## In PDF output

A PDF can't play video, so `wcl wdoc pdf` renders the poster thumbnail and — for an online video only — a link to it. A local video shows just its poster (a path to a local file is useless in a distributed PDF).

## Fields


