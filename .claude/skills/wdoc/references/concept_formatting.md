# Formatting

_Inline patterns (bold / italic / code / links / icons / math), headings, code, lists, callouts._

The everyday building blocks of a page: prose paragraphs with inline formatting, the six heading levels, syntax-highlighted code listings, lists, and callouts. `p "…"` is the canonical paragraph shorthand; the label text is the paragraph's content and inline patterns light up automatically.


## Inline patterns

The following patterns are recognised in any `p` body, in spans inside a `text` block, and in `utf8` table cells. Note the italic pattern uses underscores around the text.


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

```wcl
p <<DOC
  **bold**, _italic_, `code`, a [link](concept_overview), an inline icon :lucide.check:, and math
  $x^2$ — every pattern lights up automatically.
DOC
```

**bold**, _italic_, `code`, a [link](../references/concept_overview.md), an inline icon :lucide.check:, and math $x^2$ — every pattern lights up automatically.


For a paragraph mixing several styled segments, use a `text` block holding `span`s — each `span` takes its own `class`.


## Headings

Six heading levels are available — `h1` through `h6` — each taking the heading text as a single inline label and an optional `id` (the link target for cross-page anchors). No other fields.


```wcl
h1 "Title"             { id = top }
h2 "Section"
h3 "Subsection"
h4 "Details"
h5 "Fine points"
h6 "Minutiae"
```

## Code blocks

`code <lang> { source = … }` renders a syntax-highlighted listing. The language tag picks the grammar; an unknown tag falls back to plain text. wdoc highlights a wide range of languages (via syntect + two-face) — `rust`, `python`, `javascript`, `typescript`, `go`, `c`, `cpp`, `java`, `ruby`, `php`, `html`, `css`, `json`, `yaml`, `toml`, `sql`, `bash`, `markdown`, and `wcl` among many. The `source` is usually a raw heredoc (`<<'TAG'`) so the body is verbatim, with no escapes or interpolation to mangle backslash-heavy code.


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

## Lists

A `list` holds `li` items. It's a bullet list by default; set `style = :numbered` for a numbered one. Each `li`'s text runs through the inline-pattern engine, so **bold**, `code`, links, and icons all work inside items. Nest an `li` inside an `li` for a sublist (it inherits the parent's style); drop a whole `list` block inside an `li` for a sublist with a different style.


```wcl
list {
  li "Plain item"
  li "With **bold** and a [link](wdoc_overview)"
}

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

```wcl
list {
  style = :numbered
  li "Setup"
  li "Build" {
    li "Compile"
    li "Link"
  }
  li "Run" {
    list {
      li "Foreground"
      li "Background"
    }
  }
}
```

1. Setup
2. Build
  1. Compile
  2. Link
3. Run
  - Foreground
  - Background

## Callouts

A `callout` is an admonition: an icon, a coloured heading, and a body. Six built-in types are selected by `class` (`note`, `info`, `tip`, `warning`, `error`, `success`), each shipping a default colour and icon. For a custom type, give a `class` an `accent` colour and list it in the callout's `class`; the `icon` field picks any glyph.


```wcl
callout "Note"    { class = ["note"]    body = "Background context the reader should remember." }
callout "Warning" { class = ["warning"] body = "Something to be careful about." }

// A custom type: a class's `accent` field supplies the colour, `icon` the glyph.
class "deploy" { accent = "#b48ead" }
callout "Deploying" {
  class = ["deploy"]
  icon  = "lucide.rocket"
  body  = "A **custom** type — the class above sets its accent colour."
}
```

```wcl
callout "Note" {
  class = ["note"]
  body = "Background context the reader should remember."
}
callout "Warning" {
  class = ["warning"]
  body = "Something to be careful about — composes **inline patterns** too."
}
```

> [!NOTE]
> **Note**
> Background context the reader should remember.

> [!WARNING]
> **Warning**
> Something to be careful about — composes **inline patterns** too.

## Block reference

A `p` block: a prose paragraph whose label text runs through the inline-pattern engine.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `text` | `utf8` | yes | The paragraph text (the inline label slot); inline patterns are applied. |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Optional style classes. |

A `span`: an inline run of text inside a `text` block, carrying its own `class`.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `text` | `utf8` | yes | The span text (the inline label slot); inline patterns are applied. |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Optional style classes for this span. |

A `text` block: a paragraph assembled from `span`s, each independently styled.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `text` | `utf8` | no | Optional single-run text (the inline label slot); inline patterns are applied. Use `span` children for styled runs. |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Optional class list applied to the `<p>`. |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `spans` | `span` | yes | The styled segments, rendered as `<span>`s in source order. |

A heading block — `h1` through `h6` all share this one type — taking the heading text and an optional `id` anchor.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `text` | `utf8` | yes | The heading text (the inline label slot); inline patterns are applied. |
| `id` | `identifier` | no | Optional explicit HTML id (the link target for cross-page anchors). |

A `code` block: a syntax-highlighted listing, its language tag picking the grammar and `source` (usually a raw heredoc) holding the body.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `language` | `identifier` | yes | Language tag (the inline label slot). Picks the highlight grammar; an unknown tag falls back to plain text. |
| `source` | `utf8` | yes | The code text — usually a raw heredoc (`<<'TAG'`). |
| `filename` | `utf8` | no | Optional filename shown in the code-card header bar. |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Optional class list (in addition to `code-block`). |

## Related

- [Columns](../references/concept_columns.md)

[← Back to SKILL.md](../SKILL.md)
