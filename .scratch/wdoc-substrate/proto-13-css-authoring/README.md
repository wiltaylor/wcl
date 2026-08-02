# PROTOTYPE — ticket 13: the CSS block vocabulary

**Throwaway.** Answers three questions, then gets archived to a branch.

```bash
python3 run.py                     # extract, convert, round-trip, lint, report
python3 run.py --lint <site-dir>   # point the lint at a different built site
```

No deps. Runs against the **real** corpus, not a sample: every CSS heredoc in the
wdoc stdlib, `theme.rs`'s Rust string constants, `assets/code-theme.css`, and the
document-side heredocs in `docs/`, `examples/` and `.wad/`.

| | |
|---|---|
| [`out/all.wcl`](out/all.wcl) | all 477 rules in the proposed vocabulary |
| [`out/book.before.css`](out/book.before.css) → [`out/book.after.wcl`](out/book.after.wcl) | the worst case, side by side |

---

## Verdicts

### Q1 — Is the vocabulary lossless? **YES.**

```
original rules   477
round-tripped    477
lost             0
spurious         0
```

CSS → `class`/`base`/`font_face`/`media`/`keyframes` → CSS, diffed on
(selector, declarations). Every rule in the codebase reconstructs exactly.

**It took two vocabulary fixes to get there**, both found by the round-trip:

1. **`nest` needs SCSS's `&`.** `nest ".heading-1"` cannot say whether it means
   `.parent .heading-1` (descendant) or `.parent.heading-1` (compound), and both
   occur. Without `&`, 31 rules reconstructed wrongly — and *silently*, since both
   readings are valid CSS. So: `nest "&.tip"` / `nest "&:hover"` /
   `nest "&::before"` attach directly; anything else is a descendant.
2. **The `tag =` qualifier from decision 2 does not work.** `table.wdoc-table`'s own
   descendants use the **bare** class (`.wdoc-table th`), so a block-level tag
   wrongly narrows every nested rule too. Those ~9 tag-qualified roots want `base`,
   which decision 2 already gives them. **Drop `tag` from the vocabulary.**

One smaller spec detail: `content: "▸"` is real CSS in `book_css`, so a raw
declaration inside a WCL string literal has to escape its quotes
(`css = "content: \"▸\";"`). Either accept that, or let `css` take a raw heredoc.

### Q2 — Does it read better? **YES, and the corpus was bigger than the ticket said.**

```
477 flat rules  ->  306 blocks (190 of them nested inside a parent)
  class    243     base  40     font_face  16     media  6     keyframes  1
```

Biggest consolidations: `book-sidebar` **19 rules → 1 block**, `site-nav` 12,
`ws-header` 9, `wdoc-video` 7, `callout` 7. The worst case, `book_css`, goes from
**41 flat rules to 14 blocks**.

Note the line count *grows* (40 → 113) purely because the source packs each rule
onto one long line and the block form wraps declarations. The comparison that
matters is 41 top-level things to keep track of versus 14.

### Q3 — Does the output-scan lint work? **NO — not as specified.**

Run against a real 466-page build of the docs site:

```
distinct class names in the rendered HTML   342
distinct class names the rules define       294

USED, NO RULE   113     <- the typo check
RULE, NO USE     65     <- the dead-code check
```

**178 findings, 0 true positives.** Two distinct causes, and neither is a waiver
list:

- **84% of the typo direction (96 of 113) is `tok-*` / `language-*`** — emitted by
  syntect's scope generator, one class per Sublime scope. The vocabulary is
  open-ended and nothing will ever declare rules for all of it. This needs a
  *structural* exemption for generator-emitted vocabularies, not 96 waivers.
- **The dead-code direction is dominated by cross-site false positives.** The docs
  build exercises the book template, so every `site-*` (webpage), `ws-*` (website)
  and `deck-*` (presentation) rule reads as dead. The lint must run over the union
  of **all** a document's sites, never one build.

Strip both and the remainder is **17 names**, every one a known unstyled hook
(`book-nav-prev`, `callout-icon`, `term-cells`, `ws-main`, `wdoc-theme-dark`, …) —
the population the ticket already predicted, and still **zero typos**.

That doesn't kill decision 5, but it changes it: the lint needs the generator
exemption and all-sites scope *before* the waiver question is even reachable, and
its value stays entirely prospective — protecting user-authored templates, not
finding anything in the tree today.

---

## Correction to the resolution

The ticket scoped the migration to `crates/wcl_wdoc`, and the map records it as
"unusually self-contained". **That is wrong.** There are **8 more CSS heredocs
outside the stdlib carrying 129 rules** — `docs/pages/wcl/landing-parts.wcl` plus
every wskill's `wdoc/book/main.wcl` and `wdoc/training/main.wcl`, and two in `.wad/`.

So the real corpus is **477 rules, not 349**, and the sweep touches the docs site,
all four wskills and WAD — which means it *does* contend for the same files as the
`related` flip and the schema/template de-duplication. The map's sequencing note
has been corrected.
