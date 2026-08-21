# Callouts, footnotes and chapter headers

Three page-apparatus blocks. Each lowers to one node of the semantic content IR:
`Content::Callout`, `Content::Footnotes`, `Content::ChapterHeader`. Every backend then renders
it from that one declaration, and no backend re-reads the block.

## `callout`

An admonition: a coloured heading with an icon, over a body.

```wcl
callout "Rollback is automatic" {
  kind = :warning
  body = "A `wcl set` that violates the schema is **rolled back**. Check the exit code."
}
```

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `heading` | `utf8` | yes | The label slot — the coloured heading. |
| `body` | `utf8` | yes | The prose. Runs through the inline-pattern engine. |
| `kind` | `CalloutKind?` | no | The admonition's type, from a closed symbol set. |
| `class` | `list<utf8>?` | no | Legacy kind selection, and any user classes. |
| `icon` | `utf8?` | no | Override the per-kind default icon (`pack.name`). |
| `id` | `identifier?` | no | Explicit HTML id. |

**Prefer `kind` over `class`.** `kind` is the closed set
`note info tip warning error success`, so `kind = :dagner` is a schema violation that stops
the build. A misspelt `class = ["dagner"]` only warns, and the callout renders on with no
accent and no icon — a downgrade that is easy to miss.

`class` selection still works, so nothing written before `kind` existed has to change. When
several recognised names are present the precedence is `warning`, `error`, `success`, `tip`,
`info`, `note`; anything else in `class` is an ordinary style class. `kind` wins when both
are set.

| Kind | Use it for | Default icon | Default accent | Themed hue | Markdown alert |
| --- | --- | --- | --- | --- | --- |
| `note` | Background the reader should remember | `lucide.pencil` | `#5e81ac` | `blue` | `> [!NOTE]` |
| `info` | Neutral information worth surfacing | `lucide.info` | `#88c0d0` | `cyan` | `> [!IMPORTANT]` |
| `tip` | A shortcut or a better way | `lucide.lightbulb` | `#b48ead` | `purple` | `> [!TIP]` |
| `warning` | Something to be careful about | `lucide.triangle-alert` | `#d08770` | `yellow` | `> [!WARNING]` |
| `error` | A failure or a hard constraint | `lucide.circle-x` | `#bf616a` | `red` | `> [!CAUTION]` |
| `success` | Confirm something completed | `lucide.circle-check` | `#a3be8c` | `green` | `> [!TIP]` |

**All six kinds are distinct on every backend.** Six icons, six hues, and — where GFM runs
out of keywords — a marker that carries the kind anyway. A callout with no kind has no icon,
a grey accent, and `[!NOTE]` in Markdown.

GitHub has five alert keywords and wdoc has six kinds, so `tip` and `success` share `TIP`.
The kind is not lost: every kinded callout emits an HTML comment under the keyword, which
renders as nothing on GitHub and keeps the vocabulary readable as data.

```markdown
> [!TIP]
> <!-- wdoc-callout: success -->
> **Migration complete**
> Every row moved.
```

The accent colours the heading, the left border and the icon — never the body text. Each one
rides a **CSS custom property, `--callout-accent`**. A site theme's apply rules override the
defaults above from its own hue ring (the "Themed hue" column), so treat those hex values as
the unthemed fallback. PDF resolves the theme's **light** hue ring, because a page is a light
medium.

The kind survives lowering as a **symbol**, and each backend maps that symbol to its own
accent, icon and alert keyword. No backend matches a class name. A custom class therefore
gives you a colour but no icon and no alert keyword. Its `accent` field writes
`--callout-accent`, which is why it needs no CSS:

```wcl
class "deploy" { accent = "#b48ead" }

callout "Deploying" {
  class = ["deploy"]
  icon  = "lucide.rocket"
  body  = "A custom kind: the class sets the accent, `icon` picks the glyph."
}
```

Gotchas:

- Both `heading` and `body` run through the inline-pattern engine, so `**bold**`,
  `` `code` ``, `[links](page)`, `:icons:` and `$math$` work in either.
- `body` is one string field, so a callout holds prose — not a list, a code block or a
  diagram. For a framed container of arbitrary blocks, use a `wdoc_component` with a
  `wdoc_content` slot ([`wdoc_data_views.md`](wdoc_data_views.md)).
- The default icons come from the bundled Lucide pack and need no `iconset` declaration, on
  the web and in PDF alike.
- In PDF the box paints one shaped heading over one shaped body, so prose flattens into it.
  Anything richer lands *beneath* the box rather than disappearing.
- A bare identifier in a field expression resolves against the block's own fields first, so a
  `wdoc_slot kind` is now shadowed inside a `callout`. Name the slot something else.

## `footnote` and `footnotes`

A page collects its definitions in one `footnotes` block, usually last. In prose, `[^id]`
references it.

```wcl
page eval {
  p "Fields evaluate lazily[^lazy], and a cycle is a diagnostic rather than a hang."

  footnotes {
    footnote lazy { text = "Forced on first read, then cached on the document view." }
  }
}
```

`footnote` fields: `id` (the label slot, an `identifier`) and `text` (patterned prose).
`footnotes` holds `@children("footnote")` and nothing else. A `footnote` never renders where
it is declared — only through its parent block.

**A footnote's `marker` is its declaration id.** That id is the key both ends of the link are
anchored on:

- HTML — the definition is an `<li id="fn-<id>">` with a `↩` back-link to `#fnref-<id>`; a
  post-pass rewrites each `[^id]` in the page into
  `<sup class="footnote-ref" id="fnref-<id>">`. The **visible number** is the ordered list's,
  assigned by *definition* order, not by where the reference appears.
- Markdown — `[^marker]: text`, a real GFM footnote label.
- PDF — a "Footnotes" heading, then one `marker. text` paragraph per note.

Gotchas:

- **Only a `[^id]` whose id has a definition is rewritten.** This is deliberate: a literal
  `[^abc]` in a code sample is a regex character class, and is left alone. The flip side is
  that a typo in a reference silently stays as literal text.
- **The rewrite runs in the template path.** A page renders *bare* when it sets no `template`
  and its site sets no `default_template`. A bare page never gets the rewrite. Every `[^id]`
  then stays literal, even with a matching definition below it. Set a template.
- **Reference each footnote once per page.** The rewrite gives every occurrence of `[^id]` the
  same `fnref-<id>`. A second reference therefore duplicates an HTML id, and the back-link
  lands on the first one.
- The Markdown target escapes a `[^id]` *reference* in prose instead of linking it. Without
  the page's definitions, the inline engine cannot tell one from a regex class. The
  definitions themselves still emit as GFM labels.
- The `[^id]` rewrite matches `[a-zA-Z0-9_-]+`, so keep ids to letters, digits, `_` and `-`.

## `chapter_header`

A rich page header: an eyebrow, the page `h1`, and a meta line.

```wcl
chapter_header "Documents, fields and blocks" {
  kicker       = "Chapter 1 · The WCL language"
  reading_time = "9 min read"
  updated      = "2026-08-07"
  version      = "wcl 0.24.1-alpha"
}
```

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `title` | `utf8` | yes | The label slot — rendered as the page `h1`. |
| `kicker` | `utf8?` | no | Small eyebrow above the title. |
| `reading_time` | `utf8?` | no | A free-text label; nothing is computed for you. |
| `updated` | `utf8?` | no | A free-text label. |
| `version` | `utf8?` | no | A free-text label. |
| `id` | `identifier?` | no | Explicit HTML id for the header. |

Whichever of `reading_time`, `updated` and `version` are set join into **one meta line, in
that order, separated by ` · `**. That separator is a fact about the line, shared by all three
backends, so set the three fields separately and let wdoc join them. With none of the three
set, there is no meta line at all.

The three values are strings you write. wdoc does not count words for `reading_time`, does not
read git for `updated`, and does not know its own `version` here.

Markdown renders the kicker as an italic line, the title as `# `, and the meta line as italic.
PDF renders kicker and meta as paragraphs around an `h1`. Use `chapter_header` *instead of* an
`h1`, not above one, or the page gets two titles.
