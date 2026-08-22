# technical-book audit

Manual: `docs/reference`
Standard: `~/.claude/skills/technical-book/reference/standard.md`
Config: `.agent/config.yaml`
Coverage: **100%** — 244 of 244, verified by `just docs-coverage`
Build: `wcl wdoc build docs/reference/main.wcl` — OK, wrote 46 pages
Prose: `/technical-writing review` — clean
Run: 2026-08-22 (third run, after the second fix pass)

7 findings, all dismissed. The first run against the shipped default produced
441.

| Run | Findings | What changed |
|-----|----------|--------------|
| 1 | 441 | The shipped default, describing a product manual this book is not |
| 2 | 339 | `.agent/config.yaml` describing the book's real shape |
| 3 | 33 | Chapter headers, callout kinds, the reading measure, the builtin index, the icon-gallery split |
| 4 | **7** | The chapter split, the callout pass, coverage, the prose review |

## Findings

Every open finding is closed. These seven are dismissed, each for a reason
about this book rather than a decision to ignore a rule.

| Finding | Severity | Status | Detail |
|---------|----------|--------|--------|
| `site/missing-chapter/install` | error | dismissed | Install is an `h2` of Quick start, which is 1,633 words of prose end to end. Two chapters of ~800 words would add navigation without adding content. |
| `site/missing-chapter/first-task` | error | dismissed | Same chapter, the *Your first document* section. |
| `Callouts, footnotes and chapter headers/callout-count` | error | dismissed | 13 callouts, and 7 of them are `demo` instances of the block the chapter documents — the six kinds side by side, plus the custom-class example. 6 are apparatus, under the limit. The checker counts a callout inside a `demo` the same as one in the prose, and for this one chapter that is the wrong reading. |
| `Callouts, footnotes and chapter headers/callout-kind/Deploying` | error | dismissed | The one callout in the book that keeps `class = ["deploy"]`. It is inside a `demo`, and the point it demonstrates is that a user class sets a custom accent. Writing `kind` there would delete the example. |
| `Icons/inline-icon/bootstrap.house` ×2 | warn | dismissed | Inside a `code` example showing the inline-icon spelling. The literal text is the example. |
| `Icons/inline-icon/lucide.triangle-alert` | warn | dismissed | Same. |

## What changed

### Structure

| | |
|---|---|
| **Themes and styling split in two** | It was 21 `h2`, 23 `h3`, 5,310 words and 11 callouts covering two subjects. *Themes* (`wdoc_styling`) keeps colour, palettes, `extends`, fonts and `metrics`. *Styling rules* (`wdoc_css`) takes `class`, `base`, `media`, `keyframes`, `font_face`, `style`, the emission order, the built-in class vocabulary and the build's two checks. 48 incoming links were re-read one at a time: 27 meant the rules half and now point there, 21 meant themes and were relabelled. 11 *Where to go next* entries that pointed at both under one link became two entries each. |
| **Icon galleries on their own page** | 1,711 Lucide + 2,078 Bootstrap glyphs, ~950 lines, were sitting inside the Icons chapter. Now `wdoc_icon_gallery` with its own toc entry. The chapter went from 199KB to 50KB of source. |
| **Builtins index** | 102 builtins were a flat list of `h3` under loose groups with no way in for a reader who knows the name they want. Added *Every builtin, A to Z*: four columns, each linked, all 119 anchors on the page verified to resolve. The sections below still group by what a builtin operates on. |
| **Chapter headers, 46 of 46** | Every chapter opens with a `chapter_header` carrying a `<Part> · <mode>` kicker, a reading time, the date of its last content change, and `wcl 0.33.2-alpha`. The book documented the block and used it on zero pages. 39 chapters declare `reference`, 6 `explanation`, 1 `tutorial`. |
| **Quick start prerequisites** | A *Before you start* section: what you need, how long it takes, which platforms get the prebuilt binary, and that no prior WCL is assumed. |

### Display

The site now names a three-line `reference_book` theme that extends `:nord` and
restates one metric. `.book-measure` takes 3.5rem of padding off each side, so
the shipped 60rem left a 53rem text column — at 17px, about 100 characters a
line, well past the 45–75 a reader scans without losing the return sweep. 52rem
lands near 90. Code blocks are unaffected: `pre.code-block` is `overflow-x:
auto`, and 99% of the book's code lines are under 89 characters anyway.

The standard's own metrics were **not** applied. 58rem at 15px is about 109
characters — longer than what the book started with, and smaller type. The
config records that.

### Callouts

217 callouts moved from `class = ["note"]` to `kind = :note`. No visual change:
`Callout.lower` in `crates/wcl_wdoc/lib/callout.wcl` already read the kind off
`class`. But a misspelt `kind` is a build error, where a misspelt `class` used
to render on with the grey `#888` default and no icon.

The limit moved from 4 to 8 in config. The standard's 4 is calibrated for a
manual where a callout marks an exception; in a language reference the recurring
move is *here is what `wcl check` does not catch*, and cataloguing those is the
chapter's job. 8 still catches a chapter that has started shouting — 4 flagged
27 chapters that were working as intended.

Four callouts were demoted to prose because they explained rather than warned:
*Labels and lines are annotations, not nodes* and *Chrome is not an obstacle* in
The diagram canvas, *One place an omitted role does fall back* and *A theme
block is not per-site* in Themes.

### Coverage

`docs/reference/coverage.py` — new, and `just docs-coverage` runs it. It reads
the public surface out of the product rather than out of a list: builtins from
`wcl repl` calling `builtin_names()`, subcommands from `wcl --help` and `wcl
wdoc --help`, block kinds from `@block(...)` and `@table(...)` in
`crates/wcl_wdoc/lib`. Then it checks each against what the manual names, and
exits 1 on a gap.

```
ok  symbol    100/100  100.0%
ok  command    11/11   100.0%
ok  block     133/133  100.0%
```

Verified non-vacuous: three fabricated entries were injected and all three were
reported.

It is deliberately **not** wired as the audit's `extractor`. The audit matches
`h2` section titles inside a `reference`-keyed part; this book documents a
builtin as an `h3` under a semantic group and a block kind in prose under a
topic heading. Both read better than a heading per symbol, and both are
invisible to that rule, so wiring it there would print a percentage far below
the truth on every run. The config says so.

It found two gaps on its first run, and both were bugs in the extractor rather
than in the book: `h5` is documented as part of "`h1` through `h6`", and
`project_meta` came from a `@block(...)` inside a doc comment. Both are handled.

### Prose

`/technical-writing review` over all 46 chapters, scanned for the catalogue's
checkable patterns. It came back essentially clean, which is worth stating
plainly rather than padding into findings:

| Pattern | Hits |
|---------|------|
| AI vocabulary | 1 (`additionally`, now `also`) |
| Fancy ways to say "is" — serves as, boasts, stands as | 0 |
| Filler — "in order to", "it is important to note" | 0 |
| "Not just X, but Y" | 0 |
| Excessive hedging | 0 |
| Title-case headings | 0 |
| Curly quotes | 0 |
| Doubled words | 0 |
| Sentences over 55 words | 2, both deliberate enumerations |

138 em dashes across 5,791 sentences, one per 42. The catalogue says avoid them
entirely; the skill's own overriding rule says not to make a sentence worse to
satisfy a rule. At this density they are ordinary editorial punctuation, not a
tell, so they stay.

## What is sound

- 46 toc entries, 46 pages, no unresolved page, no broken `source_file`, exit 0
  on HTML and on Markdown.
- 100% of the public surface documented, checked against the binary.
- No `{{TODO}}` anywhere.
- Heading depth stops at `h3` outside code samples.
- Every `[text](page)` cross-reference resolves.
- The toc is ordered and unnumbered, so inserting a chapter costs one line.

## Corrections to earlier runs

- The second run raised `Data views/mode-fit` on a count of 16 `h2` and 3 `h4`.
  That count included headings inside code samples. Stripped, the chapter is 10
  `h2` and 9 `h3` with no `h4`, which is unremarkable. Withdrawn.
- The `callout-kind` findings read as though 217 callouts were rendering
  unstyled. They were not. The conversion was worth doing for the build-error
  guarantee, not for a display fix.

## Not applicable, and why

Configured in `.agent/config.yaml` rather than dismissed, because each is a
statement about this book's shape.

- **Guide / Reference / Commands / Appendices parts.** Declared absent. The
  reference material lives inside the two subject parts: *Builtins* is the API
  reference, *The CLI* is the command reference. Appendices — a troubleshooting
  chapter and a glossary — are recorded as not written yet rather than declined.
- **Diagram coordinates** (49 findings). Turned off. Every one was in a chapter
  that teaches `x` / `y` placement and therefore has to show it.
- **The `forge` house theme and its metrics.** Overridden to `nord`, with
  `measure` as the only pinned metric and the rest inherited through `extends`.

## Still open

- **Appendices.** No troubleshooting chapter and no glossary. Declared absent
  with that reason, so it produces no finding, but it is the one part of the
  standard this book has not answered.
