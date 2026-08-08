# Code

The `code` block is a syntax-highlighted source listing. The language tag is the block's
label; the source is a field, and is almost always a **raw heredoc** so that backslashes,
quotes and `${…}` survive verbatim.

```wcl
code rust {
  filename = "src/main.rs"
  source = <<'RS'
fn main() {
    println!("hello");
}
RS
}
```

Write `<<'TAG'` (quoted tag), not `<<TAG`. A plain heredoc interpolates `${…}`, which is
exactly what a shell script or a WCL sample does not want. Use `$<<TAG` only when you
deliberately want the interpolation.

## Fields

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `language` | `identifier` | yes | The label slot. Picks the highlight grammar. |
| `source` | `utf8` | yes | The code text. |
| `filename` | `utf8?` | no | Shown in the HTML card header; a caption line elsewhere. |
| `id` | `identifier?` | no | Explicit HTML id. |
| `class` | `list<utf8>?` | no | Extra classes, added to `code-block`. |

The label is an `identifier`, so it is bare: `code rust { … }`, never `code "rust" { … }`.
A one-liner still needs the braces, because `source` is a named field:

```wcl
code console { source = "$ wcl check config.wcl" }
```

## The language tag

Highlighting is syntect with the two-face extra grammar set, plus a WCL grammar bundled into
the binary. The tag is matched first as a **token** (an extension or the usual short name),
then by grammar name. Reliable tags include:

`wcl`, `rust`, `python`, `js`, `ts`, `json`, `yaml`, `toml`, `html`, `css`, `sh`, `bash`,
`sql`, `c`, `cpp`, `go`, `java`, `ruby`, `php`, `xml`, `diff`, `makefile`, `dockerfile`,
`markdown`.

**An unknown tag never fails the build.** It falls back to plain text: the listing still
renders, escaped, with no token classes. So a wrong tag is a silent loss of colour, not an
error you will be told about. If a listing renders grey, suspect the tag first.

`console` is one of these plain-text tags — there is no shell-session grammar. It is still the
right tag for a terminal transcript. It says what the listing is, and the Markdown fence
carries it through. Only the colour is missing.

The gutter is always on in HTML, and the highlighter then works one line at a time. A
construct that spans lines — a block comment — restarts its scope on each line. Short listings
are the supported case.

## `code` is not native — it lowers

`code` lowers to `Content::Code`, one node of the semantic content IR carrying
`{ source, language, filename, id, class }`. No backend re-reads the block's fields. Each one
draws its own chrome from that fixed payload:

| Target | What it draws |
| --- | --- |
| HTML | A `<figure class="code-card">`: three window dots, the filename, the uppercased language tag, then a `<pre class="code-block">` with a CSS-counter line-number gutter. |
| Markdown | The filename as a `` `code` `` line above, then a plain fenced block tagged with the language. No card. |
| PDF | The filename as a code-styled caption line, then the listing drawn as natively coloured runs. |

Two consequences worth holding on to:

- **The card is HTML chrome, not content.** Do not author window dots or a header bar
  yourself; on the other three targets they would be literal junk.
- The line-number gutter is pure CSS (`counter-increment` on `.code-line`). There is no
  `line_numbers` field to turn it off, and no start-line or highlight-line field.

## Styling

The card's colours come from the site theme's apply rules, and the tokens carry stable
`tok-…` classes — `.tok-keyword`, `.tok-string`, `.tok-comment`, `.tok-function` and so on.
Restyle a listing by declaring rules against those, not by touching the block. `class` on the
block lands on the `<pre>`, beside `code-block`.

## Neighbouring blocks

- Inline `` `code` `` inside prose is an **inline pattern**, not this block — see
  [`wdoc_formatting.md`](wdoc_formatting.md).
- `demo` prints its children's formatted WCL source **and** renders them live. When you
  document wdoc itself, prefer it to a hand-copied listing in a `code` block. See
  [`wdoc_demo.md`](wdoc_demo.md).
- `markdown_source` renders a page's own generated Markdown into a `code markdown` block —
  see [`wdoc_outputs.md`](wdoc_outputs.md).
- A `file` block ships a real file into the output instead of quoting it into the page — see
  [`wdoc_media.md`](wdoc_media.md).
