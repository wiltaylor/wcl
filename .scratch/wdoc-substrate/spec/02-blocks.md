# 02 — The block type system

Source: tickets [05](../issues/05-block-type-system.md) and [15](../issues/15-constructor-dsl.md),
resting on the survey in [04](../issues/04-backend-survey.md).

## The problem, measured

- **134** `@block` types in the wdoc stdlib — 43 `extends WdocBlock`, 30 `extends SvgBlock`.
- **57** declare a stub `lower` returning `[]` while Rust intercepts them entirely (24 returning
  `HtmlFundamental`, 33 returning `SvgFundamental`). `render_block` runs the Rust `match` **first**
  (`render/html.rs:532`), so a stub is never called. **The stub `lower`s are dead code** that exist
  only to satisfy an interface.
- **2** `lower_svg` fns (`sequence.wcl:279`, `statechart.wcl:404`) exist solely because the interface
  cannot express "this block draws SVG at page level".
- **Each of the three code backends runs its own Rust `match` on `block.kind()`, and the sets differ.**
  `kinds.rs:1-8` asserts they "special-case the same block vocabulary" — factually wrong. HTML-only:
  `column`, `li`, `markdown_source`, `edit_object`. Markdown-only: `code`. `callout`: PDF + Markdown,
  **not** HTML. `file`: HTML + Markdown, **absent from PDF entirely**.
- **Root cause: `lower_recurse` exists only in HTML** (`render/lower.rs:317`). `callout` has a real WCL
  `lower`, so HTML follows it and needs no special case, while PDF and Markdown hand-reimplement it in
  Rust (`pdf/collect.rs:137`, `markdown/emit.rs:200`). **Consequence: the WCL extension mechanism is
  HTML-only in practice** — a user block whose `lower` returns another custom variant works in the book
  and silently renders nothing anywhere else.
- **Exhaustive matching is impossible today.** `HtmlFundamental` exists only as a WCL union; Rust
  consumes it via `as_record_variant(value)` matching on **string** variant names. Which is why every
  walker ends in `_ => {}` (`emit.rs:332`), and why a variant nobody handles is silence.
- **Markdown reverse-engineers semantics out of markup.** `emit.rs:292` dispatches `element` by *tag
  string*; a heading survives as a `Paragraph` whose **CSS class** carries the level —
  `heading_level()` parses `"heading-2"` (`render/accessors.rs:321`).

Three facts that reframed the design and are easy to get wrong:

- **`code` and `math` are NOT stubs.** They have real `lower`s emitting `HtmlFundamental::Highlighted`
  / `::Math` — leaf variants meaning "Rust computes this bit", composed into WCL-built structure.
  **`CLAUDE.md`'s feature map lists them among the Rust special-cased blocks and is wrong.** Two
  mechanisms for "Rust owns the hard part" already exist and one of them is honest.
- **Templates are HTML-only.** PDF and Markdown never run one; they read `default_template` purely as a
  visibility flag (`pdf/mod.rs:292`, `markdown/mod.rs:240`). This is what makes the split cheap.
- **The content IR already exists, privately, inside the PDF backend.** `pdf/ir.rs:106` `BlockNode` is a
  10-variant semantic document IR (`Heading{level}`, `Paragraph`, `Code`, `List`, `Table`, `Callout`,
  `Image`, `Svg`, `Diagram`, `Toc`), built by hand-walking blocks. The one backend that could not fake
  it built the thing — and it converged on **one variant per concept with no generic container**.

---

## 2.1 Split the fundamental layer in two

Blocks lower to a **semantic content IR** consumed by all four backends; a separate **HTML element
vocabulary** stays for templates and the HTML backend.

The two jobs one union does today pull in opposite directions — content wants to be target-neutral,
chrome is irreducibly HTML — and collapsing them is why a heading smuggles its level through a CSS
class. Made cheap by templates already being HTML-only.

**Where the current uses sit:** 57 of 63 `Element` uses and 38 of 45 `Raw` uses are in `templates.wcl`
(36/26), `website.wcl` (15/8) and `presentation.wcl` (6/4). Exactly **six** content blocks reach for
markup — `callout`, `chapter_header`, `code`, `footnotes`, `p`, `text` — and those are precisely the
six that [04](../issues/04-backend-survey.md) measured as degrading badly.

*Rejected:* one union with semantic variants added (leaves `Element`/`Raw` as permanent holes the
non-HTML backends keep guessing at); per-target lowers (a 3× authoring tax on 134 blocks to fix a
problem most don't have).

## 2.2 The content IR is closed — no `Html{}` escape

Consequence, accepted deliberately: users keep bespoke **drawings** (`extends SvgBlock` with a WCL
`lower` is untouched — the wireframe `wf_*` family and WAD's schema-derived shapes still plug in) and
lose bespoke **page markup**. Page content is semantic or it is not content.

*Rejected:* an `Html{…}` door detected at build. It would have kept the extension story and made the
consequence visible rather than silent; Wil took the stronger line — portability by construction, no
door to police.

## 2.3 One variant per document concept (~15–20), matched exhaustively

**The exhaustive match *is* the mechanism.** Today's divergence is possible only because every walker
ends in `_ => {}`. Make the match exhaustive and a missing case is a compile error — divergence becomes
unrepresentable rather than discouraged.

`pdf::ir::BlockNode`'s 10 variants are the starting point and the existence proof; the target is
~15–20, one per document concept.

**Cost, stated honestly:** a new page-content concept costs ~3 backend arms. That cost is **already paid
today** ([04](../issues/04-backend-survey.md): "adding a page block costs up to three plus the
fundamental-walker coverage"); the difference is that today you can skip them and nobody finds out.

*Rejected:* a small core plus a generic `Container { role: symbol, children }` — the role is a
stringly-typed key each backend interprets or ignores, i.e. this map's own failure mode one level down.
`BlockNode` converged on per-concept variants unaided.

**Corollary:** `lower_recurse`'s HTML-onlyness dissolves. All four backends consume one IR, so the
extension mechanism stops being HTML-only — [04](../issues/04-backend-survey.md)'s root-cause finding.

## 2.4 `@native` on the type; `lower` comes off the interface

A block declares **either** a `lower` **or** `@native`, and wdoc's build check enforces exactly one.
The interface stops demanding a fn nobody calls, and the editor's schema introspection gets something
true to read.

**The dividing line the ticket lacked:**

- **Payload-shaped natives** — Rust needs a small fixed payload (`code`, `math`, icons). These already
  compose native *leaf variants* honestly and are **left alone**.
- **Subtree-shaped natives** — Rust needs the block *and its children* (`timeline`'s events,
  `terminal`'s widget tree, `tilemap`'s grid, `wireframe`'s nesting). These cannot be expressed as a
  leaf, because an interface `@children` list never reaches the lowering record.

That distinction is exactly why 57 blocks are stubs and 2 are not.

**Which of the 34 page-content blocks become variants vs `@native` follows mechanically from this
test.** It is spec work for the implementing ticket, not a further decision.

## 2.5 One interface per output IR; placement leaves the interface

| interface | lowers to |
|---|---|
| `ContentBlock` | the content IR |
| `SvgBlock` | `SvgFundamental` |
| `TermPrimitive` | `TermFundamental` |

Placement (page-level vs diagram-child) is carried by the **slot's accepts-type**, which
[03](03-templates.md) already put there (`slot shapes: content<SvgBlock>`).

A page-level drawing gets a `Content::Drawing { shapes: list<SvgFundamental> }` bridge, so
`sequence_diagram`'s geometry becomes its **real** `lower` and **`lower_svg` dies**.

`TypeDecl::is_descendant_of` keeps the same shape the editor reads, so `/api/palette`'s `diagram_kinds`
and the wireframe `accepts_children` keep working.

## 2.6 `@native(html, markdown, …)` declares backend coverage

Cross-checked against the Rust dispatch registry, so a declared-but-unimplemented (or
implemented-but-undeclared) target is caught rather than becoming the next stub `lower`.

*Rejected:* an optional WCL `fallback` lower for uncovered targets. It would have collapsed `demo`'s
three drifted per-backend degradations into one authored place; Wil declined the extra machinery.

## 2.7 Using a block on a target it does not cover is a build error

Waived per-instance by the existing `@except(backends = [:pdf])`. No new mechanism: **capability says
*can't*, author intent says *don't want to***, and the build refuses to proceed until they agree.
[04](../issues/04-backend-survey.md) found the axis expresses instance-scoped intent while nothing
expressed kind-scoped capability; this supplies the counterpart.

Two consequences:

- The `backends` axis needs the **missing `:skill` symbol** (skill runs as `Backend::Markdown`).
- **The docs PDF build breaks** until `file` is implemented for PDF or explicitly waived. That is the
  mechanism working, and it resolves the map's incidental defect #4 ("is `file`-in-PDF deliberate?") by
  forcing someone to state the intent.

## 2.8 `slot` is the only splice concept; splicing becomes structural

The renderer fills slot markers as **tree nodes during recursion** — no U+FFF9 string surgery, no
per-backend reimplementation.

Deleted:

- `HtmlFundamental::Children { }` — **0 users repo-wide**
- both U+FFF9 sentinels — `WF_CHILDREN_SLOT` and `WF_CONTENT_SLOT` (`render/lower.rs:120,127`)
- the three hand-written component-content-slot implementations — `html.rs:820` (`String::replace` on
  rendered HTML), `pdf/collect.rs:165-217`, `markdown/emit.rs:224-262`

**Kept:** `TermFundamental::Children{row,col}` — not a splice marker at all but a *layout node*
carrying coordinates.

Structural filling is required regardless: string surgery on rendered HTML cannot serve PDF or
Markdown, which is exactly why they each grew a copy.

**A user's only route to a container block becomes `wdoc_component` + `slot`** — which is already how
all 64 real uses do it.

## 2.9 Generate the Rust enum from the WCL union

`wcl_wdoc/build.rs` already parses and embeds `lib/*.wcl`; this is an extra emit pass over a
~15-variant union of scalars and lists, producing the enum plus `TryFrom<Value>`. WCL stays the source
of truth, as every other schema here does.

*Rejected:* two hand-written declarations plus a CI drift gate — **the mechanism that has already failed
once in this repo** (map defect #2: three drifted WAD projection files, including a `routing = :straight`
CI-failure fix stranded in the live copy). A `Value → ContentNode` conversion seam that errors on
unknown variants is a workable fallback if the codegen proves worse than it looks, but it still leaves
two declarations to keep in step.

## 2.10 The constructor DSL

Source: ticket [15](../issues/15-constructor-dsl.md), prototyped executably at
[`proto-15-constructor-dsl/`](../proto-15-constructor-dsl/).

### 2.10.1 Two free fixes

1. **Delete the 204 redundant `: none` arguments.** Optional variant fields already default to `none`
   — verified by construction. `crates/wcl_wdoc/lib/tui.wcl:33` states the opposite (*"Every field must
   be supplied at construction… that's how WCL variant literals work"*) and is **wrong**; the comment
   has cost the stdlib 204 dead arguments. Fix the comment with the code.
2. **`class` becomes a none-dropping list** — `class: ["book-chapter", if e.current { "current" }]`,
   which kills `sel_if(true, ".book-chapter", e.current, ".current")` outright. Needs
   [01](01-language.md) §1.7 (else-less `if`); the dropping itself is consumer-side, and **an all-none
   list must emit no attribute**, not `class=""`.

Measured: the free fixes alone are **−11%** across the four prototype subjects. 02's headline "25 lines
→ 8" was inflated by about a quarter, not by most of it.

### 2.10.2 The DSL is the generic `el` family, over the HTML element vocabulary only

```wcl
el(tag, cls, kids)              // the common shape
ela(tag, cls, attrs, kids)      // with attributes
eli(tag, id, cls, kids)         // with an id
raw(html)  inl(text)  icon(name, cls)  para(cls, spans)
```

1088 characters of definition, four names, break-even at **~14 sites**, against **258 real
`HtmlFundamental::Element` sites repo-wide**. Measured −38% on subject code.

*Rejected:* per-tag constructors (`div` / `span` / `ul` / `li` / `p` / `header` / `a` + a 5-arg `elem`
escape). 3% smaller on subject code for **60% more definition** (1745 vs 1088) and a dozen names, with
a worse cliff — the moment a block needs an `id` you write `elem("div", c.id, cls, none, kids)`
positionally, which is exactly the ceremony the free fix just deleted.

### 2.10.3 `SvgFundamental` and the content IR keep the named-field literal

**This narrows 05 decision 7**, which had scoped one DSL across all three vocabularies. Two
measurements killed it:

- **A DSL over the content IR is a +37% regression** (`callout` as a `Content::Callout` variant is 392
  chars; with a constructor over it, 536). The IR is field-shaped by construction — the "constructor"
  is the variant with its field names deleted.
- **On SVG the payoff is 10× smaller and the risk inverts.** Marginal saving over the free union rename
  is ~1,542 chars across 71 sites, against HTML's ~16,500 across 258 — and **56 of those 71 sites** are
  `Label`/`Rect`/`Circle`/`Line`, whose supplied fields are 4–6 interchangeable `f64`s.

**And it could not have been one function anyway.** `el` returns `HtmlFundamental`; WCL has no generics
([01](01-language.md) §1.3 is syntax-only). "Generic across three vocabularies" can only mean the same
naming convention applied three times.

**The safety argument, demonstrated not asserted** (`proto-15-constructor-dsl/probe-silent.wcl`):

```
long form, misspelled field        CAUGHT   wcl::eval::variant_shape_mismatch
DSL, two f64 args transposed       SILENT   font_size: 32, fit_width: 10
DSL, one argument short            CAUGHT   'slabel' expects 7 argument(s), got 6
```

Transposing `font_size` and `fit_width` renders an axis label at triple size in a third of its box and
nothing objects — because **WCL does not check argument types at all** (`elem("div", none, none, [])`
evaluates clean against a param declared `identifier`). That is `_ => {}` and the stub `lower` one
level down, in the part of the spec whose subject is an honest type system.

### 2.10.4 Shorten the union names instead

§2.9 regenerates these unions from WCL, so their names are in play at zero cost.
`HtmlFundamental::` is 17 characters; `Html::` is 6. Measured: **28% of the DSL's entire win, with zero
safety loss** — ~6,500 characters repo-wide across 329 construction sites.

This is the whole answer for the field-shaped vocabularies. The verbosity there was never the field
names; it was the 17-character prefix in front of them.

**OPEN (naming, not design):** which short names. `Html` / `Svg` / `Content` are the obvious picks, but
the namespace question belongs with [01](01-language.md)'s extraction, not here.

### 2.10.5 `attrs` needs no design

Measured across every `.wcl` in the repo: **40** `attrs: [` sites, **1** containing any `if` (a
conditional *value*, `website.wcl:152`), **0** conditionally-*present* attributes.

02's claim that "roughly half of real markup noise is conditional class/attr composition" is half
right — the `class` half is 11 sites and §2.10.1 kills it; the `attr` half was hypothetical. Make
`attrs` none-dropping for symmetry **if it falls out of the same change for free**; do not design for
it, and do not carry it as an open question.

### 2.10.6 The long form stays legal

It is the **only** form for the two field-shaped vocabularies, so there is no "two ways to build the
same value" problem to police — the DSL is an HTML-only convenience layered over it, and its escape
hatch is just writing the record. No deprecation, no mechanical migration of the long form itself.

The DSL needs **no language change**: it is plain WCL `let` bindings in wdoc's stdlib. Which does mean
`el` becomes a name in wdoc's namespace that user templates import.

## 2.11 This part owns replacing the string-based introspection

The typed `FieldShape` API is specified in [01](01-language.md) §1.6, but **this part is the change
that triggers the breakage**: `WdocBlock` becomes `ContentBlock`, `lower` stops being universal,
`@native` appears, `wdoc_slot` becomes `slot`, `Region` and `wdoc_content` die. The introspection swap
must ship with these renames, not after them.

---

## What this kills

`lower_svg` (2 fns) · the 57 stub `lower`s · `heading_level()` parsing `"heading-2"` out of a CSS class
· Markdown's tag-string sniffing · every `_ => {}` in the fundamental walkers · both U+FFF9 sentinels ·
`HtmlFundamental::Children` · three hand-written component-content-slot implementations · most of
`kinds.rs` and its factually wrong header comment · `pdf::ir::BlockNode` as a *private* IR (it becomes
the shared one) · 204 dead `: none` args · `sel_if`.

## What it fixes that 04 measured

`chapter_header`'s kicker/reading_time/updated/version, `footnotes`' title and `<ol>` numbering,
`callout`'s icon, `code`'s filename header — all lost outside HTML today because they were `Raw`, all
recovered by becoming semantic variants whose markup lives backend-side. And the extension mechanism
stops being HTML-only.

## Checklist for this part

- [ ] Content IR union declared in WCL (~15–20 variants), Rust enum + `TryFrom<Value>` generated in `build.rs`
- [ ] HTML element vocabulary split out; `Children` deleted; both sentinels deleted
- [ ] All four backends match the content IR exhaustively — no `_ => {}` survives
- [ ] `ContentBlock` / `SvgBlock` / `TermPrimitive`; `Content::Drawing`; `lower_svg` deleted
- [ ] `@native` + `@native(targets…)`, exactly-one-of check, registry cross-check
- [ ] `:skill` added to the `backends` symbol set; uncovered-use build error; `file`-in-PDF resolved or waived
- [ ] Structural slot filling replaces the three hand-written implementations
- [ ] 204 `: none` deleted; `tui.wcl:33` comment fixed; `class` none-dropping (needs [01](01-language.md) §1.7)
- [ ] `el` / `ela` / `eli` / `raw` / `inl` / `icon` / `para` in the stdlib; 258 sites ported (part [07](07-migration.md))
- [ ] Union names shortened
- [ ] Editor introspection swapped to `FieldShape` **in this change**
- [ ] `CLAUDE.md`'s feature map corrected re `code` and `math`
