# The constructor DSL — what does writing a fundamental actually look like?

Type: prototype
Status: resolved
Blocked by: 05

## Question

Ticket 02 chose **Model B, a terse WCL element DSL**, and handed its design to ticket 05. Ticket 05
settled its **scope** — one DSL across all three vocabularies (the HTML element vocabulary, the content
IR, `SvgFundamental`) — and explicitly did **not** settle its surface syntax. This ticket does.

It is a prototype ticket because the question is "how should it read", and the last two attempts at
answering it by argument both produced something worse than what they replaced.

### Why this can't be waved into the spec

**02's own measurement of Model B is inflated, and its known weakness is unsolved.**

- 02 measured the DSL at **25 lines → 8** for `website_header`. Ticket 05 then established that
  `tui.wcl:33` is factually wrong — **optional variant fields already default to `none`** (verified by
  `wcl eval` on a literal omitting two optionals) — and that the stdlib carries **204 redundant
  `: none` arguments** written for nothing. `website_header` alone has six. So an unknown but
  material fraction of that 25→8 win was **already free**, and the honest remaining cost is the
  `HtmlFundamental::Element { tag: … children: … }` ceremony. The DSL must be justified against the
  *deflated* number, not 02's.
- 02's prototype produced `a(sel_if(true, ".book-chapter", e.current, ".current"), { href: e.href },
  [txt(e.title)])` and 02 recorded it as **worse than what it replaced** and arguably worse than Model
  A's `class="book-chapter {% if e.current %}current{% endif %}"`. 02's conclusion, in its own words:
  *"roughly half of real markup noise is conditional class/attr composition and a constructor-only DSL
  does not touch it."*
- Ticket 05 solved **half** of that: `class` becomes a **none-dropping list**, so
  `class: ["book-chapter", if e.current { "current" }]` works and `sel_if` dies. **`attrs` has no
  answer yet** — it is `list<list<utf8>>` (name/value pairs), and a conditionally-present attribute
  (`aria-current`, `disabled`, `href` on a non-link) is the same problem one field over.

### Decide

- **The surface.** Per-tag constructors (`div(…)`, `a(…)`), one generic `el(tag, …)`, or something
  else. Note the DSL is **generic across three vocabularies** per 05 — the content IR is *field-shaped*
  (`Content::Code { source, language }`, nothing to shorten) while the HTML vocabulary is *tree-shaped*.
  A design that only serves the tree-shaped case has not met 05's scope decision.
- **Conditional attributes.** The unsolved half. Does `attrs` become none-dropping the way `class` did
  (`attrs: [["href", h], if x { ["aria-current", "page"] }]`)? Does it become a record? Something else?
- **Whether the DSL and the long form coexist.** Two legal ways to construct the same value is a
  vocabulary everyone must learn twice. Is the long form deprecated, mechanically migrated, or kept?
- **What it costs to be generic.** 05 chose one DSL over three because the alternative was "a generic
  design paid for by 4 SVG files". Show what the generic version does to the HTML case — if being
  generic makes the common case worse, that is a finding worth reporting back.

### What the prototype has to expose

Take the same real subjects 02 used, so the comparison is like-for-like:

- **`website_header`** (`lib/website.wcl:146`) — the 25-line `<header>`, **with its six dead `: none`s
  already deleted**, so the baseline is honest.
- **The book sidebar's recursive menu tree** — where conditional classes actually bite
  (`wdoc_part_menu_tree`).
- **One content-IR lower** and **one SVG lower** — the two vocabularies 02 never prototyped, and the
  ones that decide whether "generic" earns its keep. `charts.wcl` (18 `SvgFundamental::` uses) and one
  of the six markup-using content blocks are the natural picks.

Report line counts against the deflated baseline, and say plainly if the answer is "the two free fixes
carry most of the value and the DSL earns little" — 05 considered that outcome live and only ruled it
out on Wil's call for the generic version.

## Inherited from ticket 05 (resolved)

Settled — do not re-litigate:

- **Scope: one DSL across all three vocabularies** (HTML element vocabulary, content IR,
  `SvgFundamental`).
- **The two free fixes are in**: delete the 204 dead `: none`s; `class` becomes a none-dropping list.
- **The vocabularies themselves.** The fundamental layer splits into a **semantic content IR** (closed,
  ~15–20 variants, one per document concept, consumed by all four backends, matched exhaustively) and
  an **HTML element vocabulary** (templates and the HTML backend only). The content IR's Rust enum is
  **generated from its WCL union** in `build.rs`, so the DSL constructs the WCL side.
- **The content IR is closed** — there is no `Html{}` escape from content into markup. A DSL that
  quietly reintroduces one has broken decision 3.
- **Splicing is structural, not string surgery.** `HtmlFundamental::Children` is deleted (0 users) and
  both U+FFF9 sentinels go; the only authored splice concept is ticket 03's typed `slot`. The DSL does
  not need a splice primitive.

## Answer

**One generic `el` family over the HTML element vocabulary — and nothing else.** 05's three-vocabulary
scope narrows to one; the other two get a free union rename instead.

Prototyped at [`proto-15-constructor-dsl/`](../proto-15-constructor-dsl/) — every variant is
**executable against the shipped stdlib** (`import <wdoc.wcl>`, real fundamentals, real `toc_active` /
`chart_fmt` / `wdoc_part_menu_tree`). `run.py` proves the baseline copy is byte-identical to the live
`book_toc`, and that all three candidate authorings build the **same value** as today. The subjects are
the ones the ticket named: `website_header`, the recursive `book_toc`, `callout`, and
`chart_axes`/`gridlines`/`title`.

### The measurement, against the deflated baseline

Non-whitespace characters of authored code, comments stripped — lines only measure `wcl fmt`'s taste.

| subject | today | honest (free fixes) | per-tag | generic `el` |
|---|---:|---:|---:|---:|
| `website_header` | 732 | 637 | **342** | 391 |
| `book_toc` (recursive) | 1041 | 930 | **569** | 611 |
| chart axes/grid/title (SVG) | 963 | 864 | **635** | 635 |
| `callout` lower | 1087 | 998 | **740** | 748 |
| **total** | **3823** | **3429** | **2286** | **2385** |
| + DSL definitions, paid once | 0 | 0 | 1745 | 1088 |

**The ticket's suspicion was right but overstated.** The free fixes alone are **−11%**, not "most of
it" — 02's headline was inflated by about a quarter. The generic DSL is **−38%** on subject code,
break-even at **~14 sites**, against **258 real sites**. It earns its keep.

### Facts established while resolving (verified against `target/debug/wcl`, not by grep)

**The language constrains the surface far harder than either ticket assumed.**

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

So a constructor's arity is fixed at declaration and every call must fill it: there is no
`div(cls, kids)` that optionally takes an `id`, and anything needing one falls off to a 5-arg escape
passing `none` positionally — the exact ceremony the free fix just deleted.

**05's headline free fix does not parse.** `class: ["book-chapter", if e.current { "current" }]` is
rejected at the closing brace. The *list* side is genuinely free — `["a", none]` type-checks today and
evaluates to `["a", none]` — so **none-dropping is a consumer-side Rust change**, and an all-none list
must emit **no attribute**, not `class=""`.

**The ticket's "unsolved half" does not exist in the corpus.** Across every `.wcl` in the repo: **40**
`attrs: [` sites, **1** with any `if` in it (a conditional *value*, `website.wcl:152`), and **0**
conditionally-present attributes. 02's "roughly half of real markup noise is conditional class/attr
composition" is half right: the `class` half is real (**11** sites) and 05's none-dropping list kills
it; the `attr` half was hypothetical. The wireframe `disabled`/`checked` hits in Rust drive SVG
drawing, not HTML attributes.

**05 undercounted the construction corpus by 4×, and it is mostly not the stdlib.** 05 measured "57 of
63 `Element` uses" — that was `crates/wcl_wdoc/lib/` only. Repo-wide it is **258** `Element` sites and
**71** `SvgFundamental::*` sites, the largest single file being `docs/pages/wcl/landing-parts.wcl`
(51), then the scaffold templates (66) and the wskill projections (~56). Only 57 of 258 are stdlib.
**The DSL is a user-facing authoring surface, not an internal cleanup.**

**A DSL over the content IR is a regression.** `callout` as a `Content::Callout` variant is 392 chars
(**−64%** on today, versus the DSL's −31%); adding a constructor over it takes it back to 536
(**+37%**). The IR is field-shaped by construction — the "constructor" is the variant with its field
names deleted.

**"One DSL across three vocabularies" cannot be one thing.** `el` returns `HtmlFundamental` and WCL has
no generics (14 took syntax-only, no substitution), so the SVG family is a second parallel set of
names. Generic scope here can only mean *the same naming convention applied three times*.

**A positional constructor over a field-shaped variant manufactures this map's own failure mode.**
`SvgFundamental::Label` has seven fields, four `f64`. Verified in `probe-silent.wcl`:

```
long form, misspelled field        CAUGHT   wcl::eval::variant_shape_mismatch
DSL, two f64 args transposed       SILENT   font_size: 32, fit_width: 10
DSL, one argument short            CAUGHT   'slabel' expects 7 argument(s), got 6
```

Transposing `font_size` and `fit_width` renders the axis label at triple size in a third of its box and
nothing objects — because argument types are not checked at all. That is `_ => {}` and the stub `lower`
one level down, in an effort whose subject is an honest type system.

**A third option nobody had costed: shorten the union names.** 05 decision 11 regenerates these unions
from WCL anyway, so their names are in play. `HtmlFundamental::` is 17 characters; `Html::` is 6. On
the subjects that recovers **28% of the DSL's entire win with zero safety loss**; repo-wide, ~6,500
characters for a rename.

**SVG priced against that rename** — the measurement that decided the scope:

```
S3 today                                   963
+ free fixes                               864   ( -99)
+ union rename (free under 05 d.11)        765   ( -99)
+ positional constructors                  635   (-130)  <- the risky step
```

| | marginal saving over the rename | exposure |
|---|---:|---|
| HTML | 68/site × 258 = **~16,500 chars** | heterogeneous arg types (`utf8` / `list<utf8>` / `list<list<utf8>>` / `list<Html>`) — a transposition breaks the output loudly |
| SVG | 26/site × 71 = **~1,542 chars** | **56 of 71** sites pass 4–6 interchangeable `f64`s — a transposition is silent |

Ten times the payoff, opposite risk profile.

### The decisions

**1 — Scope narrows to the HTML element vocabulary.** This overrides 05 decision 7's "one DSL across
all three vocabularies", on the two measurements above: +37% on the content IR, and a 10×-worse
payoff-to-risk ratio on SVG. Wil first held HTML+SVG, then dropped SVG once it was priced against the
free rename.

*Rejected:* keeping all three (the content-IR regression is undeniable and the generic claim is
vacuous — it cannot be one function); constructors for the low-arity SVG shapes only (Polygon /
Polyline / Link, 15 of 71 sites — removes the exposure but keeps a fifth of a fifth of the win, i.e.
a vocabulary to learn for ~300 characters).

**2 — The surface is the generic `el` family, not per-tag constructors.**
`el(tag, cls, kids)` · `ela(tag, cls, attrs, kids)` · `eli(tag, id, cls, kids)`, plus the leaf helpers
`raw` / `inl` / `icon` / `para`. 1088 characters of definition, four names, break-even at 14 sites.

*Rejected:* per-tag (`div` / `span` / `ul` / `li` / `p` / `header` / `a` + a 5-arg `elem` escape). It
reads slightly better and measures 3% smaller on subject code, but costs **60% more definition** (1745)
and a dozen names, and its cliff is worse — the moment a block needs an `id` you write
`elem("div", c.id, cls, none, kids)`, positionally, which is what the free fix just deleted.

**3 — `SvgFundamental` and the content IR keep the named-field literal, and both unions get shortened
names.** Free under 05 decision 11 (they are generated), ~6,500 characters repo-wide, zero safety loss.
This is the answer for the field-shaped vocabularies: the verbosity there was never the field names,
it was the 17-character prefix in front of them.

**4 — `attrs` needs no design.** Zero conditionally-present attributes exist in the corpus. Make it
none-dropping for symmetry with `class` if that falls out of the same Rust change for free; do not
design for it, and do not treat it as an open question. This closes the "unsolved half" the ticket was
largely created to answer.

**5 — Add an else-less `if` to WCL, yielding `none`.** The one `wcl_lang` change this ticket asks for,
and it is independent of the DSL. Without it 05's headline `class` example is aspirational and each of
the 11 conditional-class sites carries 13 characters of `else { none }` ceremony. Note it is a pure
language feature, so it does not contend with ticket 14's extraction.

**6 — The long form stays legal; no deprecation, no mechanical migration.** It is the *only* form for
the two field-shaped vocabularies, so there is no "two ways to build the same value" problem to police
— the DSL is an HTML-only convenience layered over it, and its escape hatch is just writing the record.
The DSL itself needs **no language change**: it is plain WCL `let` bindings in wdoc's stdlib, which
also means `el` is a name in wdoc's namespace that user templates import.

### What this changes elsewhere

- **05 decision 7 is narrowed** (three vocabularies → one) and its corpus figure corrected (63 → 258).
- **A new mechanical migration sweep**: porting 258 `Element` sites to `el`, plus the union rename
  across 329 construction sites. It touches `docs/pages/wcl/landing-parts.wcl`, the scaffold templates
  and every wskill projection — the **same files** as ticket 13's CSS sweep and ticket 08's
  de-duplication, so it contends with them for ordering.
- **One `wcl_lang` change** (else-less `if`) that no other ticket asked for.

### Deliberately not decided here

- **Which unions get which short names.** `Html` / `Svg` / `Content` are the obvious picks but the
  namespace question belongs with 14's extraction, not here.
- **Whether the `el` family lives in wdoc's prelude or a separate importable part.** Stdlib packaging,
  not a decision.

Status: resolved
