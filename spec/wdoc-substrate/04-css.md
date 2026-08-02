# 04 — CSS authoring

Source: ticket [13](issues/13-css-authoring.md), validated by
[`proto-13-css-authoring/`](proto-13-css-authoring/).

**Position 3 — typed selectors, raw declarations.** Structure is WCL and typed; declaration bodies stay
CSS text. Every CSS heredoc dies.

## The census (measured; do not re-derive)

- **The scope is ~7× its original framing.** CSS is **27 heredocs across 27 stdlib files**, of which
  only **4** are template-level; the other 23 are block-level `stylesheet "wdoc-callout" { css = <<CSS }`
  blocks co-located with the block they style. **Plus 8 more outside the stdlib carrying 129 rules** —
  `docs/pages/wcl/landing-parts.wcl`, every wskill's `wdoc/book/main.wcl` and `wdoc/training/main.wcl`,
  and two in `.wad/`. **The real corpus is 477 rules.**
- **Nothing is computed.** **0 of 27** heredocs use `${…}` interpolation. Today's CSS is already inert
  text.
- **Selectors are the easy half.** bare `.class` 167 (48%) · descendant 64 · pseudo-class 32 · compound
  30 · at-rules 20 (`@font-face` ×16, `@media` ×3, `@keyframes` ×1) · pseudo-element 11 · the rest 25.
  Four shapes cover **293 of 349 (84%)**.
- **Properties are where "model the declarations" dies. 94 distinct, 74 outside the 22-property
  allowlist** — 4.7× growth, and the excess is exactly what the allowlist was built to exclude
  (`display` 63, `border-radius` 27, `position` 27, `height` 19, `gap` 17, `z-index` 10,
  `grid-template-columns` 7). Values lean on CSS's own functions: **`var()` 125 uses**, `rgba` 23,
  `color-mix` 8, `clamp` 6.
- **WCL has no map type** (`TypeRef` = Builtin/Named/Reference/List/Tensor/Function, `value.rs:534`).
  So declarations could only be ~94 typed fields (×3 with `dark`/`light` ⇒ ~282 declarations,
  permanently lagging CSS) or `list<list<utf8>>` — the same pair-list shape
  [02](02-blocks.md) §2.10.5 is trying to get rid of, reading worse than the CSS and checking nothing.
- **Nesting consolidates hard.** 252 selectors are one step (76%), 76 two, 20 three, nothing real
  deeper; 312 of 332 are single-branch. **`.book-sidebar` owns 18 rules**, `.site-nav` 14, `.ws-header`
  10. Six roots = 63 rules collapsing into six blocks.
- **Half the `Class` allowlist is dead.** 75 `class` instances repo-wide. **11 of 22 properties are
  never used anywhere.** Of the 11 that are used, **SVG paint is 61 of 86 uses (71%)** — `fill` 42,
  `stroke` 14, `opacity` 3, `stroke_width` 2. Rust never reads them structurally (`class_props`,
  `css.rs:26`, converts field → CSS text). The lone exception is **`accent`**, which renames to
  `--callout-accent` (`css.rs:118`).
- **WCL symbols cannot contain a hyphen** (`is_ident_cont`, `lexer.rs:592`) and **all 237 class names
  are hyphenated**. So [02](issues/02-template-authoring.md)'s winning argument — *a slot is a
  symbol, so the typo is already an error* — **cannot be carried across to CSS**. That argument has now
  failed twice (03 killed it for slots by scoping them per-declarer; this kills it for classes on
  lexing grounds).
- **A source-side cross-check can only ever see 43% of class uses.** Class names reach markup three
  ways: WCL `class:` field **76** distinct names, **Rust-generated markup 61**, raw-HTML strings inside
  WCL **39**. [02](02-blocks.md) keeps `Element`/`Raw` as template chrome and `@native` keeps Rust
  markup, so **neither blind channel goes away**.

---

## 4.1 Typed selectors, raw declarations

The class name is an identifier on the block; nesting covers descendant / pseudo / compound; at-rules
are blocks; the declaration body stays a CSS string.

*Rejected:* **full typed properties** (~282 field declarations, a list that lags CSS permanently, and
every value still an unchecked `utf8?` — so it buys name-checking only, at the cost of a schema nobody
can hold in their head). **A generic property bag** (no map type, so it reads as nested pair-lists
strictly worse than the CSS it replaces, and checks nothing).

## 4.2 The vocabulary

| block | covers | count |
|---|---|---|
| `class` | class-rooted rules, with `nest` for descendants/pseudo/compound | 243 |
| `base` | element and reset rules (`html`, `body,.wdoc-body`, `a,.link`, `blockquote`, `hr`, `kbd`, `::selection`) **and the ~9 tag-qualified roots** | 40 |
| `font_face` | pure data — family / src / weight / style / display | 16 |
| `media "…"` | wraps class blocks | 6 |
| `keyframes` | animation | 1 |

**The `@block("stylesheet")` type and its `css: utf8` heredoc are deleted.**

*Rejected:* keeping `stylesheet` for the residue (zero new vocabulary, but heredocs and the `//`
footgun survive in ~32 rules and authors must learn which mechanism to reach for); one generic
`rule "<selector>"` block (uniform and complete, but the class name goes back inside an unchecked
selector string, killing the only thing the block form buys).

## 4.3 `nest` takes a raw selector fragment — **and it needs `&`**

```wcl
nest "ul.book-toc > li::marker"   // descendant
nest "&.tip"                      // compound  — attaches directly
nest "&:hover"                    // pseudo-class
nest "&::before"                  // pseudo-element
```

One string covers every shape in the census including the awkward tail (combinators, `::marker`,
`::-webkit-scrollbar-thumb`) with no new vocabulary.

**`&` is a prototype amendment, and it is load-bearing.** `nest ".heading-1"` cannot express whether it
means `.parent .heading-1` (descendant) or `.parent.heading-1` (compound), and **both occur**. Without
`&`, **31 rules reconstruct wrongly and silently** — both readings are valid CSS. Caught only by the
round-trip; invisible to argument.

Nested class names are unchecked strings, which is acceptable **only because** §4.5 checks them from
the output anyway.

*Rejected:* structured `nest { tag =, class =, child = }` (keeps nested names first-class and
rename-refactorable, but needs vocabulary for combinators / pseudo-classes / pseudo-elements to reach
the tail, and buys checking §4.5 already provides); flat blocks carrying a whole selector (throws away
the 18-rules-into-one consolidation).

## 4.4 `Class` keeps the SVG paint set only

**Stay:** `fill` / `stroke` / `stroke_width` / `opacity` — 71% of all field use, and the case where the
shorthand genuinely reads better, since diagram shapes and chart series theme by class constantly.
**`accent` stays** because it renames rather than passing through.

**Deleted: the 11 never-used properties** (`underline`, `font_weight`, `line_height`, `text_align`,
`text_transform`, `letter_spacing`, `padding`, `margin`, `border`, `stroke_linejoin`, `stroke_linecap`)
**and the 6 thin text ones**; those **20 field uses migrate to `css`**. Applies across `Class` /
`ClassMode` / `ClassModeLight`.

This accepts two ways to write a declaration — [02](02-blocks.md) §2.10.6's warning — **scoped to the
slice that earns it**.

## 4.5 The cross-check is an output-scan lint at build, not a type

After rendering, extract every `class="…"` from the produced HTML and diff against the selectors wdoc
emitted. This is the only form that covers all three channels — **including the 61 Rust-generated names
nothing else can ever see** — and dynamic names arrive already resolved (`format("level-{}", 3)` shows
up as `level-3`, and there *is* a `.level-3` rule). Warning-level, both directions.

*Rejected:* a source-side reference check (precise spans and a hard error, but permanently blind to 57%
of uses, and it forbids `format("level-{}", h.level)` unless given an escape hatch that then also hides
real typos); symbols (impossible — hyphens don't lex); dropping the check (Wil kept it after seeing the
zero-yield measurement, on the user population the stdlib cannot measure).

### 4.5.1 The prototype falsified the lint as originally specified

Against a real 466-page build: **178 findings, 0 true positives.** Two causes, neither of which a
waiver list fixes:

- **84% of the typo direction (96 of 113) is `tok-*` / `language-*`**, emitted by syntect's scope
  generator — one class per Sublime scope, an open-ended vocabulary nothing will ever fully declare.
  Needs a **structural exemption for generator-emitted vocabularies**, not 96 waivers.
- **The dead-code direction is dominated by cross-site false positives.** The docs build exercises the
  book template, so every `site-*` / `ws-*` / `deck-*` rule reads as dead. The lint must run over the
  **union of all a document's sites**, never one build.

Strip both and **17 names** remain, every one a known unstyled hook — **still zero typos**.

**So the lint ships with the generator exemption and all-sites scope, and its value is entirely
prospective** — it protects users authoring their own templates, a population tickets
[03](03-templates.md) §3.5 and §3.8 create and which barely exists today.

## 4.6 All authored CSS leaves Rust; the generator stays

- **Moves out:** `theme.rs`'s `APPLY` (~84 static rules with exactly one substitution point,
  `{ACCENT_EXPR}` at `theme.rs:397`), `FONT_DEFAULTS` (one `:root` rule), and
  `assets/code-theme.css` (43 lines — the codebase's only sibling stylesheet).
- **Stays in Rust:** `ROLES` (a 30-entry role → CSS-var table), `palette_vars`, `resolve_roles` — they
  emit vars from WCL palette data rather than authoring rules.

This collapses the **duplicated `.tok-*` vocabulary** to one definition. After it, there is exactly one
place CSS is written, **which is what makes §4.5's lint trustworthy: it can see every rule.**

## 4.7 Migration is a throwaway script

A one-time uv single-file Python script (the `.wad/scripts/` precedent) parses the heredocs + `APPLY` +
`code-theme.css` with tinycss2, groups rules by root selector, and emits the block forms — including
the `.book-sidebar` 18-rule consolidation. Output reviewed and committed; **the script is not shipped**.
The ~20 selector-list rules and the `:root` accent line are hand-finished. Plus the schema prune from
§4.4.

See part [07](07-migration.md) for sequencing — **this sweep is not self-contained** and contends with
the others.

## Two things that fall out

**No CSS parser ships.** The lint doesn't need one — wdoc *generates* the selectors from the blocks, so
it already knows them, and the HTML side is a `class="…"` scan, not a parse. Declarations stay opaque
text end to end. The repo's no-parser-generators convention is never touched.

**The `//` footgun dies structurally.** Declarations are per-rule strings that wdoc wraps in generated
braces, so a stray `//` can corrupt at most its own rule — the documented "silently swallows every rule
after it" failure mode (`templates.wcl:363-367`, `presentation.wcl:106-110`) is **no longer
expressible**. A lint rejecting `//` inside a `css` value closes the rest.

## Prototype verdicts

`python3 run.py` in [`proto-13-css-authoring/`](proto-13-css-authoring/) converts the real corpus and
round-trips it.

- **Lossless — 477 rules in, 477 out, 0 lost, 0 spurious.** But only after the two amendments above
  (`&` in §4.3; dropping `tag` in §4.2), both invisible to argument.
- **It reads better.** 477 flat rules → **306 blocks**, 190 of them nested. `book-sidebar` goes
  **19 rules → 1 block**; `book_css` as a whole **41 flat rules → 14 blocks**.
- **The lint needed the two fixes in §4.5.1** before its waiver question is even reachable.

**Spec detail, not a decision:** `content: "▸"` is real CSS in `book_css`, so a raw declaration inside a
WCL string literal must escape its quotes (`css = "content: \"▸\";"`) — or `css` takes a raw heredoc.

## What this kills

27 CSS heredocs (+8 outside the stdlib) and the `@block("stylesheet")` type · 11 never-used `Class`
properties × 3 variants · ~128 rules of Rust-side and asset CSS · the duplicated `.tok-*` definitions ·
the two shouted `//`-footgun warning comments along with the footgun · **sibling `.css` files as a
model** (ruled out, not merely unchosen — see [08](08-open.md)).

## OPEN

- **What the lint's waiver looks like** — a field on the class block, a build-config list, or a naming
  convention for hook-only classes. Small surface, and it wants designing against a real lint run
  rather than against the 23 hooks found by grep.
- **Where the four new `@block` types sequence against [02](02-blocks.md)'s implementation.** They land
  on top of the settled type system, so they want to follow rather than precede — an implementation
  call, not a design one.
