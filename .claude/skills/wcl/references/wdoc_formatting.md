# Formatting

The everyday building blocks of a page: prose paragraphs with inline formatting, the six heading levels, and syntax-highlighted code listings.

## Paragraphs with p

`p "..."` is the canonical paragraph shorthand. The label text is the paragraph's content, and inline patterns light up automatically. Previewed in a card:

![diagram](../_wdoc/wdoc_formatting-diagram-1.svg)

```wcl
p "Quick prose with **bold** and `inline code`."
p "A whole paragraph in one line."
```

Its fields (generated from the schema with `type_table`):

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `text` | `utf8` | yes | The paragraph text (the inline label slot); inline patterns are applied. |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Optional style classes. |

For a paragraph mixing several styled segments, use a `text` block holding `span`s — each `span` takes its own `class`.

## Inline patterns

The following patterns are recognised in any `p` body, in spans inside a `text` block, and in `utf8` table cells:

```wcl
**bold**             // → bold class
_italic_             // → italic class
`code`               // → monospace inline code
[text](page)         // → in-site link to a page
[text](site:page)    // → cross-site link
[text](https://...)  // → external link
:lucide.check:       // → inline icon (pack.name)
$x^2$                // → inline LaTeX (text style)
$$\int x \, dx$$     // → display-style LaTeX
```

Live, all in one paragraph:

![diagram](../_wdoc/wdoc_formatting-diagram-2.svg)

## Headings

Six heading levels are available — `h1` through `h6` — each taking the heading text as a single inline label and an optional `id`. Previewed in a card (kept out of this page's own outline):

![diagram](../_wdoc/wdoc_formatting-diagram-3.svg)

```wcl
h1 "Title"             { id = top }
h2 "Section"
h3 "Subsection"
h4 "Details"
h5 "Fine points"
h6 "Minutiae"
```

Each `h1`–`h6` takes the heading text as its inline label plus an optional `id` (the link target for cross-page anchors). No other fields.

## Code blocks

`code <lang> { source = ... }` renders a syntax-highlighted listing. The language tag picks the grammar; an unknown tag falls back to plain text. A live, highlighted listing:

![diagram](../_wdoc/wdoc_formatting-diagram-4.svg)

```wcl
code rust {
  source = <<'RUST'
fn fib(n: u32) -> u64 {
    match n {
        0 => 0,
        1 => 1,
        _ => fib(n - 1) + fib(n - 2),
    }
}
RUST
}
```

Its fields:

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `language` | `identifier` | yes | Language tag (the inline label slot). Picks the highlight grammar; an unknown tag falls back to plain text. |
| `source` | `utf8` | yes | The code text — usually a raw heredoc (`<<'TAG'`). |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Optional class list (in addition to `code-block`). |

wdoc highlights a wide range of languages (via syntect + two-face). The tag is the language name or a common file extension; WCL ships its own grammar, so `code wcl { … }` highlights too. Supported tags include:

The `source` is usually a heredoc — use a raw heredoc (`<<'TAG'`) so the body is taken verbatim, with no escapes or interpolation to mangle backslash-heavy code. See [Strings & Heredocs](../references/strings.md) for the full syntax.

## Lists

A `list` holds `li` items. It's a bullet list by default; set `style = :numbered` for a numbered one. Each `li`'s text runs through the inline-pattern engine, so **bold**, `code`, links, and icons all work inside items. A live bullet and numbered list:

![diagram](../_wdoc/wdoc_formatting-diagram-5.svg)

```wcl
list {
  li "Plain item"
  li "With **bold** and a [link](wdoc_overview)"
}

list { style = :numbered
  li "First step"
  li "Second step"
}
```

Nest an `li` inside an `li` for a sublist — it inherits the parent list's style, so a numbered list numbers sublists hierarchically (`2.1`, `2.2`). For a sublist with a \*different\* style, drop a whole `list` block inside the `li` instead. Live:

![diagram](../_wdoc/wdoc_formatting-diagram-6.svg)

```wcl
list { style = :numbered
  li "Setup"
  li "Build" {
    li "Compile"          // renders 2.1
    li "Link"             // renders 2.2
  }
  li "Run" {
    list {                // a bulleted sublist inside a numbered list
      li "Foreground"
      li "Background"
    }
  }
}
```

The `list` block's fields:

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Optional style classes. |
| `style` | `ListStyle` | no | `:bullet` (default → `<ul>`) or `:numbered` (→ `<ol>`). |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `items` | `li` | yes | The list items. |

And each `li`:

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `text` | `utf8` | yes | Item text (the inline label); inline patterns apply. |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Optional style classes. |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `children` | `ListNode` | yes | Nested `li`s (a sublist), or a whole `list` block for a different style. |

## Callouts

A `callout` is an admonition: an icon, a coloured heading, and a body. Six built-in types are selected by `class`, each shipping a default colour and icon. Live — all six, plus a custom `deploy` type at the end:

![diagram](../_wdoc/wdoc_formatting-diagram-7.svg)

```wcl
callout "Note"    { class = ["note"]    body = "Background context the reader should remember." }
callout "Info"    { class = ["info"]    body = "Neutral information worth surfacing." }
callout "Tip"     { class = ["tip"]     body = "A helpful shortcut or best practice." }
callout "Warning" { class = ["warning"] body = "Something to be careful about." }
callout "Error"   { class = ["error"]   body = "A failure or hard constraint." }
callout "Success" { class = ["success"] body = "Confirm an action completed." }

// A custom type: a class's `accent` field supplies the colour, `icon` the glyph.
class "deploy" { accent = "#b48ead" }
callout "Deploying" {
  class = ["deploy"]
  icon  = "lucide.rocket"
  body  = "A **custom** type — the class above sets its accent colour."
}
```

The six built-in types ship their own accent. For a custom type, give a `class` an `accent` colour and list it in the callout's `class` — that sets the accent (heading, border, icon) with no CSS. The `icon` field picks any glyph. See [Styling](../references/wdoc_styling.md) for the full class reference.

Its fields:

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `heading` | `utf8` | yes | Inline label — the coloured heading at the top of the callout. |
| `body` | `utf8` | yes | The prose under the heading. Runs through the inline-pattern engine. |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Selects the type: `["note"]` / `["tip"]` / etc. May also carry user classes. |
| `icon` | `utf8` | no | Override the default icon (any `pack.name` from a built-in or declared iconset). |
