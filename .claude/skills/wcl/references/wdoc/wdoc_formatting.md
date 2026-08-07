# Text and formatting

The prose vocabulary: the six heading levels, the paragraph blocks and the side-by-side
`column` layout. It also covers the inline patterns that light up inside every one of them.

## Headings

`h1` through `h6`. Each one takes the heading text as its inline label and an optional `id`.
There are no other fields.

```wcl
h1 "Title"      { id = top }
h2 "Section"
h3 "Subsection"
h4 "Details"
h5 "Fine points"
h6 "Minutiae"
```

| Field | Type | Meaning |
| --- | --- | --- |
| label | `utf8` | The heading text. Inline patterns apply. |
| `id` | `identifier?` | An explicit anchor target, for a cross-page link. |

The renderer emits a real `<hN>` and derives a `heading-N` class from the level:

```html
<h1 class="heading-1" id="top">Title</h1>
<h2 class="heading-2" id="section"><span class="heading-marker">§ 1</span>Section</h2>
```

(`§ 1` is the section marker a templated page adds; see below.)

Three things follow:

- On a **templated** page, wdoc **synthesises** an `id` you do not supply. It slugifies the
  heading text, so `"Hello There"` gives `id="hello-there"`. It also numbers h2 and h3 headings
  with a `§` marker. A page that renders bare gets neither: no `default_template`, and no page
  `template`. Set `id` when you need a target that survives a wording change.
- The `heading-N` class is a style hook. Restyle a level by declaring
  `class heading-2 { css = "…" }`.
- The `book` template lists the page's own h2 and h3 headings in the right-hand "on this page"
  rail. It derives that from the authored headings; no field controls it.

## Paragraphs

### `p` — the shorthand

```wcl
p "Quick prose with **bold** and `inline code`."
p "A styled line." { class = ["lead"] }
```

| Field | Type | Meaning |
| --- | --- | --- |
| label | `utf8` | The paragraph text. Inline patterns apply. |
| `id` | `identifier?` | An explicit HTML id. |
| `class` | `list<utf8>?` | Style classes. |

`p` is the canonical paragraph. Reach for it first.

### `text` and `span` — one paragraph from several runs

```wcl
text {
  span "One run, "
  span "then another."
}

text "A leading label, " {
  class = ["lead"]
  span "then a span."
}
```

`text` accepts an optional label of its own, rendered before its spans. That form is the natural
one inside a `card` body or a `node_row`, where a bare `text "…"` reads best.

The spans concatenate **in source order** into one paragraph:

```html
<p>One run, then another.</p>
<p class="lead">A leading label, then a span.</p>
```

**A span's `id` and `class` do not render.** The content model carries prose as a string, not
as a list of styled runs. A `text` therefore flattens to one paragraph. The two fields stay
declared, so an existing document keeps validating. But `span "…" { class = ["accent"] }`
paints nothing. Style the whole paragraph through `class` on the `text`. To format inside it,
use the inline patterns below.

Use `text` when you assemble a paragraph from computed pieces. Use `p` otherwise.

## Inline patterns

wdoc recognises every pattern below in a `p` label, in a `span` and in a heading. It also
recognises them in an `li`, in a callout body and in a `utf8` table cell.

The content model carries prose rather than markup,
so every target sees them. Each target then renders what it can. HTML draws all of them.
Markdown re-emits bold, italic, code and links as Markdown, and leaves an icon as its literal
`:name:` text.

| Write | Get |
| --- | --- |
| `**bold**` | a run with the `bold` class |
| `_italic_` | a run with the `italic` class |
| `` `code` `` | a run with the `code` class (monospace) |
| `[text](page)` | an in-site link to that page |
| `[text](site:page)` | a cross-site link |
| `[text](https://…)` | an external link |
| `https://example.com` | the same, with no brackets — the display text drops the scheme |
| `:lucide.check:` | an inline icon (`pack.name`, or a bare name from a declared `iconset`) |
| `$x^2$` | an inline LaTeX equation, text style |
| `$$\int x \, dx$$` | an inline LaTeX equation, display style |

```wcl
p "Here is **bold**, _italic_, `code`, a [link](other), and an icon :lucide.check:."
```

```html
<p>Here is <span class="bold">bold</span>, <span class="italic">italic</span>,
<span class="code">code</span>, a <a class="link" href="other.html">link</a>,
and an icon …</p>
```

Patterns **nest**. A matched run is fed back through the engine, with a depth cap, so bold can
contain italic.

### The guards you will hit

Each guard exists because the regex engine has no look-around, so a literal case has to be
excluded by the pattern itself.

- **`_italic_` is word-boundary gated.** The engine skips a match that touches a letter, a
  digit or `_` on either side. `safe_mode_password` stays literal; `a _word_ here` formats.
  This is why snake_case identifiers survive.
- **`$…$` may not touch whitespace at either end.** `it cost $10 or $20` stays prose, and
  `$x^2$` is an equation. Malformed LaTeX renders an inline error marker; it does not fail the
  build.
- **`:name:` needs a lower-case name.** The pattern is `:([a-z0-9][a-z0-9._-]*):`, and an
  unresolved name falls back to the literal text. `time: 10:30` is therefore untouched, and so
  is a typo — check the rendered page.
- **A bare URL stops before sentence punctuation.** A trailing `.` or `,` stays prose. It also
  stops at whitespace, quotes and brackets.
- **`[text](url)` wins over the bare-URL pattern**, because its `[` starts earlier.
- **`$$…$$` wins over `$…$`**, because it is declared first and matches at the `$$` opener.

### Declaring your own

An `inline_pattern` block is a regex plus a function from the captured groups to inline runs.
Group 0 is the whole match, and groups 1 and up are the explicit captures, read with
`at(g, n)`:

```wcl
inline_pattern kbd {
  pattern  = "\\|\\|([^|\n]+)\\|\\|"
  boundary = false
  to_span  = fn(g: list<utf8>) -> list<InlineSpan>
    [InlineSpan::Plain { text: at(g, 1), class: ["kbd"] }]
}

class kbd { css = "font-family:var(--wdoc-font-mono);border:1px solid;" }

p "Press ||Ctrl+C|| to stop."
```

```html
<p>Press <span class="kbd">Ctrl+C</span> to stop.</p>
```

| Field | Type | Meaning |
| --- | --- | --- |
| label | `identifier` | The pattern name. |
| `pattern` | `utf8` | The regex. |
| `boundary` | `bool` | Default `false`. `true` skips a match that touches a word character on either side — the `_italic_` guard. |
| `to_span` | `fn(list<utf8>) -> list<InlineSpan>` | Builds the runs. |

The four `InlineSpan` variants:

| Variant | Fields |
| --- | --- |
| `Plain` | `text`, `class?` |
| `Link` | `text`, `href`, `class?` |
| `Icon` | `name`, `class?` |
| `Math` | `latex`, `display?`, `class?` |

Declare the block at the document top level. It applies to the whole document.

## `column` — side by side

`column` lays its children out across the page instead of stacking them. `widths` gives one CSS
percentage per child slot, and the children flow across the slots in source order:

```wcl
column {
  widths = [50.0, 50.0]
  p "Left half."
  p "Right half."
}
```

```html
<div style="display:grid;grid-template-columns:50% 50%;"><p>Left half.</p><p>Right half.</p></div>
```

| Field | Type | Meaning |
| --- | --- | --- |
| `widths` | `list<f64>` | One percentage per child slot. Required. They should total about 100. |
| `id` | `identifier?` | An explicit HTML id. |
| `class` | `list<utf8>?` | Classes on the whole group. |
| children | `ContentBlock`s | Any wdoc blocks — a paragraph, a heading, a callout, a code block, a diagram, a chart, another `column`. |

Any number of columns works:

```wcl
column {
  widths = [33.3, 33.3, 33.3]
  h4 "One"
  h4 "Two"
  h4 "Three"
}
```

**`column` is HTML-only layout.** The Markdown and PDF targets render the children **stacked in
source order**. The layout is lost; the content is not. Never put meaning in the arrangement
alone.

## Gotchas

- Put each block on **its own line**. A second block on the same line becomes a *child* of the
  first. `li "one"  li "two"` renders one item, and `h2 "Agenda"  list { … }` fails with
  `block kind 'li' is not allowed inside 'h2'`.
- `class` is a **list**: `class = ["lead"]`, not `class = "lead"`.
- A `span`'s `id` and `class` render nowhere. Style the `text` instead.
- A link's URL is a bare **page name**, not a path or a filename. `[docs](docs.html)` is wrong;
  `[docs](docs)` is right. A link to an unknown page fails the build.
- A prose string with a literal `$`, `_`, `` ` `` or `**` may match a pattern. Check the
  rendered page.
- In a WCL string a `$` is only special in an interpolating string. A regex that must match a
  literal `$` writes `\\$`, which decodes to the two characters `\$`.

## See also

- [`wdoc_code.md`](wdoc_code.md) — the `code` block.
- [`wdoc_tables.md`](wdoc_tables.md) — `list`, `li` and `table`.
- [`wdoc_callouts.md`](wdoc_callouts.md) — `callout`, `footnote` and `chapter_header`.
- [`wdoc_icons.md`](wdoc_icons.md) — the icon packs the `:name:` pattern resolves against.
- [`wdoc_math.md`](wdoc_math.md) — the `math` block and the supported LaTeX.
- [`wdoc_styling.md`](wdoc_styling.md) — the `class` blocks that give `bold`, `italic`, `code`
  and your own patterns their look.
