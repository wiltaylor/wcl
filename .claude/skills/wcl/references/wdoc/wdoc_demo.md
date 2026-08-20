# Demo blocks

A `demo` shows a fragment of wdoc source **and** renders that same fragment live, under both
the light and the dark palette, side by side. You author the fragment once, as the demo's
children; the block prints their formatted source as the "Example" and renders them as the
"Preview".

This is the block to reach for when documenting wdoc itself. A hand-copied `code` listing next
to a hand-authored example drifts the moment either is edited; a `demo` cannot.

```wcl
demo {
  title = "A tip callout"
  callout "Tip" { class = ["tip"]  body = "Author once, see both." }
}
```

## Fields

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `title` | `utf8?` | no | Caption above the block. **Write it as a field** — see the gotcha below. |
| `diagram` | `bool` | no, default `false` | Centre and scale the two previews, for diagram-family children. |
| `id` | `identifier?` | no | Explicit HTML id. |
| `children` | `list<ContentBlock>` | — | The fragment: any content blocks. |

**Gotcha: `title` only works as a field.** It is declared `@inline(0)`, so
`demo "A tip callout" { … }` parses without complaint — and renders no caption at all. The
renderer reads `title` as a named field. Write `demo { title = "…" … }`.

`diagram = true` keeps the two previews side by side. It also centres and scales each one to
fit its pane, so a compact diagram fills the box instead of sitting in a half-empty one:

```wcl
demo {
  diagram = true
  diagram { width = 120  height = 60
    rect { x = 10.0  y = 10.0  width = 100.0  height = 40.0  fill = "#a3be8c" }
  }
}
```

## What it emits in HTML

A `<div class="wdoc-demo">` holding, **in this order**:

1. The optional `title`, as a `.wdoc-demo-label`.
2. A `Preview` label, then a `.wdoc-preview-row` of two panes:
   `.wdoc-theme-light .wdoc-preview` and `.wdoc-theme-dark .wdoc-preview`.
3. An `Example` label, then the children's formatted WCL source in a highlighted
   `<pre class="code-block"><code class="language-wcl">`.

The theme renderer emits the palette-scoped `.wdoc-theme-light` and `.wdoc-theme-dark`
wrappers. CSS custom properties inherit, so each pane re-themes its own subtree. The reader's
global theme toggle does not change either pane. That is the whole point: the page shows both
readings at once.

The block renders the children **twice**, once per palette mode. HTML content would re-theme
for free from the scoped variables. SVG content — diagrams, wireframes — cannot: Rust bakes
the resolved palette into it, because a PDF needs the same colours and carries no
`currentColor`. One render therefore cannot serve both panes. The image, icon and video
registries key on the source, so the second pass adds nothing and emits nothing twice.

The "Example" source comes from pretty-printing each child. It is the **formatter's**
rendering of what you wrote — normalised spacing and field order, not a byte copy of your
file.

## `demo` is native

`demo` is `@native` on all four targets. WCL cannot express either half of the job — reading a
block's own source text, and re-rendering its children into themed wrappers. There is
therefore no `lower` to intercept and no `Content` payload behind it.

Degradation, since a static target has no theming and no two panes:

| Target | What you get |
| --- | --- |
| HTML | Title, dual-palette preview, highlighted source. |
| Markdown | The source as a fenced `wcl` block, then **one** un-themed render of the children. The `title` is dropped. |
| PDF | **Only** one render of the children, in place. No source listing, no title. |

So in Markdown the order flips — source first, then the render — and in PDF the example
disappears entirely. If the source listing matters in print, write a `code` block instead.

## Neighbours

- A plain source listing with no live render: [`wdoc_code.md`](wdoc_code.md).
- The palettes the two panes use, and the `class` system:
  [`wdoc_styling.md`](wdoc_styling.md).
- Previewing a page's *generated Markdown* rather than its source is `markdown_source` — see
  [`wdoc_outputs.md`](wdoc_outputs.md).
