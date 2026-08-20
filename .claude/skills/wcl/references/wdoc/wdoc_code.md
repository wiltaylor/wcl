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

Write `<<'TAG'`, with the tag quoted: it is fully raw, interpreting neither escapes nor
`${…}`. A plain `<<TAG` interprets backslash escapes, so a `\n` or a `\t` a listing means to
show literally becomes a real newline or tab. It does **not** interpolate. Reach for `$<<TAG`,
which does both, only when the interpolation is the point. `$<<'TAG'` does not parse.

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
`sql`, `c`, `cpp`, `go`, `java`, `ruby`, `php`, `xml`, `diff`, `makefile`, `dockerfile`, `console`,
`markdown`.

**An unknown tag never fails the build.** It falls back to plain text: the listing still
renders, escaped, with no token classes. So a wrong tag is a silent loss of colour, not an
error you will be told about. If a listing renders grey, suspect the tag first.

`console` is a **session** grammar, bundled next to the WCL one because the syntax set ships
none. Tag a terminal transcript `console`: it colours the prompt marker, the program name and
its options, and leaves every output line without a token class, so output renders exactly as
`text` would.

Reach for `bash` / `sh` / `zsh` only for a shell *script*. On a transcript a script grammar
tokenises the captured output as source, and an apostrophe in a diagnostic — `field 'replicas'
is below @min(0)` — opens a string literal that mis-colours the rest of the line. Bare output
with no `$` line is `text`.

The gutter is always on in HTML, so the highlighter works one line at a time. A construct that
spans lines — a block comment — restarts its scope on each line. Short listings are the
supported case.

## `code` is not native — it lowers

`code` lowers to `Content::Code`, one node of the semantic content IR carrying
`{ source, language, filename, id, class }`. No backend re-reads the block's fields. Each one
draws its own chrome from that fixed payload:

| Target | What it draws |
| --- | --- |
| HTML | A `<figure class="code-card">`: three window dots, the filename, the uppercased language tag, then a `<pre class="code-block">` with a CSS-counter line-number gutter. |
| Markdown | The filename as a `` `code` `` line above, then a plain fenced block tagged with the language. No card. |
| PDF | The filename as a code-styled caption line, then the listing drawn as natively coloured runs. |

Two consequences:

- **The card is HTML chrome, not content.** Author `source` and `filename`, and let HTML draw
  the frame — hand-written window dots or a header bar arrive on the other targets as literal
  junk.
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
