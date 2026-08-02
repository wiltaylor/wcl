# How is template CSS authored — and does the `class` DSL grow to cover it?

Type: grilling
Status: resolved
Blocked by: —

## Question

Ticket 02 chose a WCL element DSL over external HTML files. Wil's follow-up: **why wouldn't CSS get
the same treatment?**

It partly already has, and the boundary is documented and measured.

**What exists.** wdoc has split CSS three ways:

- **Colour** → `theme` / `palette` blocks emitting `--wdoc-*` custom properties (`theme.wcl`). Data,
  and it works — a new template needs no theme work, it just references the vars.
- **Content + shape styling** → the `class` block (`Class`, `core.wcl:212`) with a stdlib of instances
  (`css-classes.wcl`). Data, and it works.
- **Layout / structure** → WCL heredocs emitted verbatim into a page `<style>` (`book_css`,
  `website_css`, `presentation_css`). Not data, and this is the painful part.

**Where the existing DSL stopped.** `css-classes.wcl`'s own doc comment:

> Only rules expressible as a single bare `.name` selector with allowlisted properties live here;
> selector-, at-rule-, or custom-property-dependent CSS stays in `render.rs`.

**Measured against `book_css` (41 rules):** 13 are bare `.name` selectors, **28 are not** —
`::-webkit-scrollbar-thumb`, `.book-sidebar ul.book-toc ul.book-toc`,
`li.book-branch > .book-toc-row > .book-toc-toggle::before`, `@media (max-width: 82rem)`, `:hover`.
Of the 13 that are bare, most use properties outside the `Class` allowlist anyway (`position`,
`overflow-y`, `flex-direction`, `grid-template-columns`, `inset`, `z-index`). The allowlist is text
styling plus SVG paint — built for content and diagram shapes, not page layout.

So the question is not "should CSS be a DSL" (it already is one) but **how far the DSL grows, and
whether the part it doesn't cover lives in a heredoc or a file.**

## The three positions

1. **Sibling `.css` file.** A template becomes a small folder. Kills the documented footgun — a stray
   `//` in the heredoc silently swallows every rule after it, warned about in identical shouted
   comments at `templates.wcl:710` and `presentation.wcl:106`. Buys **no checking**, and gives up the
   one thing that made the ticket-02 decision go the way it did.
2. **Grow the `class` DSL to full CSS** — combinators, pseudo-classes and -elements, at-rules,
   custom properties, `color-mix()` / `clamp()`. Unlike HTML, CSS has no small core: tags-and-attributes
   covers nearly all of HTML, while bare-selector-and-properties covers a third of one file. The
   existing implementation already declined this once, deliberately.
3. **Typed selectors, raw declarations.** Class names become **symbols**, shared between the element
   DSL and the CSS; the rule body stays CSS text. `div(".ws-headr", …)` becomes an error, and a rule
   nothing references becomes detectable dead code. Today's `class` block is a degenerate case of this.

## Why this is worth a ticket rather than a default

Position 3 is the one that carries ticket 02's own winning argument — *a slot is a symbol, so the typo
is already an error* — across to CSS, and it does so **without** modelling CSS. Neither a `.css` file
nor a heredoc can cross-check a class name against the markup that uses it. That cross-check is
plausibly worth more than the syntax question that prompted this.

Things to settle, not a menu:

- **Is the cross-check real leverage or theoretical?** How many class-name typos actually happen? The
  failure mode is a silently unstyled element, which is visible immediately in a browser — unlike
  `wdoc_region(c, "heor")`, which is invisible. If the browser already catches it, position 3's whole
  case weakens.
- **Which direction is checked?** Markup → CSS (every class used has a rule) is a different check from
  CSS → markup (every rule is used, i.e. dead-code detection), with different false-positive profiles.
  Dynamic class names (`format("level-{}", h.level)` in `book_onpage_link`) break both.
- **What about the 28 non-bare selectors?** Under position 3 they still name classes. Does the checker
  parse selectors far enough to extract the names, or is the declaration side opaque?
- **Does the `//` footgun survive?** It is the one concrete, documented, repeat-offender defect in this
  area. Position 1 kills it outright; 2 and 3 need their own answer.
- **What happens to `render.rs`'s remaining CSS constants** — the ones `css-classes.wcl` says stayed
  behind because they needed selectors or at-rules? They are the same problem one layer down.

## Inherited from ticket 02 (resolved)

- Templates are **WCL** (Model B element DSL), not external files. A sibling `.css` file would make a
  template a *folder* — the only place the chosen model reintroduces a non-WCL file, which is either a
  pragmatic exception or a smell depending on your read.
- **Ticket 02 originally decided position 1** as an in-scope call. **That decision is retracted** — it
  foreclosed position 3 without anyone weighing it. Nothing downstream depends on it yet.
- The reasoning that won 02 — *don't hand-write a second language when the type system already
  checks it* — cuts **both ways** here. Against position 2 (a CSS parser is a second language). For
  position 3 (symbols are already checked).

## Interacts with

**Ticket 05** (block type system). `Class` is a `@block` type in `core.wcl`, its instances are stdlib
data, and its property allowlist is a schema decision. Growing or retyping it is a change to the block
type system, not just to the styling story. Neither blocks the other, but whichever lands second
inherits.

---

## Resolution

**Position 3 — typed selectors, raw declarations — reached from the DSL side.** Structure is WCL and
typed; declaration bodies stay CSS text. Every CSS heredoc dies.

### Facts established while resolving (measured, not taken on report)

**The ticket's scope was ~7× its framing.** CSS is **27 heredocs across 27 stdlib files** — ~289
lines, ~240 rules — of which only **4** are template-level (`webpage` / `book` / `presentation` /
`website`). The other **23** are block-level `stylesheet "wdoc-callout" { css = <<CSS … }` blocks
co-located with the block they style. A `@block("stylesheet")` type with a `css: utf8` field and
`sites: list<symbol>?` scoping **already exists** (`core.wcl:60`). This is a stdlib-wide question, not
a corner of `templates.wcl`.

**Nothing is computed.** **0 of 27** CSS heredocs use `${…}` interpolation. Today's CSS is already
inert text, which is what makes moving it cheap and what makes "grow the DSL for expressiveness" buy
nothing anyone is currently reaching for.

**The whole CSS surface is 349 rules** (27 heredocs + `theme.rs` + `assets/code-theme.css`):

| selector shape | rules |
|---|---|
| bare `.class` — *all the current DSL covers* | **167 (48%)** |
| descendant | 64 |
| pseudo-class | 32 |
| compound `.a.b` | 30 |
| at-rule — `@font-face` ×16, `@media` ×3, `@keyframes` ×1 | 20 |
| pseudo-element | 11 |
| selector list / combinator / element / `tag.class` | 25 |

The tail is thin: bare + descendant + pseudo-class + compound = **293 of 349 (84%)** with four shapes.
**Selectors are the easy half.**

**Properties are where position 2 dies. 94 distinct, 74 outside the 22-property allowlist** — 4.7×
growth, and the excess is exactly what the allowlist was built to exclude: `display` 63,
`border-radius` 27, `position` 27, `height` 19, `width` 18, `gap` 17, `cursor` 17, `align-items` 16,
`box-sizing` 14, `z-index` 10, `grid-template-columns` 7, `transform` 7. Values lean on CSS's own
functions — **`var()` 125 uses**, `rgba` 23, `color-mix` 8, `clamp` 6.

**WCL has no map type.** `TypeRef` is `Builtin | Named | Reference | List | Tensor | Function`
(`value.rs:534`). So declarations could only be held as ~94 typed fields (×3 with `dark`/`light` ⇒
~282 field declarations, permanently lagging CSS) or as `list<list<utf8>>` — the same pair-list shape
ticket 15 is trying to get rid of for `attrs`, which reads worse than the CSS and checks nothing.
That is what forced the raw-declaration answer.

**307 of 349 rules (88%) are class-rooted on every branch.** The residue splits three ways: ~9
tag-qualified class rules (`svg.wdoc-icon`, `table.wdoc-table`, `ol.wdoc-list-numbered > li::marker`)
that an optional `tag =` absorbs, taking coverage to **~91%**; ~12 element/base rules (`html`,
`body,.wdoc-body`, `a,.link`, `blockquote`, `hr`, `kbd`, `::selection`), all but two of them in
`theme.rs`; and the 20 at-rules.

**Nesting consolidates hard.** Depth needed is shallow — 252 selectors are one step (76%), 76 are two,
20 are three, nothing real deeper — and selector lists are rare (312 of 332 single-branch, 94%). Roots
cluster: **`.book-sidebar` owns 18 rules**, `.site-nav` 14, `.ws-header` 10, `.wdoc-table` 8,
`.wdoc-video` 7, `.code-card` 6. Six roots = 63 rules collapsing into six blocks. Today those 18
`.book-sidebar` rules are 18 unrelated lines in a 41-line wall.

**Half the `Class` allowlist is dead, and the rest is a passthrough alias table.** 75 `class`
instances repo-wide (13 use `dark`/`light`, 1 uses `sites`). **11 of the 22 properties are never used
anywhere** — `underline`, `font_weight`, `line_height`, `text_align`, `text_transform`,
`letter_spacing`, `padding`, `margin`, `border`, `stroke_linejoin`, `stroke_linecap`. Of the 11 that
are used, **SVG paint is 61 of 86 uses (71%)**: `fill` 42, `stroke` 14, `opacity` 3, `stroke_width` 2.
Text styling is thin — `color` 7, `font_size` 6, `bold` 4, `italic` 1, `font_family` 1, `background` 1.
**Rust never reads them structurally**: `class_props` (`css.rs:26`) converts field → CSS text and PDF
consumes the resulting *string* (`pdf/mod.rs:243`). The lone exception is **`accent`**, which emits
`--callout-accent` (`css.rs:118`) — a rename, and the only field doing something `css = "…"` couldn't.

**WCL symbols cannot contain a hyphen.** `is_ident_cont` is alphanumeric-or-underscore
(`lexer.rs:592`), and **all 237 class names are hyphenated** (`class "book-sidebar"` works only
because `name: identifier` accepts a quoted string). So **ticket 02's winning argument — *a slot is a
symbol, so the typo is already an error* — cannot be carried across to CSS**, short of renaming 237
classes or extending the lexer. That argument has now failed **twice**: ticket 03 killed it for slots
by scoping them per-declarer, and this ticket kills it for classes on lexing grounds.

**A source-side cross-check can only ever see 43% of class uses.** Class names reach markup three ways,
counted as distinct names: WCL `class:` field **76**, **Rust-generated markup 61**
(`push_str("<span class=\"wdoc-video-play\"")`, `vec!["wdoc-table".to_string()]`), raw-HTML strings
inside WCL **39** (`html: "<div class=\"deck-progress\">…"`). Ticket 05 confirms `Element`/`Raw`
survive as template chrome (57 of 63 `Element` and 38 of 45 `Raw` uses are in the three template
files) and `@native` blocks keep generating Rust markup, so **neither of the two blind channels is
going away**.

**The cross-check's measured yield on today's tree is zero.** markup → CSS flagged 23 names with no
rule; I checked eight and **none is a typo** — they are structural hooks (`ws-main`, `book-nav-prev`,
`wdoc-site-index`), a JS/DOM pairing (`term-cells` ↔ `data-term-cells`) and Bootstrap-Icons names
(`bi`, `bi-house`). CSS → markup flagged 93 unused rules, overwhelmingly composition artifacts a
static checker can't follow: `flatten([["callout"], cls])`, `vec!["wdoc-table".to_string()]`, the eight
`wdoc-series-*` living in a list constant at `charts.wcl:30`, and class names passed as **plain string
arguments** (`website_region_section(c, "hero", "section", "ws-hero")`). Only **3** dynamic
constructions exist in WCL total, so dynamism is not what defeats it — ordinary composition is.
**Caveat that changed the call:** every number here is over the *stdlib* — 4 templates and 23
stylesheets, one careful author, reviewed. The population the check protects is *users* authoring
their own templates, which barely exists today and which tickets 02 and 12 create. "0 typos" is partly
a selection effect.

**`theme.rs` splits cleanly.** `APPLY` is **~84 static rules with exactly one substitution point**
(`{ACCENT_EXPR}`, `theme.rs:397`). `palette_vars` / `ROLES` (a 30-entry role → CSS-var table) /
`resolve_roles` are a *generator* over WCL palette data (`lib/theme.wcl`), not authored CSS.
`FONT_DEFAULTS` is one `:root` rule. `assets/code-theme.css` is 43 lines and is the codebase's only
sibling `.css` file.

### The decisions

**1 — Typed selectors, raw declarations.** Structure is WCL — the class name is an identifier on the
block, nesting covers descendant / pseudo / compound, at-rules are blocks — while the declaration body
stays a CSS string. This is the ticket's position 3, arrived at from position 2's direction once the
property census and the missing map type made "model the declarations" untenable.

*Rejected:* **full typed properties** (~282 field declarations across `Class`/`dark`/`light`, a list
that lags CSS permanently, and every value still an unchecked `utf8?` — so it buys name-checking only,
at the cost of a schema nobody can hold in their head). **Generic property bag** (no map type, so it
reads as nested pair-lists strictly worse than the CSS it replaces, and checks nothing).

**2 — Dedicated blocks cover the residue; heredocs die entirely.** `font_face` (16 rules, pure data —
family / src / weight / style / display), `base` for element and reset rules (~12), `media "…"`
wrapping class blocks (3), `keyframes` (1). `class` gains an optional `tag =` qualifier for the ~9
tag-narrowed rules. **The `@block("stylesheet")` type and its `css: utf8` heredoc are deleted.**

*Rejected:* keeping `stylesheet` for the residue (zero new vocabulary, but heredocs and the `//`
footgun survive in ~32 rules and authors must learn which mechanism to reach for). One generic
`rule "<selector>"` block (uniform and complete, but the class name goes back inside an unchecked
selector string, killing the only thing the block form buys).

**3 — `Class` keeps the SVG paint set only.** `fill` / `stroke` / `stroke_width` / `opacity` stay —
71% of all field use, and the case where the shorthand genuinely reads better, since diagram shapes and
chart series theme by class constantly. **`accent` stays** because it renames to `--callout-accent`
rather than passing through. The **11 dead properties and the 6 thin text ones are deleted**; those 20
uses migrate to `css`. This accepts two ways to write a declaration — ticket 15's warning — scoped to
the slice that earns it.

**4 — `nest` takes a raw selector fragment.** `nest "ul.book-toc > li::marker"`, appended to the
parent selector. One string covers every shape in the census including the awkward tail (combinators,
`::marker`, `::-webkit-scrollbar-thumb`) with no new vocabulary. Nested class names are unchecked
strings, which is acceptable **only because** decision 5 checks them from the output anyway.

*Rejected:* structured `nest { tag =, class =, child = }` (keeps nested class names first-class and
rename-refactorable, but needs vocabulary for combinators / pseudo-classes / pseudo-elements to reach
the tail, and buys checking decision 5 already provides). Flat blocks carrying a whole selector
(throws away the 18-rules-into-one consolidation and makes the root indistinguishable from the rest of
the selector).

**5 — The cross-check is an output-scan lint at build time, not a type.** After rendering, extract
every `class="…"` from the produced HTML and diff against the selectors wdoc emitted. This is the only
form that covers all three channels — **including the 61 Rust-generated names nothing else can ever
see** — and dynamic names arrive already resolved (`format("level-{}", 3)` shows up as `level-3`, and
there *is* a `.level-3` rule). Warning-level, both directions (used-without-rule, rule-without-use),
with a waiver for the ~23 deliberate unstyled hooks.

*Rejected:* a source-side reference check (precise spans and a hard error, but permanently blind to
57% of uses, and it forbids `format("level-{}", h.level)` unless given an escape hatch that then also
hides real typos). Symbols (impossible — hyphens don't lex). Dropping the check (Wil kept it in
question 1 after seeing the zero-yield measurement, on the user population the stdlib can't measure).

**6 — All authored CSS leaves Rust; the generator stays.** `APPLY` (~84), `FONT_DEFAULTS` (1) and
`assets/code-theme.css` (43 lines) become `class` / `base` / `font_face` blocks. `ROLES` /
`palette_vars` / `resolve_roles` stay in Rust — they emit vars from WCL palette data rather than
authoring rules. Collapses the **duplicated `.tok-*` vocabulary** to one definition. After this there
is exactly one place CSS is written, which is what makes decision 5's lint trustworthy: it can see
every rule.

**7 — Migration is a throwaway CSS-parsing script.** A one-time uv single-file Python script (the
`.wad/scripts/` precedent) parses the 27 heredocs + `APPLY` + `code-theme.css` with tinycss2, groups
rules by root selector, and emits the block forms — including the `.book-sidebar` 18-rule
consolidation. Output reviewed and committed; the script is not shipped. The ~20 selector-list rules
and the `:root` accent line are hand-finished.

### Two things that fall out

**No CSS parser ships.** The lint doesn't need one — wdoc *generates* the selectors from the blocks, so
it already knows them, and the HTML side is a `class="…"` scan, not a parse. Declarations stay opaque
text end to end. The repo's no-parser-generators convention is never touched, which was the strongest
argument against position 2 and does not apply to what was actually chosen.

**The `//` footgun dies structurally.** Declarations are per-rule strings that wdoc wraps in generated
braces, so a stray `//` can corrupt at most its own rule — the documented "silently swallows every rule
after it" failure mode (`templates.wcl:363-367`, `presentation.wcl:106-110`) is no longer expressible.
A lint rejecting `//` inside a `css` value closes the rest.

### What this kills

- **27 CSS heredocs** and the `@block("stylesheet")` type.
- **11 never-used `Class` properties**, × `Class` / `ClassMode` / `ClassModeLight`.
- **~128 rules of Rust-side and asset CSS** (`APPLY`, `FONT_DEFAULTS`, `code-theme.css`).
- The **duplicated `.tok-*` definitions**.
- The two shouted `//`-footgun warning comments, along with the footgun.
- **Sibling `.css` files as a model** — ruled out, not merely unchosen. `code-theme.css`, the only one
  in the codebase, folds in. It buys editor support for free (CodeMirror already maps `css`,
  `EditorPane.jsx:18`) but at 23 files of ~11 lines each it fragments the co-location that commit
  `5ee5d88f` deliberately created, and it checks nothing.

### Deliberately not decided here

- **What the lint's waiver looks like** — a field on the class block, a build-config list, or a naming
  convention for hook-only classes. Small surface, and it wants designing against a real lint run
  rather than against the 23 hooks found by grep.
- **Where the new block types sequence against ticket 05's implementation.** They are four `@block`
  types landing after 05 settled the block type system; whether they ride with 05's work or follow it
  is an implementation call, not a design one.

### Validated by prototype — [`proto-13-css-authoring/`](../proto-13-css-authoring/)

`python3 run.py` converts the **real** corpus into the vocabulary and round-trips it
back to CSS. Three verdicts, two of which amend decisions above.

**Q1 — the vocabulary is lossless. 477 rules in, 477 out, 0 lost, 0 spurious.** But it
took two fixes to get there, both invisible to argument and caught only by the round-trip:

- **`nest` needs SCSS's `&`** (amends decision 4). `nest ".heading-1"` cannot express
  whether it means `.parent .heading-1` (descendant) or `.parent.heading-1` (compound),
  and **both occur**. 31 rules reconstructed wrongly, and *silently* — both readings are
  valid CSS. So `nest "&.tip"` / `"&:hover"` / `"&::before"` attach directly; anything
  else is a descendant.
- **The `tag =` qualifier does not work** (amends decision 2). `table.wdoc-table`'s own
  descendants use the **bare** class (`.wdoc-table th`), so a block-level tag wrongly
  narrows every nested rule. Those ~9 roots want `base`, which decision 2 already gives
  them. **Drop `tag` from the vocabulary.**
- Smaller: `content: "▸"` is real CSS in `book_css`, so a raw declaration inside a WCL
  string literal must escape its quotes (`css = "content: \"▸\";"`) — or `css` takes a
  raw heredoc. A spec detail, not a decision.

**Q2 — it reads better.** 477 flat rules → **306 blocks** (243 `class`, 40 `base`, 16
`font_face`, 6 `media`, 1 `keyframes`), 190 of them nested. `book-sidebar` goes **19
rules → 1 block**; `book_css` as a whole goes **41 flat rules → 14 blocks**.

**Q3 — the output-scan lint does NOT work as specified** (amends decision 5). Against a
real 466-page build: **178 findings, 0 true positives.** Two causes, neither of which a
waiver list fixes:

- **84% of the typo direction (96 of 113) is `tok-*` / `language-*`**, emitted by
  syntect's scope generator — one class per Sublime scope, an open-ended vocabulary
  nothing will ever fully declare. Needs a **structural exemption for
  generator-emitted vocabularies**, not 96 waivers.
- **The dead-code direction is dominated by cross-site false positives.** The docs build
  exercises the book template, so every `site-*` / `ws-*` / `deck-*` rule reads as dead.
  The lint must run over the **union of all a document's sites**, never one build.

Strip both and **17 names** remain, every one a known unstyled hook — the population the
ticket predicted, and still **zero typos**. Decision 5 survives, but it needs the
generator exemption and all-sites scope *before* the waiver question is reachable, and
its value stays entirely prospective.

### Correction to decision 7's scope

The migration is **not** confined to `crates/wcl_wdoc`. There are **8 more CSS heredocs
outside the stdlib carrying 129 rules** — `docs/pages/wcl/landing-parts.wcl` plus every
wskill's `wdoc/book/main.wcl` and `wdoc/training/main.wcl`, and two in `.wad/`. The real
corpus is **477 rules, not 349**, and the sweep touches the docs site, all four wskills
and WAD — so it *does* contend for the same files as the `related` flip and the
schema/template de-duplication.

### Incidental defects found (file separately)

1. **`crates/wcl_wdoc/src/render/css.rs:1-13` is factually wrong.** It states *"The lone Rust-side CSS
   that remains is `highlight::theme_css()`"*; `theme.rs`'s `APPLY` is ~84 hand-written rules covering
   headings, links, code cards, book chrome, syntax tokens and every diagram shape. *Decision 6
   dissolves this along with the CSS it misdescribes — fix only if the refactor slips.*
2. **The syntax-token classes are defined twice.** `.tok-comment`, `.tok-keyword`, `.tok-string`,
   `.tok-type` and the rest appear in `assets/code-theme.css` *and* again in `theme.rs`'s `APPLY` as
   `var(--wdoc-syn-*)` versions — two mechanisms, two files, one vocabulary. *Decision 6 resolves this;
   don't file.*
