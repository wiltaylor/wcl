# An honest block type system — what replaces WdocBlock / SvgBlock and the 24 stub lowers?

Type: grilling
Status: resolved
Blocked by: 01, 04

## Question

The declared type system and the real one have diverged. Measured:

- **134** `@block` types. 43 `extends WdocBlock`, 30 `extends SvgBlock`.
- **24** declare a stub `lower` returning `[]` **while Rust intercepts them entirely** — terminal,
  card, node_table, tree, timeline, tilemap, dopesheet, map, wireframe widgets, icons, math, code,
  table, list. The declaration is a lie told to satisfy an interface.
- **5** `lower_svg` fns exist purely because a page-level block that draws SVG must still satisfy
  `WdocBlock` (whose `lower` returns HTML fundamentals), so its geometry has to hide in a second fn.

The mechanism itself is good and worth preserving: an unrecognised block kind dispatches to a WCL
`<kind>_lower` returning fundamentals, which the renderer recurses until only fundamentals remain.
That's what makes user-declared `@block(...) extends SvgBlock` shapes plug in — the wireframe `wf_*`
family, and the WAD Systems view's schema-derived editing, both depend on it.

Decide:

- **How does a Rust-implemented block declare itself truthfully?** A `@native` marker? A declared
  capability the renderer checks? Something that makes "Rust owns this one" a fact in the schema
  rather than a stub plus a match arm. The 24 aren't going away — calendar math, ANSI grids, LaTeX,
  syntax highlighting, measured widget layout and valid nested-list HTML genuinely aren't expressible
  in WCL — so the goal is honesty, not elimination.
- **How does a block declare which targets it can lower to?** One `lower` per target? A target-keyed
  set? Today it's one HTML-shaped `lower` plus ad-hoc second fns, which is why `lower_svg` exists.
- **Does the `WdocBlock` / `SvgBlock` split survive?** Its real content is "page-level vs
  diagram-child", which is about *placement*, not about what the block renders to. Those may be two
  different axes wrongly collapsed into one.
- **What happens to blocks a backend can't render?** Degradation is scattered per-block today
  (`demo`, `edit_object`, `markdown_source` each handle it differently). Does the type system carry
  it?

Constraints:

- **`TypeDecl::is_descendant_of` is already load-bearing.** `/api/palette` derives `diagram_kinds` from
  every `@block` descending from `wdoc.SvgBlock`; the WAD Systems view derives its whole model from
  schema introspection (`kind_links`), and the wireframe palette reads a schema-derived
  `accepts_children`. The editor's schema-driven UI is built on this hierarchy — changing it changes
  the editor.
- `04-backend-survey` establishes what each backend needs. Read it first.
- `01-content-seam` fixes what a template receives; the fundamentals are the shared currency.

## Inherited from tickets 01 + 04 (both resolved)

**From 04 — the root cause is narrower than this ticket assumed.** `lower_recurse` exists **only in
HTML** (`render/lower.rs:317`). That single asymmetry explains the per-backend divergence: `callout` has
a real WCL `lower`, so HTML follows it and needs no special case, while PDF and Markdown hand-reimplement
it in Rust. **Consequence: the WCL extension mechanism is HTML-only in practice** — a user block whose
`lower` returns another custom variant works in the book and silently renders nothing elsewhere. Fixing
recursive lowering across backends may resolve more of this ticket than redesigning the hierarchy does.

Corrected counts: **57** stub `lower`s (24 `HtmlFundamental` + 33 `SvgFundamental`), not 24; **2**
`lower_svg` fns, not 5.

**From 11 — the concrete breakage risk.** The editor's schema introspection (`kind_links`,
`blocks.rs:1526-1579`) tests types by **string equality on printed names**: `bare_type(f) == "identifier"`
/ `== "list<identifier>"` (`:1542,1558`), `to_string().starts_with("fn")`,
`full_name().starts_with("wdoc.")`. **Several fail silently** — reclassify a field and it just stops being
a parent link, no error. Any hierarchy change must account for this or the WAD Systems view degrades
quietly.

**From 01 — a template sees authored blocks, not lowered output.** So the block tree and the fundamentals
are now two distinct consumer-facing surfaces: templates walk the authored tree, backends consume
fundamentals. That may relieve pressure on the fundamentals to be expressive.

## Inherited from ticket 02 (resolved)

**You also own the fundamentals' constructor ergonomics — 02 handed them over.** 02 chose Model B, a
terse WCL element DSL for templates (`div(".ws-header", [ … ])` over nested `HtmlFundamental::Element`
records). But templates are not the only writer of fundamentals: **every block's `lower` writes them
too**, and there are far more `lower` fns than templates. The DSL is therefore a change to the
*fundamentals' constructor surface*, which is this ticket's currency — not a template-layer feature.

Design it once, here, for both consumers.

**Two things measured in 02's prototype that constrain it:**

- **Conditional attributes are the real cost, not element construction.** The prototype's DSL shortened
  the 25-line `<header>` to 8 lines, then produced
  `a(sel_if(true, ".book-chapter", e.current, ".current"), { href: e.href }, [txt(e.title)])` — worse
  than what it replaced. Roughly half of real markup noise is conditional class/attr composition and a
  constructor-only DSL does not touch it. **A DSL that only shortens construction buys much less than it
  appears to.**
- **`HtmlFundamental::Element` already carries slot machinery** — it is recursive (`children:
  list<HtmlFundamental>`) and `Children { }` is already a positional splice marker. 01 found the same
  pattern twice more as U+FFF9 sentinel strings (`WF_CHILDREN_SLOT` / `WF_CONTENT_SLOT`,
  `render/lower.rs:120,127`). Three instances of one idea, one of them typed. Whatever the fundamentals
  become should have exactly one.

**Not a constraint, but a reduced worry:** the `wdoc_part_*` family survives under Model B (composable
WCL fns are what B is), so the template layer imposes no migration on the fundamentals beyond the DSL
itself.

## Inherited from ticket 03 (resolved)

**`wdoc_content` is subsumed** by a unified typed `slot` concept shared by `template` and
`wdoc_component`. That removes one of the three instances of the splice-marker idea noted above; the
U+FFF9 sentinels remain yours.

**A slot's accepts-type must be expressible over whatever replaces `WdocBlock` / `SvgBlock`.**
`slot shapes: content<SvgBlock>` is now part of the contract — a slot may restrict what blocks it
accepts, and that restriction is checked at `wcl wdoc build`. Whatever the block hierarchy becomes has
to name a set of acceptable blocks in a *field type* position, not merely in an `extends` clause.

**Slot violations are checked at build, not `wcl check`**, because Wil ruled that wdoc concepts leave
`wcl_lang` entirely — see [ticket 14](14-wdoc-lang-extraction.md), which is blocked by this ticket
because its extraction seam must express whatever type system you land on.

## Answer

**One lowering pass, one semantic content IR, four backends that match it exhaustively.**

### Facts established while resolving (verified, not taken on report)

Several reframe the ticket's own premises:

- **`code` and `math` are not stubs.** They have real `lower`s emitting `HtmlFundamental::Highlighted`
  / `::Math` — leaf variants meaning "Rust computes this bit", composed into a WCL-built structure
  (`code`'s lower is genuinely `figure > pre > code > Highlighted(…)`). CLAUDE.md's feature map lists
  them among the Rust special-cased blocks; **that is wrong**. Two mechanisms for "Rust owns the hard
  part" already exist and one of them is honest.
- **Markdown reverse-engineers semantics out of markup.** `emit.rs:292` dispatches `element` by *tag
  string* (`"p"|"span"|"div"` → paragraph, `h1`–`h6` → heading, anything else → descend blindly), and
  a heading survives as a `Paragraph` whose **CSS class** carries the level — `heading_level()` parses
  `"heading-2"` (`render/accessors.rs:321`).
- **Templates are HTML-only.** PDF and Markdown never run one; they read `default_template` purely as
  a visibility flag (`pdf/mod.rs:292`, `markdown/mod.rs:240`). Neither the map nor this ticket said so,
  and it is what makes the split cheap.
- **The stub `lower`s are dead code.** `render_block` runs the Rust `match` **first**
  (`render/html.rs:532`), so a stub is never called. It exists only to satisfy the interface.
- **`Element`/`Raw` are template chrome, not content.** 57 of 63 `Element` uses and 38 of 45 `Raw`
  uses are in `templates.wcl` (36/26), `website.wcl` (15/8) and `presentation.wcl` (6/4). Exactly
  **six** content blocks reach for markup — `callout`, `chapter_header`, `code`, `footnotes`, `p`,
  `text` — and those are precisely the six 04 measured as degrading badly.
- **`tui.wcl:33` is wrong: optional variant fields already default to `none`.** Verified by
  construction (`wcl eval` on a variant literal omitting two optionals returns them as `none`). The
  stdlib carries **204 redundant `: none` arguments** written for nothing. This deflates ticket 02's
  headline measurement — its Model B win of "25 lines → 8" includes noise that was already free.
- **`HtmlFundamental::Children` has zero users repo-wide.** The component content slot (`wdoc_content`,
  64 authored uses) is `String::replace` on rendered HTML in the HTML path (`html.rs:820`) and is
  **hand-reimplemented** in PDF (`pdf/collect.rs:165-217`) and Markdown (`emit.rs:224-262`) — the same
  three-way divergence 04 found everywhere else.
- **The content IR already exists, privately, inside the PDF backend.** `pdf/ir.rs:106` `BlockNode` is
  a 10-variant semantic document IR — `Heading{level}`, `Paragraph`, `Code`, `List`, `Table`,
  `Callout`, `Image`, `Svg`, `Diagram`, `Toc`. `pdf/collect.rs` builds it by walking blocks and
  hand-reimplementing what HTML gets from lowering. The one backend that could not fake it built the
  thing. Note it converged on **one variant per document concept with no generic container** —
  `Callout` is its own variant carrying a resolved accent colour.
- **Exhaustive matching is impossible today.** `HtmlFundamental` exists only as a WCL union; Rust
  consumes it via `as_record_variant(value)` matching on *string* variant names. A string match has no
  exhaustiveness — which is why every walker ends in `_ => {}` (`emit.rs:332`).

### The decisions

**1 — Split the fundamental layer in two.** Blocks lower to a **semantic content IR** consumed by all
four backends; a separate **HTML element vocabulary** stays for templates and the HTML backend. The two
jobs one union does today pull in opposite directions — content wants to be target-neutral, chrome is
irreducibly HTML — and collapsing them is why a heading smuggles its level through a CSS class. Made
cheap by the fact that the chrome half already *is* HTML-only.

*Rejected:* one union with semantic variants added (leaves `Element`/`Raw` as permanent holes the
non-HTML backends keep guessing at); per-target lowers (a 3× authoring tax on 134 blocks to fix a
problem most don't have).

**2 — `@native` on the type; `lower` comes off the interface.** A block declares *either* a `lower`
*or* `@native`, and wdoc's build check enforces exactly one. The interface stops demanding a fn nobody
calls, and the editor's schema introspection gets something true to read.

The dividing line the ticket lacked: **payload-shaped** natives (Rust needs a small fixed payload —
`code`, `math`, icons) already compose native *leaf variants* honestly and are left alone;
**subtree-shaped** natives (Rust needs the block *and its children* — `timeline`'s events, `terminal`'s
widget tree, `tilemap`'s grid, `wireframe`'s nesting) cannot be expressed as a leaf, because an
interface `@children` list never reaches the lowering record. That is exactly why 57 blocks are stubs
and 2 are not.

**3 — The content IR is closed. No `Html{}` escape.** Consequence, accepted deliberately: users keep
bespoke *drawings* (`extends SvgBlock` with a WCL `lower` is untouched — the wireframe `wf_*` family
and WAD's schema-derived shapes still plug in) and lose bespoke *page markup*. Page content is semantic
or it is not content.

*Rejected:* an `Html{…}` door detected at build. It would have kept the extension story and made the
consequence visible rather than silent, but Wil took the stronger line — portability by construction,
no door to police.

**4 — One interface per output IR; placement leaves the interface.** `ContentBlock` → content IR,
`SvgBlock` → `SvgFundamental`, `TermPrimitive` → `TermFundamental`. Placement (page-level vs
diagram-child) is carried by the slot's accepts-type, which **ticket 03 already put there**
(`slot shapes: content<SvgBlock>`). A page-level drawing gets a `Content::Drawing { shapes:
list<SvgFundamental> }` bridge, so `sequence_diagram`'s geometry becomes its *real* `lower` and
**`lower_svg` dies**. `TypeDecl::is_descendant_of` keeps the same shape the editor reads, so
`diagram_kinds` and the wireframe `accepts_children` keep working.

**5 — `@native(html, markdown, …)` declares backend coverage**, cross-checked against the Rust dispatch
registry so a declared-but-unimplemented (or implemented-but-undeclared) target is caught rather than
becoming the next stub `lower`.

*Rejected:* an optional WCL `fallback` lower for uncovered targets. It would have collapsed `demo`'s
three drifted per-backend degradations into one authored place; Wil declined the extra machinery.

**6 — Using a block on a target it does not cover is a build error**, waived per-instance by the
existing `@except(backends = [:pdf])`. No new mechanism: capability says *can't*, author intent says
*don't want to*, and the build refuses to proceed until they agree. 04 found the axis expresses
instance-scoped intent while nothing expressed kind-scoped capability; this supplies the counterpart.

Two consequences: the `backends` axis needs the **missing `:skill` symbol** (skill runs as
`Backend::Markdown`), and **the docs PDF build breaks** until `file` is implemented for PDF or
explicitly waived — which is the mechanism working, and resolves the map's incidental defect #4
("whether `file`-in-PDF is deliberate is unresolved") by forcing someone to state the intent.

**7 — Two free fixes, plus one constructor DSL across all three vocabularies.** Delete the 204 dead
`: none`s; make `class` a **none-dropping list** (`class: ["book-chapter", if e.current { "current" }]`),
which kills 02's `sel_if(true, ".book-chapter", e.current, ".current")` outright. Then one generic DSL
over the HTML vocabulary, `SvgFundamental` and the content IR.

This is ticket 02's Model B **carried further than 02 scoped it**, against a deflated measurement: 02
handed the DSL over believing "there are far more `lower` fns than templates", which split A makes
false for the tree-shaped vocabulary. Its surface syntax is **not decided here** — see
[ticket 15](15-constructor-dsl.md).

**8 — `slot` is the only authored splice concept, and splicing becomes structural.** The renderer fills
slot markers as **tree nodes during recursion** — no U+FFF9 string surgery, no per-backend
reimplementation. `HtmlFundamental::Children` is deleted (0 users), both sentinels go, and
`TermFundamental::Children{row,col}` stays because it is not a splice marker at all but a *layout node*
carrying coordinates. Structural filling is required regardless: string surgery on rendered HTML cannot
serve PDF or Markdown, which is exactly why they each grew a copy.

A user's only route to a container block becomes `wdoc_component` + `slot` — which is already how all
64 real uses do it.

**9 — One variant per document concept (~15–20), matched exhaustively.** The exhaustive match *is* the
mechanism: today's divergence is possible only because every walker ends in `_ => {}`, so a variant
nobody handles is silence. Make the match exhaustive and a missing case is a compile error —
divergence becomes unrepresentable rather than discouraged.

*Rejected:* a small core plus a generic `Container { role: symbol, children }` — the role is a
stringly-typed key each backend interprets or ignores, i.e. this ticket's own failure mode one level
down. `BlockNode` converged on per-concept variants unaided.

Cost, stated honestly: a new page-content concept costs ~3 backend arms. That cost is **already paid
today** (04: "adding a page block costs up to three plus the fundamental-walker coverage"); the
difference is that today you can skip them and nobody finds out.

**10 — This ticket owns replacing the string-based schema introspection.** A typed
`TypeField::shape() -> FieldShape::{Scalar(prim), List(prim), Fn, Block, …}` replaces
`bare_type(f) == "identifier"` / `== "list<identifier>"` (`blocks.rs:1512,1542,1558`),
`to_string().starts_with("fn")` and `full_name().starts_with("wdoc.")`. This ticket **is** the change
that triggers the breakage 11 flagged — `WdocBlock` becomes `ContentBlock`, `lower` stops being
universal, `@native` appears — and ticket 14 reshuffles the very namespaces `full_name()` reads. The
failure mode is this ticket's theme in miniature: a reclassified field stops being a parent link with
no error, the same silence as `_ => {}` and as a stub `lower`.

**11 — Generate the Rust enum + `TryFrom<Value>` from the WCL union in `build.rs`.** WCL stays the
source of truth (as every other schema here does) and `wcl_wdoc/build.rs` already parses and embeds
`lib/*.wcl`, so this is an extra emit pass over a ~15-variant union of scalars and lists, not new
infrastructure.

*Rejected:* two hand-written declarations plus a CI drift gate — the mechanism that has **already
failed once in this repo** (map defect #2, three drifted WAD projection files). A `Value → ContentNode`
conversion seam that errors on unknown variants is a workable fallback if the codegen proves worse than
it looks, but it still leaves two declarations to keep in step.

### What this kills

`lower_svg` (2 fns) · the 57 stub `lower`s · `heading_level()` parsing `"heading-2"` out of a CSS class ·
Markdown's tag-string sniffing · every `_ => {}` in the fundamental walkers · both U+FFF9 sentinels ·
three hand-written component-content-slot implementations · most of `kinds.rs` (and its factually wrong
header comment, map defect #3) · `pdf::ir::BlockNode` as a *private* IR — it becomes the shared one ·
204 dead `: none` args · `sel_if`.

### What it fixes that 04 measured

`chapter_header`'s kicker/reading_time/updated/version, `footnotes`' title and `<ol>` numbering,
`callout`'s icon, `code`'s filename header — all lost outside HTML today because they were `Raw`, all
recovered by becoming semantic variants whose markup lives backend-side. And the extension mechanism
stops being HTML-only, which was 04's root-cause finding.

### Deliberately not decided here

- **The constructor DSL's surface syntax** — split out as [ticket 15](15-constructor-dsl.md). 02's
  measured weakness was conditional *attribute* composition; the none-dropping `class` list only
  half-solves it (`attrs` still needs an answer).
- **Which of the 34 page-content blocks become variants vs `@native`** — follows mechanically from the
  payload-shaped / subtree-shaped test in decision 2. Spec work, not a decision.

Status: resolved
