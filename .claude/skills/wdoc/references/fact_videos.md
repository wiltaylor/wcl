# video

`video` embeds a video on a page — either a local file (typically `.mp4`, copied into `_wdoc/` like an `image`) or a web URL (YouTube, Vimeo, or any other embeddable address). To keep a page light, a video first renders as a click-to-play **facade** (a poster thumbnail plus a play button); the real player loads only on click.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `source` | `utf8` | yes | Video source (the label slot) — a doc-relative path, or a YouTube / Vimeo / other URL. |
| `poster` | `utf8` | no | Thumbnail shown before play — a doc-relative path or URL. Auto-derived for YouTube; otherwise a placeholder. |
| `title` | `utf8` | no | Accessible label (and the iframe / link title). |
| `width` | `f64` | no | Display size; responsive default if omitted. |
| `height` | `f64` | no | Display size; responsive default if omitted. |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Optional class list. |

A YouTube URL renders live as a privacy-respecting click-to-play facade (the poster is derived from the video id — no local asset needed):

```wcl
video "https://www.youtube.com/watch?v=aqz-KE-bpKQ" {
  title = "Big Buck Bunny"
}
```

[Big Buck Bunny](https://www.youtube.com/watch?v=aqz-KE-bpKQ)

## A local file

Give `video` a doc-relative path and an optional `poster` thumbnail. Set `width` to size it and `title` for an accessible label. wdoc never decodes a local video, so it can't auto-generate a thumbnail — supply a `poster`, or a plain play-button placeholder shows.

```wcl
page about { sites = [:demo]
  video "assets/intro.mp4" {
    poster = "assets/intro-thumb.jpg"
    title  = "Product intro"
    width  = 640.0
  }
}
```

## YouTube, Vimeo, and other embeds

A YouTube or Vimeo URL is recognised automatically and embedded as a privacy-respecting click-to-play iframe (YouTube derives its poster from the video id). Any other `http(s)` URL is embedded verbatim in an iframe; a URL ending in a video extension (`.mp4`, `.webm`) plays natively in a `<video>` element.

```wcl
video "https://www.youtube.com/watch?v=aqz-KE-bpKQ" { title = "Trailer" }
video "https://vimeo.com/76979871" {
  poster = "assets/vimeo-thumb.jpg"   // Vimeo has no auto-thumbnail
  title  = "Our talk"
}
```

## PDF output

A PDF can't play video, so `wcl wdoc pdf` degrades every `video` to a still: it renders the `poster` thumbnail (or a plain play-button placeholder when none is given) at the block's `width`, and — for an **online** video only — prints the URL beneath it as a tappable link. A local-file video has nowhere to link to, so it shows the poster alone.

So author a `poster` for any video you expect to appear in a PDF — otherwise the page shows only the placeholder box. The same `poster` doubles as the click-to-play facade in HTML, so it's worth setting regardless of target.

> [!TIP]
> **Rule of thumb**
> Always give a local-file `video` a `poster`: it's the HTML facade thumbnail **and** the PDF still. YouTube derives its own poster from the video id, so an embed can skip it.

## Related

- [image](../references/fact_images.md)

[← Back to SKILL.md](../SKILL.md)
