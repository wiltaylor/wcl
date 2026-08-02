# PROTOTYPE — ticket 15: what does writing a fundamental actually look like?

**Throwaway.** Answers one question, then gets archived to a branch.

```bash
python3 run.py     # no deps; uses target/debug/wcl against the real wdoc stdlib
```

Everything here is **executable against the shipped stdlib** — `import <wdoc.wcl>`,
real `HtmlFundamental` / `SvgFundamental`, real `toc_active` / `chart_fmt` /
`wdoc_part_menu_tree`. `run.py` proves the V0 copy is byte-identical to the live
`book_toc`, then proves every variant builds the **same value** as today.

| | file | what it is |
|---|---|---|
| V0 | [`v0-today.wcl`](v0-today.wcl) | today, verbatim |
| V1 | [`v1-honest.wcl`](v1-honest.wcl) | ticket 05's two free fixes and nothing else — **the honest baseline** |
| V2 | [`v2-pertag.wcl`](v2-pertag.wcl) | per-tag constructors — `div(cls, kids)`, `a(href, cls, kids)` |
| V3 | [`v3-generic.wcl`](v3-generic.wcl) | one generic `el(tag, cls, kids)` + `ela` / `eli` |
| V4 | [`v4-content-ir.wcl`](v4-content-ir.wcl) | the control — `callout` under 05's content IR |
| — | [`probe-silent.wcl`](probe-silent.wcl) | the safety probe |

Subjects, as the ticket specified: `website_header`, the recursive book sidebar
(`book_toc` + `book_toc_link`), a content block reaching for markup (`callout`),
and an SVG lower (`chart_axes` / `chart_gridlines` / `chart_title`).

---

## The measurement, against the deflated baseline

Non-whitespace characters of authored code, comments stripped. Characters not
lines: a line count only measures `wcl fmt`'s taste.

| subject | V0 today | V1 honest | V2 per-tag | V3 generic |
|---|---:|---:|---:|---:|
| S1 `website_header` | 732 | 637 | **342** | 391 |
| S2 `book_toc` (recursive) | 1041 | 930 | **569** | 611 |
| S3 chart axes/grid/title (SVG) | 963 | 864 | **635** | 635 |
| S4 `callout` lower | 1087 | 998 | **740** | 748 |
| **subject total** | **3823** | **3429** | **2286** | **2385** |
| + DSL definitions, paid once | 0 | 0 | 1745 | 1088 |

- **The free fixes alone are −11%.** So 02's headline was inflated, as ticket 15
  suspected — but only by about a quarter, not by most of it.
- **The generic DSL is −38% on subject code**, break-even at **~14 sites**, and
  there are **258 `HtmlFundamental::Element` sites repo-wide** — a projected
  ~19,000 characters. It earns its keep.
- **Per-tag buys 3 more percentage points for 60% more definition** (1745 vs
  1088) and a vocabulary of a dozen names to learn. Not worth it.

---

## Findings

### 1. ⚠ The ticket's "unsolved half" does not exist in the corpus

15 was created largely to answer conditional *attributes*. Measured across every
`.wcl` in the repo:

```
`attrs: [` sites                        40
... with any `if` in them                1     (a conditional VALUE, website.wcl:152)
... conditionally-PRESENT attribute      0     <- the unsolved half
conditional `class` sites               11
```

02's claim that "roughly half of real markup noise is conditional class/attr
composition" is **half right**. The `class` half is real — 11 sites — and ticket
05's none-dropping list already kills it. The `attr` half has **zero instances**.
`aria-current` / `disabled` / `href`-on-a-non-link were hypothetical; the wireframe
`disabled` / `checked` hits in Rust drive SVG drawing, not HTML attributes.

**So `attrs` needs no answer.** Make it none-dropping for symmetry with `class` if
that falls out for free; do not design for it.

### 2. ⚠ "One DSL across all three vocabularies" cannot be one thing, and is wrong in two of the three

05 decision 7 scoped the DSL over the HTML vocabulary, `SvgFundamental` and the
content IR. Measured:

- **HTML, tree-shaped** — pays clearly. 258 sites, ~79 chars/site, break-even ~14.
- **SVG, field-shaped** — pays *numerically* (−26%) and **regresses on safety**;
  see finding 3.
- **Content IR** — a constructor DSL over it is a **+37% regression** (392 → 536
  chars). The IR is field-shaped by construction, so there is nothing to shorten;
  the "constructor" is the variant with its field names deleted.

And it cannot be one *function* in any case: `el` returns `HtmlFundamental`, WCL
has no generics (14 took syntax-only, no substitution), so the SVG family is a
second parallel set of names. **"Generic across three vocabularies" can only ever
mean "the same naming convention applied three times."** The generic scope 05
chose over Wil's objection to "a generic design paid for by 4 SVG files" turns out
to cost nothing because it buys nothing.

**Recommendation: narrow decision 7 to the HTML element vocabulary only.**

### 3. ⚠ A positional constructor over a field-shaped variant manufactures this map's own failure mode

`SvgFundamental::Label` has seven fields, four of them `f64`. Run
[`probe-silent.wcl`](probe-silent.wcl):

```
long form, misspelled field        CAUGHT   wcl::eval::variant_shape_mismatch
DSL, two f64 args transposed       SILENT   font_size: 32, fit_width: 10
DSL, one argument short            CAUGHT   'slabel' expects 7 argument(s), got 6
```

Transposing `font_size` and `fit_width` renders the axis label at triple size in a
third of its box. Nothing objects — because **WCL does not check argument types at
all** (verified: `elem("div", none, none, [])` against a param declared
`identifier` evaluates clean). Arity is the only positional mistake the language
catches.

That is `_ => {}` and the stub `lower` one level down: a silent wrong answer where
the shape it replaced gave an error. In an effort whose whole subject is an honest
type system, it is the wrong trade — and the char count cannot see it.

### 4. The language constrains the surface much harder than either ticket assumed

All verified against `target/debug/wcl`:

| | |
|---|---|
| named arguments | **no** — `Call { args: Vec<Expr> }`, positional only |
| default arguments | **no** |
| variadics | **no** |
| optional parameter types | **PARSE ERROR** — `?` in a param list is rejected; **0** in the whole stdlib |
| argument type checking | **none** — `none` passes through a param declared `identifier` |
| arity checking | yes |
| options-record parameter | **no** — a bare record's missing fields are an *unresolved reference*, not `none` |
| else-less `if` | **PARSE ERROR** |

Two consequences the spec has to carry:

- **A constructor's arity is fixed at declaration and every call must fill it.**
  There is no `div(cls, kids)` that optionally takes an `id`; you fall off to a
  5-arg escape (`elem("div", c.id, cls, none, kids)`) and pass `none` positionally
  — the exact ceremony the free fix just deleted. V2's `callout` shows it.
- **05's "free fix" `class: ["book-chapter", if e.current { "current" }]` does not
  parse.** It needs either a language change (else-less `if` yielding `none`) or
  authors writing `else { none }` — 13 characters of ceremony on each of the 11
  conditional-class sites. The list side is genuinely free: `["a", none]`
  type-checks today and evaluates to `["a", none]`, so **dropping is a
  consumer-side Rust change**, which is what `run.py` normalises to compare
  variants. Note an all-none list must become *no class attribute*, not `class=""`.

### 5. ⚠ 05 undercounted the corpus by 4×, and it is mostly not the stdlib

05 measured "57 of 63 `Element` uses in `templates.wcl` / `website.wcl` /
`presentation.wcl`" — that was `crates/wcl_wdoc/lib/` only. Repo-wide:

```
258  HtmlFundamental::Element sites        71  SvgFundamental::* sites
  51  docs/pages/wcl/landing-parts.wcl        <- user code
  36  crates/wcl_wdoc/lib/templates.wcl
  36  crates/wcl/src/scaffold/templates/website.wcl
  30  crates/wcl/src/scaffold/templates/wskill.wcl
  17  docs/wskills/wdoc/wdoc/training/main.wcl
  15  crates/wcl_wdoc/lib/website.wcl
```

Only 57 of the 258 are in the stdlib. The rest are scaffold templates, the docs
landing page and the wskill projections. **The DSL is a user-facing authoring
surface, not an internal cleanup** — which raises what it is worth, and means it
lands in the migration sweep alongside every wskill (the same files ticket 13's
CSS sweep and the `related` flip already contend for).

### 6. The content IR does the heavy lifting, not the DSL

`callout` is one of the six content blocks that reach for markup. Under 05
decision 1 it stops building `<div>`s at all:

| | chars |
|---|---:|
| today, HTML tree | 1087 |
| as a `Content::Callout` variant | **392** (−64%) |
| ... with a constructor DSL over it | 536 (+37%) |

−64% from the split versus −31% from the DSL, on the same subject. Worth saying
plainly in the spec: **for the six markup-using content blocks the constructor
question is moot** — their trees are deleted, not shortened.

### 7. A third option nobody costed: just shorten the union names

05 decision 11 regenerates these unions from WCL anyway, so their names are in
play. `HtmlFundamental::` is 17 characters; `Html::` is 6.

```
prefix occurrences in the honest subjects   22 Html + 5 Svg
chars recovered by renaming                297
the generic DSL's saving over honest      1044
→ renaming alone buys 28% of the DSL's win, with ZERO safety loss
```

Repo-wide that is roughly 6,500 characters for a rename. It is not an alternative
to the DSL for the HTML case, but it **is** the honest answer for the SVG case,
where it delivers a third of the win without finding 3's silent transposition.

---

## Finding 8 — SVG, priced against the rename it gets for free

Once the union rename is taken (free under 05 decision 11), the SVG constructors
add very little and expose a lot:

```
S3 today                                   963
+ free fixes                               864   ( -99)
+ union rename (free under 05 d.11)        765   ( -99)
+ positional constructors                  635   (-130)  <- the risky step
```

| | marginal saving over the rename | exposure |
|---|---:|---|
| HTML | 68/site × 258 = **~16,500 chars** | heterogeneous arg types — a transposition breaks the output loudly |
| SVG | 26/site × 71 = **~1,542 chars** | **56 of 71** sites pass 4–6 interchangeable `f64`s — a transposition is silent |

Ten times the payoff, opposite risk profile. That is what decided the scope.

---

## Decided (with Wil, in session)

1. **Do the two free fixes**, and **add an else-less `if` to WCL** (yielding
   `none`) so 05's headline `class: ["book-chapter", if e.current { "current" }]`
   is true rather than aspirational. The none-dropping itself is a Rust-side
   change, and an all-none list must emit **no attribute**, not `class=""`.
2. **Adopt V3, the generic `el` family, for the HTML element vocabulary only** —
   `el(tag, cls, kids)`, `ela(tag, cls, attrs, kids)`, `eli(tag, id, cls, kids)`,
   plus the leaf helpers `raw` / `inl` / `icon` / `para`. 1088 characters of
   definition, break-even at 14 sites, 258 available.
3. **`SvgFundamental` and the content IR keep the named-field literal.** Both
   unions get shortened names instead — free under 05 decision 11, ~6,500
   characters repo-wide, zero safety loss. This narrows 05 decision 7.
4. **`attrs` needs no design.** Zero conditionally-present attributes exist.
5. **The long form stays legal** and is the only form for the two field-shaped
   vocabularies, so there is no deprecation and no mechanical migration — the DSL
   is an HTML-only convenience layered over it, and the escape hatch is just
   writing the record.

Per-tag was rejected: 3% smaller on subject code for 60% more definition (1745 vs
1088) and a dozen names, plus a worse cliff (`elem("div", c.id, cls, none, kids)`).

The DSL itself needs **no language change** — it is plain WCL `let` bindings in
wdoc's stdlib. The else-less `if` is the one `wcl_lang` change, and it is
independent of the DSL.
