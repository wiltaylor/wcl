# Code

The `code` block is a syntax-highlighted source listing. The language tag is the block's
label. The listing itself comes from one of two fields — `source` for inline text, almost
always a **raw heredoc** so that backslashes, quotes and `${…}` survive verbatim, or
`source_file` to read it off disk at build time. Give exactly one; both, or neither, fails
the build.

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
| `source` | `utf8?` | one of | The code text, inline. |
| `source_file` | `utf8?` | one of | Path to read the listing from, relative to the document. |
| `anchor` | `utf8?` | no | Region between `ANCHOR: name` / `ANCHOR_END: name` in `source_file`. |
| `lines` | `utf8?` | no | 1-based inclusive range of `source_file`: `"12-30"`, `"12-"`, `"-30"`, `"12"`. |
| `dedent` | `bool?` | no | Strip the indentation the selected region shares. Default `false`. |
| `filename` | `utf8?` | no | Shown in the HTML card header; a caption line elsewhere. Defaults to `source_file`. |
| `id` | `identifier?` | no | Explicit HTML id. |
| `class` | `list<utf8>?` | no | Extra classes, added to `code-block`. |

`anchor` and `lines` are two ways to say the same thing — setting both fails the build. Both
apply only to `source_file`.

The label is an `identifier`, so it is bare: `code rust { … }`, never `code "rust" { … }`.
A one-liner still needs the braces, because `source` is a named field:

```wcl
code console { source = "$ wcl check config.wcl" }
```

## Reading a listing from a file

`source_file` resolves against the directory of the document that names it, so a path may
climb out of the doc tree — which is the usual case, since the code being documented rarely
lives under the manual.

```wcl
code rust { source_file = "../src/retry.rs" }
```

Narrow it with `anchor` (robust — survives edits above it) or `lines` (brittle — renumbers
whenever the file grows):

```wcl
code rust {
  source_file = "../src/retry.rs"
  anchor      = "backoff"
  dedent      = true
}
```

The markers go in the quoted file, in whatever comment syntax it uses — the match is on the
text `ANCHOR: name`, so `// ANCHOR: x`, `# ANCHOR: x` and `<!-- ANCHOR: x -->` all work:

```rust
// ANCHOR: backoff
pub fn backoff(attempt: u32) -> Duration { … }
// ANCHOR_END: backoff
```

**Marker lines never reach the output**, in any mode — anchored, line-ranged or whole-file.
Anchors may nest; an inner anchor's markers vanish from an outer region too. A page that
needs to *show* a marker writes that listing inline with `source`.

**A read that fails fails the build**, exit 3, with a message naming the file: a missing
`source_file`, an `anchor` the file does not mark, a `lines` range past its end. That is the
point of the field — an inline copy goes stale silently, a file-backed one cannot.

Two limits worth knowing:

- `wcl check` does **not** catch a broken listing. It parses and schema-validates; it never
  renders, and the read happens at render time. Only `wcl wdoc build` (or a `serve` rebuild)
  reports it.
- `wcl wdoc serve` does not watch the quoted file. Its watcher only tracks `.wcl`, and
  rebuilds are manual anyway (press Enter). A **full** rebuild re-reads the file, so editing
  the code and pressing Enter does pick the change up; it is just not what tells you to.

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
`{ source, source_file, anchor, lines, dedent, language, filename, id, class }`. A file-backed
listing is read by one pass over that IR before any backend runs, so all three targets render
one read rather than three that could disagree, and a backend only ever sees `source` filled
in. No backend re-reads the block's fields. Each one
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
  `line_numbers` field to turn it off, no start-line field, and no highlight-line field.
  (`lines` chooses which lines are *included* from a `source_file`; it does not number or
  emphasise them.)

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
  [`wdoc_media.md`](wdoc_media.md). `source_file` here does the opposite: it reads a file
  *into* the page and copies nothing.
