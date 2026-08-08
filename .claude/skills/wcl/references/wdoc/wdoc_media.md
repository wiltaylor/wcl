# Images, videos and file assets

Three blocks that pull something from disk (or the web) into the output: `image`, `video` and
`file`.

## Path resolution — read this first

Every local `source` is resolved **relative to the build entry document's folder**, not
relative to the `.wcl` file the block is written in. A page under `pages/guide.wcl` that is
imported by `docs/main.wcl` still writes its paths from `docs/`. This trips up almost
everyone once.

A source is **external** — passed through verbatim, never copied — when it starts with
`http://`, `https://`, `data:` or `/`. Anything else is a local file.

A local file is copied into the output and referenced by a **relative URL**, so it resolves
when the output is served or hosted. It does not resolve in a page opened directly from disk.

A local source that does not exist fails the build when the copy pass runs — not at parse
time, and not with the page name attached. If a build dies on an I/O error naming a path you
half-recognise, check an `image` / `file` source against the entry folder.

## `image`

```wcl
image "assets/architecture.png" {
  alt   = "The three crates and their dependencies"
  width = 640.0
}
```

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `source` | `utf8` | yes | The label slot: a doc-relative path, a URL, or a `data:` URI. |
| `alt` | `utf8?` | no | Alt text for the page `<img>`. |
| `width` / `height` | `f64?` | no | Display size. Omit both to use the file's natural size. |
| `id`, `class` | | no | Explicit id; classes added to `wdoc-image`. |
| `x`, `y`, `scale`, `anchor_*`, `connect_points` | | no | Diagram placement only — ignored on a page. |

`image` is **`@native`** and it extends *both* `ContentBlock` and `SvgBlock`, so one block
serves two placements:

- Directly under a `page` → a standalone `<img class="wdoc-image">`, responsive by default
  (`max-width: 100%`).
- Inside a `diagram` or `container` → a placeable SVG `<image>`, positioned by `x` / `y` or by
  anchors, connectable by edges like a `rect`. See [`wdoc_diagrams.md`](wdoc_diagrams.md).

The natural pixel size is read from the file header, so a diagram image with neither `width`
nor `height` still gets a bounding box. A local image lands in `_wdoc/` under a readable stem
plus a hash of its source path (`image-architecture-1f2e3d4c.png`), so two `logo.png` files in
different folders cannot collide.

**There is no crop field.** `image` shows the whole file. Cutting sprites out of a sheet is
the `tileset` / `tile` job — see [`wdoc_tilemaps.md`](wdoc_tilemaps.md). There is also no
caption field; put a `p` under the block.

## `video`

```wcl
video "https://www.youtube.com/watch?v=aqz-KE-bpKQ" {
  title = "Big Buck Bunny"
}

video "assets/intro.mp4" {
  poster = "assets/intro-thumb.jpg"
  title  = "Product intro"
  width  = 640.0
}
```

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `source` | `utf8` | yes | The label slot: a doc-relative path, or a YouTube / Vimeo / other URL. |
| `poster` | `utf8?` | no | Thumbnail shown before play. Auto-derived for YouTube. |
| `title` | `utf8?` | no | Accessible label, and the iframe / link title. |
| `width` / `height` | `f64?` | no | Display size; responsive default when omitted. |
| `id`, `class` | | no | Explicit id and classes. |

`video` lowers to `Content::Video`, and it is **page-only** — unlike `image` it is not an
`SvgBlock`, so it cannot go inside a diagram.

In HTML a video first renders as a lightweight **click-to-play facade**: a poster plus a play
button. The bundled `_wdoc/wdoc-video.js` swaps in the real `<video>` or `<iframe>` only on
click, so a page with ten embeds loads none of them. YouTube and Vimeo URLs are recognised and
embedded as privacy-respecting iframes; any other `http(s)` URL is embedded verbatim, and a
URL ending in a video extension plays natively.

Per target:

| Target | What you get |
| --- | --- |
| HTML | The click-to-play facade, then the real player. |
| Markdown | A link, labelled with `title` (falling back to the URL). A local file is still copied out and linked. |
| PDF | The poster still at the block's `width`, plus — for an **online** video only — the URL printed beneath as a link. |

**Author a `poster` for any local-file video.** wdoc never decodes a video, so it cannot
generate a thumbnail. Without one, HTML shows a plain play-button placeholder, and PDF shows
an empty box with nowhere to link. A YouTube embed can skip it: YouTube derives its own poster
from the video id.

## `file`

Ship an arbitrary file into the build output, and optionally link to it.

```wcl
file "scripts/bootstrap.sh" {
  dir = "scripts"
  as  = "bootstrap.sh"
}

file "assets/schema.json" { dir = "assets" }   // shipped silently
```

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `source` | `utf8` | yes | The label slot: a doc-relative path, a URL, or a `data:` URI. |
| `dir` | `utf8?` | no | Output subdirectory. Defaults to the `_wdoc/` asset folder. |
| `as` | `utf8?` | no | Link text. Set ⇒ renders a link; absent ⇒ the file is shipped silently. |
| `id`, `class` | | no | Explicit id, and classes on the rendered link. |

Unlike `image`, a `file` **keeps its basename** under `dir`, so the emitted path is stable and
hand-linkable — `scripts/bootstrap.sh`, not a hashed name. That is what lets a build write a
folder a reader or a tool can navigate by convention. The cost is that two different sources
whose basenames collide within one `dir` are a conflict, not two files.

Two roles from one block: with no `as` the file is copied and nothing renders — reference it
yourself by its `<dir>/<basename>` path. With `as`, a link renders in its place.

### `file` refuses to build to PDF

`file` is declared `@native(backends = [:html, :markdown])`. A PDF is one
self-contained document with no folder beside it to copy into, so a rendered link would point
at something that was never shipped. A page carrying a `file` block that is built to PDF is
therefore a **build error** until you state the intent per instance:

```wcl
@except(backends = [:pdf])
file "scripts/bootstrap.sh" { as = "bootstrap.sh" }
```

This is the general rule for native blocks, not a special case: capability says *can't*,
`@except` says *don't want to*. See [`wdoc_visibility.md`](wdoc_visibility.md).
