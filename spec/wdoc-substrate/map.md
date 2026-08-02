# Map: wdoc substrate refactor

Label: `wayfinder:map`

## Resume

```
/wayfinder spec/wdoc-substrate/map.md            # take the next frontier ticket
/wayfinder spec/wdoc-substrate/map.md ticket 02  # take a specific one
```

**Frontier (open, unblocked, unclaimed): none — every ticket is resolved.** Resolved: 01, 02, 03, 04,
05, 06, 07, 08, 09, 10, 11, 12, 13, 14, 15. Blocked: none.

**The route is walked, and the destination is delivered.** Nine files beside this one consolidate all
15 resolutions into a buildable spec — start at [`README.md`](README.md), which carries the dependency
order, the non-negotiables and how to cut tickets from it. The build tickets derived from it are
GitHub issues #34–#76.

What remains beyond the spec is the **Not yet specified** section — migration sequencing (eight
numbered sweeps, now written up as [`07-migration.md`](07-migration.md)), the blog gaps, and
the questions later tickets deliberately handed forward. All of that is carried into
[`08-open.md`](08-open.md), separated into *out of scope* / *deliberately not decided* /
*open questions*. None of it is a decision this map still owes.

Tickets live in `issues/NN-<slug>.md` — a `Status:` line (`open`/`claimed`/`resolved`) and a
`Blocked by:` line near the top; a ticket is unblocked when every file it lists is `resolved`.
**Claim a ticket by setting `Status: claimed` before doing any work.** Research findings are in
`research/`. One ticket per session (research excepted).

Read the **Measured facts** and **Decisions taken while charting** sections below before choosing —
they exist so a fresh session doesn't re-derive them, and several facts carry annotated corrections. The
downstream tickets 02, 03, 05, 07, 08, 14 and 15 each end with an **"Inherited from …"** section
recording what an upstream resolution already settled; don't re-litigate those.

## Destination

A **spec** for a refactored wdoc substrate — an honest block/type system, and a typed HTML
templating layer where you author real HTML templates and render wdoc content into declared
slots — **with the in-repo wskills and WAD migrated onto it as the proof it works**. Detailed
enough to break into tickets and hand to agents.

**Delivered: [`README.md`](README.md).**

## Notes

**Domain.** WCL / wdoc — a typed configuration language whose document model gathers structured
data, and a static-site generator that projects it to HTML, Markdown, a Claude skill folder, and
PDF. A *wskill* is a self-contained folder capturing one topic, projected into a book, an AI skill,
a training course and a presentation. A *WAD* is a typed architecture document rendered the same way.

**Skills every session should consult:** `/grilling` and `/domain-modeling` by default;
`/prototype` for the prototype tickets; `/research` for the research tickets. `/wcl`, `/wdoc` and
`/wskill` carry the reference for the three subject areas — prefer them over re-deriving behaviour
from source. `CLAUDE.md` carries the implementation contracts and the wdoc feature map.

**Standing preferences for this effort:**

- **Substrate-first.** wdoc's type system and templating layer are the destination. The in-repo
  consumers validate the design — they are not parallel subjects. A decision is only settled once it's
  clear they can be built on it.
- **CORRECTED by ticket 11 — the two halves have different proving grounds.** Charting assumed wskill
  and WAD both validated the template layer. They don't: **WAD does not touch the `TemplateCtx`/region
  seam at all** (one line, `default_template = :book`). WAD validates the layer *below* the template —
  repeaters, `project`, `wdoc_component`/`wdoc_slot`, string page identity, gathers, schema
  introspection. So:
  - **Block/type layer** (tickets 04, 05) — proved by WAD *and* the wskills. Well covered.
  - **Template/slot layer** (tickets 01, 02, 03) — proved by the **docs site** and the **wskill
    projection template sets**. WAD contributes nothing here. Do not let a template decision claim WAD
    as evidence.
- **Breaking refactor, no compat shims.** A shim would preserve the exact stringly-typed seam this
  effort exists to kill. Everything in-repo migrates.
- **Prose guidance is not a mechanism.** ~~The single most load-bearing finding below is that the
  wskill authoring rules are already written, already shipped to the agent, and already ignored.~~
  **NARROWED by ticket 06** — the rules are *not* ignored; measurement shows the `related` cap, the
  kind definitions and the anti-mirroring rule all being followed. The one rule that failed is the
  one with **nowhere in the data model to write it down** ("say why the reader would follow this
  link" — `related` is a bare id list). So the preference stands in this weaker, better-evidenced
  form: **guidance fails where the schema gives it no place to live; prefer giving the rule a field
  over giving it a paragraph.** Still do not resolve a ticket with "document it better" — but do not
  assume authors ignore documentation either, because here they didn't.
- **Plan, don't do.** Tickets resolve decisions. The destination is a spec someone else builds.

### Measured facts (established while charting — don't re-derive)

**The type hierarchy is lying.**
- **134** `@block` types in the wdoc stdlib: 43 `extends WdocBlock`, 30 `extends SvgBlock`.
- **57** declare a stub `lower` returning `[]` while Rust intercepts them — **24** returning
  `HtmlFundamental`, **33** returning `SvgFundamental`. The declared type system and the real one have
  diverged. *(Corrected from 24 by ticket 04 — the charting grep hardcoded `HtmlFundamental` and missed
  every SVG stub. Re-verified.)*
- **2** `lower_svg` fns — `sequence.wcl:279`, `statechart.wcl:404` — existing solely because the
  interface can't express "this block draws SVG at page level". *(Corrected from 5 by ticket 04 — the
  charting grep counted doc-comment mentions. Re-verified.)*

**The backends have diverged, and recursive lowering is why** (ticket 04).
- Each of the three code backends runs its **own** Rust `match` on `block.kind()`, and **the sets
  differ**. `kinds.rs:1-8` asserts they "special-case the same block vocabulary" — **factually wrong**.
- **`lower_recurse` exists only in HTML** (`render/lower.rs:317`). That single asymmetry is the root
  cause: `callout` has a real WCL `lower`, so HTML follows it and needs no special case, while PDF and
  Markdown hand-reimplement it in Rust (`pdf/collect.rs:137`, `markdown/emit.rs:200`). `file` is the
  mirror case — HTML + Markdown only, **absent from PDF entirely**.
- **Consequence: the extension mechanism is HTML-only in practice.** A user-declared block whose
  `lower` returns another custom variant works in the book and **silently renders nothing anywhere
  else** — the very mechanism CLAUDE.md presents as how user shapes plug in.
- Fundamental coverage of the 10 declared `HtmlFundamental` variants: HTML 10/10, Markdown 6/10, PDF
  5/10. `Table`, `Head`, `Children`, `Icon` are HTML-only.
- Of 34 page-content blocks, 29 reach all three backends — but *reaches ≠ renders equivalently*. Only
  `p`, `text`, `h1`–`h6`, `math` survive intact everywhere through the fundamental layer alone.
- `@only`/`@except` has a `backends` axis (`:html`/`:pdf`/`:markdown`) but **no `:skill` symbol**, and
  expresses instance-scoped *author intent*, not kind-scoped *capability*. A vanished block warns nobody.

**The book template is 36% of a build** (ticket 11). WAD: 32 authored `page` blocks → **161 rendered
pages**; full HTML build 55.4s debug, of which **20.22s is inside `wdoc_book_layout`** — called once per
page and recursively re-walking the whole toc each time, `book_pageflow` **26082 calls** for 161 pages.
Parse+validate is only 4%. *The template layer needs to change on performance grounds alone, independent
of ergonomics.*

**Schema introspection rests on printed-string comparisons** (ticket 11). The editor's WAD Systems view
derives its whole model from `kind_links` (`blocks.rs:1526-1579`); its load-bearing tests are string
equality on rendered type names — `bare_type(f) == "identifier"` / `== "list<identifier>"`
(`blocks.rs:1542,1558`), `to_string().starts_with("fn")`, `full_name().starts_with("wdoc.")`. **Several
fail silently**: a reclassified field just stops being a parent link, with no error. This is the concrete
breakage risk the type-system refactor carries.

**Four more corrections, from ticket 05** (each verified by construction, not by grep):
- **`code` and `math` are NOT stub-lowered.** They have *real* `lower`s emitting `HtmlFundamental::
  Highlighted` / `::Math` — leaf variants meaning "Rust computes this bit" composed into WCL-built
  structure. **CLAUDE.md's feature map lists them among the Rust special-cased blocks and is wrong.**
  Two mechanisms for "Rust owns the hard part" already exist; one of them is honest.
- **Templates are HTML-only.** PDF and Markdown never run one — they read `default_template` purely as
  a visibility flag (`pdf/mod.rs:292`, `markdown/mod.rs:240`). This is what makes splitting the
  fundamentals cheap, and it was nowhere on this map.
- **`tui.wcl:33` is wrong: optional variant fields already default to `none`.** The stdlib carries
  **204 redundant `: none` arguments** written for nothing. This *deflates ticket 02's headline
  measurement* — part of its "25 lines → 8" DSL win was already free. ~~See ticket 15.~~ **QUANTIFIED
  by ticket 15: the free fixes are −11%, so 02 was inflated by about a quarter, not by most of it.**
- **The content IR already exists, privately, inside the PDF backend.** `pdf/ir.rs:106` `BlockNode` is
  a 10-variant semantic document IR (`Heading{level}`, `Paragraph`, `Code`, `List`, `Table`, `Callout`,
  …), built by hand-walking blocks. The one backend that couldn't fake it built the thing — and it
  converged on one variant per concept with **no generic container**.

**What the language can and cannot express in a constructor** (ticket 15, all verified against
`target/debug/wcl` — these constrain any future DSL question, not just this one).
- Fn calls are **positional only**: no named arguments, no defaults, no variadics
  (`Call { args: Vec<Expr> }`; `FnParam` is `{name, ty}`).
- **`?` in a parameter list is a PARSE ERROR**, and there are **zero** optional fn params in the whole
  stdlib. So a constructor's arity is fixed at declaration and every call must fill it.
- **Argument types are not checked at all** — `elem("div", none, none, [])` evaluates clean against a
  param declared `identifier`. Arity is the only positional mistake the language catches.
- **A bare record is not an options bag**: a missing field is an *unresolved reference*, not `none`.
- **An else-less `if` does not parse**, which is why 05's headline `class` example is aspirational.
  Ticket 15 asks for this as the one `wcl_lang` change on the map.
- **`["a", none]` type-checks today** in a `list<utf8>` and stays `["a", none]`, so none-dropping is a
  **consumer-side Rust change** — and an all-none list must emit *no attribute*, not `class=""`.

**The construction corpus is 329 sites, and mostly not the stdlib** (ticket 15). **258**
`HtmlFundamental::Element` + **71** `SvgFundamental::*`. 05's "57 of 63" counted `crates/wcl_wdoc/lib/`
only; the largest single file is `docs/pages/wcl/landing-parts.wcl` (**51**), then the scaffold
templates (**66**) and the wskill projections (**~56**). **Only 57 of 258 are stdlib** — so the
constructor surface is user-facing, and porting it contends for the same files as the CSS sweep and the
schema/template de-duplication.

**Conditional attributes are a phantom** (ticket 15). Across every `.wcl` in the repo: **40** `attrs: [`
sites, **1** containing any `if` (a conditional *value*), **0** conditionally-*present*. 02's "roughly
half of real markup noise is conditional class/attr composition" is half right — the `class` half is
**11** sites and 05's none-dropping list kills it; the `attr` half was hypothetical.

**Exhaustive matching is impossible today** (ticket 05). `HtmlFundamental` exists only as a WCL union;
Rust consumes it via `as_record_variant(value)` matching on **string** variant names. A string match has
no exhaustiveness — which is precisely why every backend walker ends in `_ => {}` (`emit.rs:332`), and
why a variant nobody handles is silence rather than a compile error.

**The template seam is stringly-typed.**
- `TemplateCtx.content: utf8` — the page body reaches a template as **opaque pre-rendered HTML**.
- `Region { name: utf8, content: utf8 }` — named regions likewise, keyed by **unchecked string
  name**. `wdoc_region(c, "heor")` silently returns `""`.
- A layout cannot declare the slots it needs; a page cannot be validated against them.
- **4** templates total (`webpage`, `book`, `website`, `presentation`).
- No middle gear for authoring: either construct typed `HtmlFundamental::Element` records
  (`website_header` = 25 lines of WCL for one `<header>`) or drop to
  `HtmlFundamental::Raw { html: <<HEREDOC }` and lose every guarantee.

**Ergonomics can also *lose* checking you already had** (ticket 15) — the inverse of the pattern below,
and worth the same suspicion. A positional constructor over a field-shaped variant silently drops the
`variant_shape_mismatch` error the named literal gave you, because argument types are unchecked: two
transposed `f64`s render a triple-size axis label and nothing objects. Demonstrated, not argued
(`proto-15-constructor-dsl/probe-silent.wcl`). **Ask what a shorthand costs, not only what it saves.**

**"Get the checking free" — failed twice, held once.** *(14 is the counterexample: its accepts-check
for `content<SvgBlock>` genuinely came free, because `@children(X)` + `is_descendant_of` already does
that work and the derivation just emits the decorator. The pattern isn't dead — it's a claim to
verify. Both failures below were verifications that came back negative.)*

**"Make it a symbol" has now failed twice** (tickets 03 and 13). Ticket 02 won partly on *a slot is a
symbol, so the typo is already an error*. Ticket 03 killed that for slots by scoping them to their
declaring block. Ticket 13 killed it for CSS classes on lexing grounds: **`is_ident_cont` is
alphanumeric-or-underscore** (`lexer.rs:592`) and **all 237 class names are hyphenated** — `:book-sidebar`
does not lex. Treat "we'll make it a symbol set and get checking free" as a claim to verify, not a
premise.

**CSS and class names, measured** (ticket 13). CSS is **27 heredocs across 27 stdlib files** (~349 rules
counting `theme.rs` + `code-theme.css`), only **4** of them template-level; **0 use interpolation**.
**94 distinct properties, 74 outside the `Class` allowlist**, and **WCL has no map type**
(`TypeRef` = Builtin/Named/Reference/List/Tensor/Function, `value.rs:534`). Class names reach markup
through **three** channels — WCL `class:` field **76** distinct names, **Rust-generated markup 61**,
raw-HTML strings inside WCL **39** — and 05 keeps `Element`/`Raw` as template chrome while `@native`
keeps Rust markup, so **neither blind channel goes away**: any source-side check sees 43%, permanently.

**Regions have almost no users, and a checked slot contract already exists next door** (ticket 03).
- **7** `region "…"` blocks in the entire repo — 4 names, across 3 files. Only the `website` template
  consumes them; book, presentation, webpage, WAD and every wskill projection use **zero**.
- **`wdoc_component` / `wdoc_slot` is already a working bidirectional contract**, verified by running
  `wcl check`: unknown field *and* missing-required-slot are both errors
  (`crates/wcl_lang/src/doc/schema_check.rs:353-412`). The contract the map set out to invent for
  layouts was already shipping one abstraction over.
- **A `template` is already a block** (`templates.wcl:327`), so slot declarations had an obvious home.

**wdoc's entanglement in `wcl_lang` is 12 sites, not 109** (ticket 03). 109 raw `wdoc` greps in
`crates/wcl_lang/src/`, but only **12 are live code outside tests** — components/slots (`doc.rs:2043,2157`,
`views.rs:2884,2895`, `schema_check.rs:374`) and repeaters/instances (`views.rs:2813,2834,2854`,
`schema_check.rs:787`). Everything else is doc comments, a lexer test string and a docstring example.
Wil ruled on ticket 03 that wdoc comes out of the language crate; **ticket 14** owns it.
*Ticket 14 adds **8 more in `crates/wcl`** (`editor/blocks.rs:1686,1699`, `editor/nav.rs` ×3,
`editor/mod.rs`) — 03 surveyed only the language crate. Those are **legitimate** (the editor is a wdoc
consumer) but they break on 03/05's renames, so they are migration surface.*

**Four more measurements, from ticket 14** (each verified before deciding):
- **`component_def` is public and used 9× in `wcl_wdoc`**, not an internal detail — `render/expand.rs`
  ×3, `build.rs`, `tree.rs`, `node_table.rs`, `html.rs`, `pdf/collect.rs`, `markdown/emit.rs`.
- **The language's expander is a deliberate simplified mirror of wdoc's** — `views.rs:2824-2902`
  (87 lines) against `render/expand.rs` (431). Its own doc comment says so: *"kept minimal for the
  projection path: an erroring `each` or slot value contributes nothing here."* Two implementations of
  one semantics, drifting **by design**. `partial` / `collect` are the control case — wdoc generators
  declared in the same file, appearing in `wcl_lang` **zero** times.
- **Child-kind checking reads the `@children(X)` decorator argument, not the field type**
  (`schema_check.rs:742-770`). The field type is decorative there. This is what let 14 buy 03's
  `content<SvgBlock>` accepts-check with **zero** type-system semantics.
- **`TypeRef` has no generics** (`value.rs:534`), so 03's `slot shapes: content<SvgBlock>` was
  literally not expressible. **36** `TypeRef::Named` sites, **all inside `wcl_lang`**.

**~~The wskill graph has collapsed.~~ CORRECTED BY TICKET 06 — it has not. Two of these three
claims were measurement errors; the guidance is being followed.**
- ~~The `wcl` wskill: **45 concepts, 2 entities, 18 facts**. The kind vocabulary is not discriminable
  in practice — almost everything becomes a `concept`.~~ *`wcl` is one topic shape, not the norm.
  wdoc is **fact**-dominant (34/13), wad is procedure-heavy (22), wskill is balanced — and wcl and
  wdoc were created the **same day**. Reading the ids, all four apply the shipped guide correctly:
  WCL genuinely is 45 ideas. The distributions differ because the topics do.*
- ~~One unit carries **19** `related` ids, another 12; `linking_discipline` caps them at 3–5.~~ *Both
  are **indexes**, which the cap explicitly exempts ("an `index` may pin as many units as its area
  needs — the cap is for content units"). Content units over cap: **wcl 0%, wskill 0%, wdoc 2%,
  wad 4%**. Index-mirroring into `related` is likewise not happening — 11–23% link density among
  co-pinned pairs vs a 1–2.5% baseline; wholesale mirroring would read 80–100%.*
- `docs/wskills/wskill/data/fact/linking_discipline.wcl` already states the cap, the "a link costs
  two pages" rule, the hub-note anti-pattern ("a menu wearing a page's clothes") and a symptom
  table. `unit_decision_guide` and `writing_style` sit beside it. All are `audience = :both`, so they
  ship to the agent. ~~**They are ignored.**~~ *They are followed — measurably. **This is the single
  most important correction on the map**: the standing preference "prose guidance is not a mechanism"
  was inferred from evidence that does not hold. Guidance did bind here. What did **not** bind is the
  one rule with nowhere to write it down — "say why the reader would follow this link" — because
  `related` is a bare id list. **87–92% of edges carry no reason anywhere** (WAD: 0 of 175). The
  lesson is narrower than the original: guidance fails when the data model gives it no place to live.*

**The real defect is unannotated edges, not a collapsed vocabulary** (ticket 06).
- **87–92%** of `related` ids appear nowhere in the source unit's prose. The graph has nodes and
  edges; the edges carry no meaning. Fixed by making a per-edge `why` mandatory — **736 edges** to
  migrate.
- **`concept` and `fact` are the same type.** Identical fields; `Concept` adds `summary`, says `name`
  for `title`. Their two page components diff to **one rendered italic line**. Kept anyway (Wil).
- **32–48% of edges are reciprocal**, and "Related" / "Referenced by" do not dedup — so a third to a
  half of edges render the same link twice on one page.
- **53 indexes across the four wskills, only 6 carry a `body`** — and a body is what earns an index a
  page (`book/main.wcl:113–130`). Indexes are absent from `all_units`, so a `related` id naming one
  renders nothing, silently.

**The wskill toolchain is scattered and incomplete.**
- **There is no search anywhere in Design mode** (ticket 10). Grepping the whole
  `editor-ui/src/components/design/` tree for search UI returns **one** hit, and it is a colour
  constant. Not in the graph view, not in the index panel, not in the content modal — at 65 units.
- **A graph diff over an authoring commit finds what a file diff cannot** (ticket 10). On the real
  commit `99518181` (+30 −5 units, +160 −19 edges): **all 30 new units landed unpinned**, because the
  index housing them wasn't authored until three commits later. Also, four of ten graph-health metrics
  moved worse in that one commit — edges/unit 2.11 → 3.39, hub-shaped units 3 → 11, units over the
  5-link cap 0 → 10, reasonless edges 92 → 208.
- `crates/wcl/src/editor/graph.rs` (639 lines) **already computes** the structural model a curator
  needs — units, nested index trees, `related` edges, pin edges, per-unit block lists,
  `related_editable` flags, unindexed detection. It is locked behind an editor HTTP endpoint.
- There is **no `wcl wskill` subcommand at all**.
- ~~The seven gates~~ **CORRECTED by ticket 08 — there are eight, and `wskill-check` does not exist.**
  The real set: `wskill-schema-sync`, `wskill-schema-check`, `wskill-template-check`,
  `wskill-crosstopic-check`, `wskill-artifact-check`, `wskill-coverage`, **`skills-check`** and
  **`wcl-refcheck`** (that last one is `wcl`-wskill-specific — it diffs the documented builtin and
  subcommand lists against the binary, so it is not part of the consolidation). Being repo-local, they
  contradict the format's own `selfcontained` philosophy unit. **None checks graph shape.**
- **Four of the eight police duplicated files, not the format** (ticket 08). `schema/base.wcl` plus
  **14 topic-agnostic wdoc templates** are copied verbatim into every wskill; two CI gates diff the
  copies (one against the scaffold heredoc, one against the reference implementation). That is ~56
  copies maintained to permit **five** documented divergences, three of which are `wcl` and `wdoc`
  being reference-heavy. Ticket 08 deletes the duplication rather than migrating the gates.
- `docs/wskills/wskill/skill/` does not exist — the shipped AI skill bundles **zero** executables.
  *(Ticket 08 keeps it that way — the skill documents the CLI — but the `script` block survives:
  other wskills under way generate AI skills and will use it.)*
- **Unit ids are assumed unique across kinds and nothing enforces it** (ticket 08). `related:
  list<identifier>` resolves against a flat `all_units` built per projection.
- **`wcl_wdoc` already knows the wskill format** (ticket 08) — `sidecar.rs:38,60` walks up looking for
  `wskill.wcl` to place `comments.wcl`. Small, but the format-agnostic line is already crossed.

**`researching_a_topic` has never been run** (ticket 09). All four in-repo wskills carry **zero**
`research` units and no `data/questions.wcl`; so do the installed skills under `~/.claude/skills`. The
only `research` block in the repo is the fill-out template. So ticket 06's "the graph is healthy"
measures wskills authored by hand through `adding_content` and says **nothing** about the parallel
fan-out — and the sprawl the research path is accused of has never been observed either. Ticket 09's
decisions are reasoned, not measured; don't cite them as evidence.

**Authoring processes derive structure from content.** *(Both reordered by ticket 09 — see Decisions.)*
- `adding_content`: decompose → classify → write → **link** → **pin**. The unit exists before anyone
  asked where it belongs, so `related` becomes the agent's way of justifying it after the fact.
- `researching_a_topic`: scope → decompose → **dispatch researchers in parallel** → gate → distill
  into units → **build the index**. Every researcher writes blind to every other; the index is
  retrofitted onto nodes that already exist.

### Decisions taken while charting

These were settled in the charting grill and are premises, not open questions:

- Leverage is **how the agent authors**, not better prose and not lint alone.
- Structure is owned by a **dedicated curator pass**: authors write freely and messily; the curator
  re-shapes the graph.
- The curator **edits directly**, gated on render + validate. An advisory curator would just be
  `linking_discipline` again.
- Its eyes are a **new `wcl wskill` CLI surface** — `graph.rs`'s model lifted into the library.
- That surface **consolidates** the seven justfile recipes, so the gates travel with the format.
- **One map, shared spine**: the agent-authoring half and the editor-review half together.
- The human loop is the **browser editor** — Design mode, reviewing AI output, navigating a big
  wskill. Not hand-editing `.wcl`.

## Decisions so far

<!-- one line per closed ticket -->

- [What does each of the four backends actually need from a block?](issues/04-backend-survey.md) — the
  Rust special-casing is a **different set per backend**, and the root cause is that `lower_recurse`
  exists only in HTML, so the WCL extension mechanism silently renders nothing outside the book. Also
  corrected two charting facts: **57** stub `lower`s (not 24) and **2** `lower_svg` fns (not 5).
- [How does WAD use the current template and block seam?](issues/11-wad-seam-survey.md) — **WAD does not
  touch the template seam at all** (one line: `default_template = :book`), so it proves the block/data
  layer, not the slot contract. Separately: `wdoc_book_layout` is **36% of a build**, and the editor's
  schema introspection rests on silently-failing printed-string comparisons.
- [The content seam — what does a template actually receive?](issues/01-content-seam.md) — the
  **authored block tree** for its page plus prepared site context. Placement is by **typed block
  handles resolved after template eval** — no phase inversion, and the pattern already exists twice as
  U+FFF9 sentinel strings. Query scope: page-local free, cross-page **memoised**. No template today
  inspects content at all (3 uses of `c.content`, all pure paste) — the opacity hurts the *renderer*,
  which string-searches its own output to recover an h1 it just rendered.
- [How is an HTML template authored?](issues/02-template-authoring.md) — **Model B, a terse WCL element
  DSL**; templates stay in WCL. Model C collapses into A (a heredoc can't loop, so every loop chops the
  markup into fragments that aren't well-formed HTML — exactly the property C existed to buy), and A was
  ruled out once Wil killed the paste-a-design story (*"exact html is not a problem, can get AI to
  migrate"*), leaving syntax preference against hand-writing a second language. ~~**Checking comes free:
  a slot is a symbol, so `:heor` is already a symbol-set violation.**~~ *(CORRECTED by ticket 03 — that
  holds only if slot names share one global symbol set, which 03 killed by scoping slots to their
  declaring block. References resolve at render instead.)* Note Hugo's template layer *is*
  Model A — parity is with its capabilities, not its surface. The one thing B loses is that Model A's
  inability to compute forced the book's recursive helpers out of the template; recovered deliberately
  as a **memoised, metadata-only page-metadata builtin** that never forces a page body. *(Its CSS call
  was retracted in the same session — see ticket 13.)*
- [The slot contract — how does a layout declare what it needs?](issues/03-slot-contract.md) — **one
  typed `slot` concept** shared by `template` and `wdoc_component` (`slot hero: content`,
  `content<SvgBlock>`, `?` optional, `= …` defaulted); `region` / `wdoc_region` / `Region` die and
  `wdoc_content` is subsumed. Fills are **bare names, layout-agnostic** — non-negotiable, or wskill's
  four-way projection breaks. `content` is a reserved slot the layout must *declare* but pages fill
  implicitly, so a blog list layout can declare none. **Checked at `wcl wdoc build`, not `wcl check`** —
  Wil ruled wdoc out of `wcl_lang` (**new ticket 14**). Strict errors, with `?` at the *fill* site as
  the author's opt-in to "fill only if this layout has it" (dropped otherwise): page owns
  conditionality, layout owns the fallback. Provenance becomes one `display:contents` wrapper **per
  slot, including unfilled ones** — which is also the editor's editing surface, and fixes a live bug
  where region content has no page provenance at all.
- [An honest block type system](issues/05-block-type-system.md) — **the fundamentals split in two**: a
  closed semantic **content IR** (~15–20 variants, one per document concept) every backend matches
  **exhaustively**, and an **HTML element vocabulary** for templates only. `@native` replaces the 57
  stub `lower`s and declares its backend coverage, cross-checked against the Rust dispatch registry;
  use on an uncovered target is a build error waived by `@except(backends = …)`. One interface per
  output IR, so placement leaves the hierarchy and **`lower_svg` dies**. Splicing becomes structural;
  the Rust enum is **generated from the WCL union**. Also: 02's DSL is scoped wider but its measurement
  deflated (**ticket 15**), and this ticket owns replacing the editor's string-based introspection.
- [Does the unit-kind vocabulary survive, and what makes it discriminable?](issues/06-unit-kinds.md) —
  **the vocabulary stands unchanged; the ticket's own premise was wrong.** `concept`/`fact` bind fine
  as prose definitions (`fact` = indisputable/verifiable, `concept` = a describable model) and
  `entity` at 3% is **correct rarity** — it holds external proper nouns, and self-documenting topics
  have few. `related` stays **untyped**, but **every edge now carries a mandatory `why`** — the one
  real defect being that 87–92% of edges are bare, which is what makes over-linking free. Indexes
  become linkable nodes **iff they carry a `body`** (Wil's ask; a page for a body-less index would be
  a machine-generated hub note). Hubs are a **screen, not a test**, so the curator **reads bodies but
  writes only structure** — settling `07`'s sharpest question. Also corrected two map facts: the
  vocabulary hasn't collapsed, and the linking guidance **is** being followed.
- [The curator contract — what may it change, and what gates it?](issues/07-curator-contract.md) —
  **one agent in two phases** (the CLI's mechanical screens emit a *candidate list*; the agent reads
  only the flagged units), so **scale stops mattering**. Its write set is **forward-only on prose**: it
  authors a `why` for an edge it *creates* but **never backfills** one onto an existing bare edge —
  Wil's line, on the grounds that writing the reason for an edge you're making is stating it, while
  writing one for someone else's is fabricating it. Findings it may not fix are filed into the
  existing **`comments.wcl`** sidecar as `author = "curator"`. Same op vocabulary as the editor,
  transport left to 08, **no running editor required**. Runs as a terminal phase of every authoring
  session at 1-hop scope; whole-graph only on demand. Gate is two-tier — per-op schema rollback, then
  *four projections build* **and** *lint must not get worse than the pre-run baseline*. Backstop is
  **git alone** (clean tree, one commit, `--dry-run`), with **no human gate on the pass**. Forces the
  736-edge migration to a shape: **delete every bare edge except where the target is already named in
  the source prose** (~60–95 survive; the flip is a filter, not a wipe).
- [The `wcl wskill` CLI surface](issues/08-wskill-cli.md) — **five subcommands** (`graph`, `lint`,
  `check`, `op`, `install`) over a new **`crates/wcl_wskill`**, with the editor demoted to a thin
  adapter; nomination is a `candidate` **severity** inside `lint`, not a sixth command. The ticket's own
  premise was wrong — there are **eight** recipes, and **four of them police duplicated files**, so
  Wil killed the duplication instead of migrating the gates: the schema and the 14 topic-agnostic
  templates become `import <wskill.wcl>` from an embedded registry, and **overrides are import
  granularity** (opt out of a part, declare your own — no shadowing mechanism, which dissolved a
  queued ticket). One **id-addressed** op vocabulary defined once, with the editor's span path
  resolving in front. `why` is **schema-required, not a lint rule**; the gate baseline is
  **in-memory per-run**, no file. `Comment` gains `object_kind`/`object_id` so a page-less finding has
  somewhere to land. The 736-edge flip is **`lint --fix`**, which forces a four-step migration order.

- [How do research findings become units without manufacturing sprawl?](issues/09-research-flow.md) —
  **both authoring processes become shape-first**, and the fan-out survives untouched: the sprawl
  source was never the parallelism but step 5's *per-finding* distillation loop, so distillation
  becomes **one pass over all findings** filling a **provisional index tree** authored at the scoping
  interview (each node carrying a `body` that does triple duty — reader's area page per 06,
  researcher's brief, distiller's fill contract; *a node nobody can scope shouldn't exist*). The
  "Build the index" step **dissolves**. Wil kept the gate coverage-only, so the guard moved inside
  distillation: **pass A revises the tree (required, not permitted), pass B fills it** — no unit
  written against an unrevised node. The terminal curator pass is **whole-graph** on this process
  alone, and `adding_content` reorders to **place-before-write**. Also measured: this process has
  never actually been run.

- [What does the editor need so you can audit AI output and navigate a big wskill?](issues/10-editor-review.md)
  — **one audit view** (changelog + union graph, health metrics collapsed into its header) behind a
  **sixth** subcommand `wcl wskill audit [<rev>..<rev>]`, breaking 08's five-command precedent because
  it is the only one taking *two* graphs. Its graph is **before ∪ after with removals ghosted** — the
  one thing the editor's live graph structurally cannot do, since it draws only what exists. Lint
  findings ride the changed rows as tags, scoped to the diff. Plus: **plain search everywhere** (none
  exists in Design mode today), and the "curate this index" button is taken, opening the audit view on
  the pass's commit. Measured on a real commit: **all 30 units of `99518181` landed unpinned** — their
  index wasn't authored until three commits later.

- [Template selection — does a page *type* axis exist, or is it per-page?](issues/12-template-selection.md)
  — **no type axis**; the type is the **site** or the **repeater**, both of which already share a template
  by construction (measured: exactly **one** per-page `template =` override in the whole repo). Blog
  collections are **data**, not pages. But the ticket's second half inverted: wdoc grows **general
  collection templates** (Wil overruled the recommendation against), declared by **slot arity** —
  `slot content: content*` — which **deletes `build.rs:1483`'s `default_template == "presentation"`
  string comparison** and makes user-declared collection layouts work. Members arrive as **typed page
  handles**, so forcing is demand-driven; `*` slots are filled per member, non-`*` ones by the **`site`
  block**, which now carries content. Also: **`Page.sites` stops defaulting to every site** in
  multi-site documents (the site now picks the layout, so implicit fan-out silently reassigns layouts),
  and slot checking on repeater-generated pages is **possibly-fills**.

- [How is template CSS authored — and does the `class` DSL grow to cover it?](issues/13-css-authoring.md)
  — **position 3, typed selectors + raw declarations**, reached from position 2's direction: structure is
  WCL (`class` with an optional `tag` and raw-fragment `nest`, plus `base` / `font_face` / `media` /
  `keyframes` for the ~10% residue), declaration bodies stay CSS text, and **every heredoc dies** along
  with the `@block("stylesheet")` type. Modelling declarations was killed by the census — **94 distinct
  properties, 74 outside the allowlist**, and **WCL has no map type**, leaving only ~282 field
  declarations or `list<list<utf8>>`. `Class` keeps the **SVG paint set only** (71% of field use) plus
  `accent`; **11 never-used properties** are deleted. Checking is an **output-scan lint at build**, not a
  type: a source-side check can only ever see **43%** of class uses, and **symbols are impossible because
  hyphens don't lex**. All authored CSS leaves Rust (`APPLY` ~84 rules, `FONT_DEFAULTS`, `code-theme.css`);
  the palette generator stays. Migration is a **throwaway tinycss2 script** — **no CSS parser ships**, since
  wdoc generates the selectors it lints against. Also: the ticket's scope was ~7× its framing (**27
  heredocs stdlib-wide, 23 of them block-level**, not "template CSS"), and the `//` footgun dies
  structurally. **Prototyped** ([`proto-13-css-authoring/`](proto-13-css-authoring/)): the vocabulary
  round-trips the real corpus **losslessly — 477 rules in, 477 out** — but only after two fixes argument
  missed (`nest` needs SCSS's **`&`** to tell descendant from compound, else 31 rules reconstruct wrongly
  and *silently*; the **`tag =` qualifier doesn't work** and those ~9 roots want `base`). It also
  **falsified the lint as specified**: 178 findings and **0 true positives** on a real 466-page build,
  84% of them syntect's open-ended `tok-*` vocabulary, the rest cross-site false positives — so the lint
  needs a structural generator exemption and **all-sites** scope before its waiver question is reachable.

- [Break wdoc out of `wcl_lang` — what replaces the hardcoded block names?](issues/14-wdoc-lang-extraction.md)
  — the 12 sites are **three clusters with three different answers**, and two of them are **deleted
  rather than moved**. A **language-owned decorator vocabulary, consumer-applied**
  (`@declares_kind(name = 0, params = "slots", body = "body")`, `@contextual`) — new ground, since
  wdoc's existing decorators are inert metadata and these change the language's *own* behaviour.
  `@declares_kind` makes `block_schema()` fall back to a **lazily derived schema**, so component
  instances become ordinary registered kinds and `validate_component_instance` **dissolves into
  `UnknownField`/`MissingRequired`** — which is what keeps `wcl check` catching the slot error the
  ticket named as its regression risk. `@contextual` covers the placement exemption; the **expansion**
  becomes an **expander callback on `Environment`**, deleting the 87-line drifting mirror. A missing
  expander is an **error on demand**, not silent degradation — forcing the CLI's generic paths to
  supply the wdoc Environment. The **template** check stays wdoc's (four of 03's six severity rows are
  irreducibly wdoc's); only the derivation is shared. `TypeRef` grows **syntax-only generics**
  (Wil overruled the recommended `content` → `list<T>` desugar), which is enough because the
  derivation emits `@children(SvgBlock)` alongside the typed field and *that* does the checking. Also
  **corrected 05**: this seam does **not** reshuffle the `wdoc` namespace, so `FieldShape` carries no
  ordering constraint from here — but derived schemas must be reachable through it.

- [The constructor DSL — what does writing a fundamental actually look like?](issues/15-constructor-dsl.md)
  — **one generic `el` family over the HTML element vocabulary and nothing else**, which **narrows 05
  decision 7** from three vocabularies to one. `SvgFundamental` and the content IR keep the named-field
  literal and get **shortened union names** instead — free under 05 decision 11, ~6,500 chars repo-wide,
  zero safety loss, and worth **28% of the DSL's entire win** on its own. Per-tag constructors were
  rejected (3% smaller for 60% more definition and a worse `id` cliff). Two of the ticket's premises
  died on measurement: **the "unsolved half" doesn't exist** (40 `attrs:` sites repo-wide, **1**
  conditional, **0** conditionally-*present*), and the free fixes are **−11%**, so 02's headline was
  inflated by about a quarter, not by most of it. The generic DSL is −38% on subject code, break-even
  at ~14 sites, against **258 real sites**. Also: **05 undercounted the corpus 4×** (258, not 63 — and
  only 57 are stdlib), **05's headline `class` example does not parse** (else-less `if` is rejected —
  so this ticket asks for one `wcl_lang` change), and a DSL over the content IR is a **+37% regression**.
  **Prototyped** ([`proto-15-constructor-dsl/`](proto-15-constructor-dsl/), executable against the
  shipped stdlib): every candidate authoring builds the **same value** as today, and the safety probe
  shows a misnamed field is a hard error while two transposed `f64`s through a positional constructor
  are **silent** — which, priced against the free rename (SVG's marginal win is ~1,542 chars against
  HTML's ~16,500, with 56 of 71 sites on interchangeable `f64`s), is what dropped SVG from the scope.

## Not yet specified

- **Migration sequencing.** Once the type system and template layer are settled, in what order do
  the docs site, `examples/`, the four in-repo wskills and WAD move — and is there a mechanical
  migration tool or is each hand-done? *Sharpened by ticket 05, not yet sharp enough to ticket: the
  block-layer migration shape is now known (each of the 34 page-content blocks becomes a content-IR
  variant or an `@native`, by 05's payload-shaped/subtree-shaped test), and three mechanical sweeps
  fall out of it — deleting 204 `: none` args, porting `class` to none-dropping lists, and rewriting
  the six markup-using content blocks. What's still foggy is ordering against the template layer and
  whether any of it can be automated. **Ticket 06 adds a fourth sweep of a different character: 736
  `related` edges each need a `why` written.** ~~It is the only migration on the list that is
  agent-doable but **not mechanical** — the reasons cannot be generated from the ids~~ — **RESHAPED by
  ticket 07, which made it mechanical after all.** Since the curator authors `why` forward-only and
  never backfills, the flip **deletes every bare edge except where the target id or title already
  appears in the source unit's prose** — a computable test leaving **~60–95 survivors** flagged for an
  author pass, not 736. So it is now a **tool-shaped sweep** like the other three, plus a small human/
  agent tail. ~~What's still foggy: whether that tool is a `wcl wskill` subcommand (ticket 08 has the
  question) or belongs to the migration effort~~ — **ANSWERED by ticket 08: the tool is
  `wcl wskill lint --fix`**, an autofixing lint rule emitting `related_remove` ops, not a bespoke
  subcommand. **08 also pins this sweep's ordering, previously unconstrained**, because it made `why`
  schema-required: a bare edge won't *parse* under the new schema, so the flip cannot run after the
  schema tightens. The order is **(1)** ship `wcl_wskill` with `why` optional → **(2)** run the flip
  (736 → ~60–95) → **(3)** author the survivors' reasons → **(4)** tighten `why` to required (one line,
  gated on 3). Step 4 is cheap and easy to forget, and forgetting it leaves 06's whole finding
  unenforced. Still foggy: where this tail sequences against the block-layer sweeps.* **Ticket 12 adds
  two small template-layer migrations of its own**, both mechanical and both independent of the above:
  rewriting `presentation` as an ordinary collection template (deleting `build.rs:1483`'s
  `default_template == "presentation"` comparison and `build_presentation_page`'s dedicated path), and
  tagging `sites` on any page in a multi-site document that lacks it — measured at approximately zero
  work, since `docs/main.wcl` and `examples/wdoc/main.wcl` already tag everything.
- **A fifth sweep, from ticket 08: deleting the duplicated schema and templates.** Every wskill's
  `schema/base.wcl` plus its **14 topic-agnostic wdoc templates** become imports from a registry
  embedded in `wcl_wskill`, so the four in-repo wskills each shed ~15 files and four drift-policing
  justfile recipes are deleted outright. The three legitimately-divergent files (`wcl`'s `common.wcl`,
  `wcl` + `wdoc`'s `book/main.wcl`) must switch from the aggregate `import <wskill.wcl>` to enumerated
  part imports. Mechanical, but it touches every wskill entry file — foggy only in where it sequences
  against the other sweeps, and whether it lands before or after the `related` flip (both edit the same
  wskills).
- **A seventh sweep, from ticket 14: teaching the CLI's generic paths that a document is wdoc's.**
  14 decision 5 made a missing expander a **hard error on demand**, and `wcl check` opens with a plain
  `Environment::new()` (`main.rs:1383`) while the wdoc registry is threaded separately as a *loader*
  (`main.rs:33`). So `wcl get` / `eval`, the LSP and the editor's open paths must supply the wdoc
  Environment when a document imports `<wdoc.wcl>`. Small and self-contained, and unlike the other
  sweeps it touches **no** `.wcl` — it is Rust in `crates/wcl` + `crates/wcl_lsp` only, so it doesn't
  contend for files with any of them. Foggy only in whether "is this document wdoc's?" is decided by
  the import list or by something explicit. **It must land with 14's implementation, not after** —
  between the two, those commands fail loudly on every wdoc document.
- **An eighth sweep, from ticket 15: the constructor port and the union rename.** Porting **258**
  `HtmlFundamental::Element` sites to the `el` family, plus renaming both unions across all **329**
  construction sites, plus deleting the 204 dead `: none`s (already on the list as one of 05's three
  sweeps — 15 confirms it is the same edit and should ride along). Mechanical and almost certainly
  scriptable, but **only 57 of the 258 sites are stdlib**: the rest are
  `docs/pages/wcl/landing-parts.wcl` (51), the scaffold templates (66) and every wskill projection
  (~56), so it **contends for exactly the same files** as ticket 13's CSS sweep and ticket 08's
  schema/template de-duplication. Foggy only in ordering against those two — and note the rename half
  wants to follow 05's codegen (the unions are generated from WCL by then, so the name is a one-line
  change at the source rather than a sweep). Carries **one `wcl_lang` change of its own** — the
  else-less `if` — which is independent of every sweep and can land whenever.
- **A sixth sweep, from ticket 13: the CSS migration.** **477 rules** relocate out of 35 heredocs,
  `theme.rs`'s `APPLY`/`FONT_DEFAULTS` and `assets/code-theme.css` into `class` / `base` / `font_face` /
  `media` / `keyframes` blocks, grouped by root selector (`.book-sidebar`'s 18 rules collapse into one).
  Driven by a **throwaway uv/tinycss2 script** — the `.wad/scripts/` precedent — with the ~20
  selector-list rules and the `:root` accent line hand-finished, plus a schema prune (11 dead `Class`
  properties, 20 field uses → `css`). **CORRECTED by the prototype — it is NOT self-contained.** Charting
  and the resolution both assumed it touched only `crates/wcl_wdoc`; there are **8 more heredocs carrying
  129 rules outside the stdlib** — `docs/pages/wcl/landing-parts.wcl` plus every wskill's
  `wdoc/book/main.wcl` and `wdoc/training/main.wcl`, and two in `.wad/`. So it edits the docs site, all
  four wskills and WAD, and **does** contend for the same files as the `related` flip and the
  schema/template de-duplication. Still foggy: whether it lands before or after ticket 05's block-layer
  sweeps — it adds four `@block` types on top of 05's type system, so it wants to follow rather than
  precede.
- **How WAD's view set lands on the new template layer.** *Largely answered by ticket 11 — WAD barely
  touches the template layer, so its migration is about the block/data layer plus the
  `toc { chapter … page = <name> }` string contract, not the slot contract. The `wdoc_book_layout` cost
  is now answered too: ticket 02 established it is **computation, not layout**, and the memoised
  page-metadata builtin is the fix.* What remains foggy: whether the 140-of-252 repeaters that are
  disguised `if` statements want a different primitive.
- **A blog as a consumer.** Ticket 02 surfaced Wil's target use cases — landing pages *and eventually a
  blog*. Landing pages are already proved by the docs site. A blog is not: dated collections, list/index
  pages over them, feeds, possibly taxonomies. Note it would be a **new build**, not one of the
  migrations the map's Out of scope rules out. *Ticket 03 settled one piece: a **list layout may declare
  no `content` slot** — that case is what decided the body hole must be declared rather than magic.*
  **Ticket 12 cleared most of the rest and left exactly two hard gaps.** Selection needs nothing (a blog
  is a `site`); dated collections need nothing (posts are **data**, and `sort_by`/`group_by`/`take`/
  `slice`/`unique` all exist in `collections.rs`); taxonomies need nothing (a repeater over a grouped
  gather). What is genuinely missing:
  - **No date handling anywhere in `wcl_lang`** — no `now`, no date type, no parse/format builtin. ISO-8601
    strings sort correctly, so *ordering* works; *displaying* "1 August 2026" means authoring the string
    or doing string surgery. A small, self-contained builtin question.
  - **No mechanism to emit a generated non-HTML output file**, which is what a feed is. The `file` block
    only copies from disk (`file.wcl:23`), and 12 ruled collection templates HTML-only with no filename
    control. This is the one that could want its own ticket — but only once someone actually builds the
    blog, since the shape of the answer (a generalised `file`? a non-HTML target? a second fundamentals
    vocabulary per 05?) depends on how much more than feeds it has to carry.
- **What Design mode is missing for wskill work specifically**, as against generic wdoc pages.
  *Ticket 10's fifth bullet, returned unresolved: that session spent its fidelity on the audit
  surface, and the remainder isn't sharp enough to state as a decision. What 10 does hand it: the
  editor now grows an audit view, a search box and a curator trigger, so ask this again once those
  exist — the answer may be smaller than it looks, or may be the "fuller editor rework" Wil flagged
  on 08 (*"we will have to rework editor later I think"*).*
- **Whether comment pins scale as the findings surface at volume.** *07's narrowed question, still
  open. Ticket 10 routed **diff-scoped** findings to row tags in the audit view, which sidesteps it
  there — but the standing surfaces (graph, content modal) still have to show findings on units
  nobody just touched, and `comments.wcl` with `author = "curator"` is where they live. Sharp enough
  to ticket only once the lint rule set is producing real volume on a real wskill.*
- **WAD improvements beyond substrate fit.** Wil flagged WAD as "another area that needs some work".
  This map treats WAD as a *consumer* that proves the substrate. Whatever remains wrong with WAD
  after it lands on the new substrate may need its own effort — revisit once the port is specified.
- **Whether the four projections stay four.** book / ai_skill / training / presentation are separate
  template sets over one model today. *Ticket 03 removed one way this could have resolved: the slot
  contract does **not** collapse them, and in fact hard-codes their independence — fills are bare names
  because one unit body must render under all four (Wil: "make sure we can do that projection or wskill
  breaks"). What remains foggy is whether the projections want restructuring for their own reasons.
  Ticket 06 added one concrete divergence to carry into that question: the book gives a body-carrying
  index its own page while the skill has **no per-kind index pages at all** (`skill/main.wcl:9`,
  inlined into SKILL.md), so making indexes linkable forces the skill to grow pages the book already
  has. A second small instance of the same asymmetry.* *Ticket 12 removes one structural obstacle to
  a fifth: the deck stops being a privileged built-in and becomes an ordinary **collection template**,
  so a new whole-site-as-one-file projection is now declarable rather than a Rust change.*
- **What replaces `audience` scoping.** `Audience` (`:book`/`:ai`/`:both`) plus the `@only`/`@except`
  visibility system are two overlapping mechanisms for "which projection renders this". A typed
  template layer may subsume one. *Ticket 05 loaded the `@except` side further: its `backends` axis
  becomes the per-instance waiver for a block used on a target it can't render, and gains the missing
  `:skill` symbol. So `@except` is now doing three jobs — sites, backends-as-intent, and
  backends-as-capability-waiver — which makes the overlap worth looking at sooner.*
- **CI shape after consolidation.** ~~Once the seven recipes become `wcl wskill` subcommands~~ —
  *sharpened by ticket 08, which settled the disposition of all eight: **four die** with the
  duplication they police, **three fold into `wcl wskill check`** (artifact-check, coverage,
  skills-check's projection half) with skills-check's install-drift half going to
  `wcl wskill install --check`, and **`wcl-refcheck` stays** as WCL-repo housekeeping. Severities are
  settled too — lint errors fail, warnings don't, `--deny warn` escalates, `candidate` never fails
  anything. What's left foggy is only the assembly: whether `just ci` calls `check` per wskill or
  once over `docs/wskills/`, where `install --check` sits relative to `just skills-install`, and
  whether the `--deny warn` escalation is on in this repo. **Ticket 10 adds a sixth subcommand,
  `audit`**, which is not a CI gate but carries a constraint the assembly must respect: it loads a
  wskill's model **at a git revision**, so `crates/wcl_wskill` needs the gitspec plumbing that today
  lives in `crates/wcl` (`src/gitspec.rs`) reachable from the library.*

## Out of scope

- **Migrating out-of-repo wskills and pages.** They're already broken and need migrating regardless;
  that happens as a separate effort after this lands.
- **Backwards compatibility for anyone outside this repo.** Explicitly ruled out — a compat shim
  preserves the seam being killed.
- **Hand-editing `.wcl` ergonomics.** Not Wil's loop; the browser editor is.
- **Redesigning the PDF and Markdown backends.** They must keep rendering, so they are *constraints
  on* the new type system — not subjects of it.
- **i18n.** Ruled out by Wil on ticket 02 when scoping what "Hugo parity" means. Not a gap to close.
- **`baseof`-style template inheritance.** Same call. A layout does not extend another layout.
- **A theme system** — drop in a folder, get its whole look, override one layout and inherit the rest.
  Follows from the two above: Hugo themes lean on templates being *files* overridable one at a time,
  and ticket 02 chose WCL templates over files. Reopening themes means reopening that decision.
- **Sibling `.css` files.** Ruled out on ticket 13 — not merely unchosen, and note ticket 02 had
  originally decided *for* them before that decision was retracted. They buy editor support for free
  (CodeMirror already maps `css`, `EditorPane.jsx:18`) but at 23 files of ~11 lines each they fragment
  the co-location commit `5ee5d88f` deliberately created, and they check nothing. `code-theme.css`, the
  codebase's only sibling stylesheet, folds into the block vocabulary.
- **Full generics in WCL** — substitution, generic fields, `type Pair<A, B>` genuinely working. Ticket 14
  took **syntax-only** generics (`TypeRef::Named { path, args }`: parse, print, round-trip; no arity
  check, no substitution, no declaration form) because that is all the slot accepts-type needs — the
  `@children(X)` decorator does the checking. Real generics are a language-design effort in their own
  right and nothing on the route to this destination wants them. If they're ever wanted it is a **fresh
  effort**, not a resumption of this map.
- **Model A, an external `.html` template language.** Ruled out on ticket 02 — not merely unchosen. It
  works (the prototype runs), but it costs a hand-written second language in a repo whose conventions
  forbid parser generators, and the paste-a-design story that justified it is dead.

## Incidental defects found while charting

Real, actionable, and **not** part of this effort — nothing here is a decision. Recorded so they aren't
lost when the research findings are archived. File separately.

1. **`node_table` in a diagram loses all row text in PDF.** `card_rects` counts every `foreignObject`
   while `collect_card_blocks` counts only `card` blocks, so the counts disagree and every box renders
   empty. (ticket 04)
2. **No `wad-template-check`, and WAD's scaffold copy has drifted.** All 14 WAD projection files are
   duplicated into `crates/wcl/src/scaffold/templates/wad.wcl` with **no drift gate** — `wad-schema-check`
   covers only `schema/base.wcl`, while both `wskill-template-check` and `wplan-template-check` exist
   (verified). 11 of 14 are byte-identical; **3 have drifted**, including a `routing = :straight` CI-failure
   fix stranded in the live copy and absent from every newly-scaffolded WAD — so `wcl init wad` currently
   scaffolds a document with a known CI failure in it. (ticket 11)
3. **`crates/wcl_wdoc/src/kinds.rs:1-8` is factually wrong.** It states the three backends "special-case
   the same block vocabulary"; three of its own constants are referenced by a strict subset. (ticket 04)
   *Ticket 05 dissolves most of `kinds.rs` along with the comment — fix only if the refactor slips.*
4. **`file` blocks in PDF** render and ship nothing — no `pdf/` dispatch site exists. Whether this is
   deliberate is unresolved (a question of intent, not behaviour). (ticket 04) *Ticket 05 forces the
   answer: `@native` coverage plus a build error on uncovered use means someone must implement it or
   waive it explicitly. Don't file this one — 05 resolves it.*
5. **CLAUDE.md's wdoc feature map misdescribes `code` and `math`.** It lists them among the blocks
   "special-cased in Rust with stub WCL `lower`s"; both have *real* lowers emitting `Highlighted` /
   `Math` leaf variants. (ticket 05)
6. **`crates/wcl_wdoc/lib/tui.wcl:33` states a language rule that isn't true** — *"Every field must be
   supplied at construction (set `none` to opt out) — that's how WCL variant literals work."* Optional
   variant fields already default to `none`; the comment has cost the stdlib 204 dead arguments.
   (ticket 05)
7. **6 `related` ids in the `wskill` wskill resolve to nothing and render silently** —
   `ripgrep → ignore_rules`, `exit_codes → validate_format`, `commands → git_add`, +3. A dangling id
   produces no bullet and no warning. (ticket 06) *Ticket 06 decision 7.3 adds the missing check, so
   fix it there rather than filing separately.*
8. **Reciprocal `related` edges render the same link twice on one page** — "Related" and "Referenced
   by" are computed independently with no dedup, and **32–48%** of edges are reciprocal. (ticket 06)
   *Resolved by ticket 06 decision 8; don't file.*
9. **The `Index` schema doc-comment is stale.** `docs/wskills/*/schema/base.wcl` claims an index
   "renders as a link-collection page" in the skill; `skill/main.wcl:9` says *"There are NO per-kind
   index pages"* — they are inlined into SKILL.md. (ticket 06)
10. **`entity wil_taylor` is triplicated** across the wcl, wad and wskill wskills with identical
    content, with nothing keeping the three copies in sync. (ticket 06)
11. **`crates/wcl_wdoc/src/render/css.rs:1-13` is factually wrong.** It states *"The lone Rust-side CSS
    that remains is `highlight::theme_css()`"*; `theme.rs`'s `APPLY` is **~84 hand-written rules**
    covering headings, links, code cards, book chrome, syntax tokens and every diagram shape. (ticket 13)
    *Ticket 13 decision 6 dissolves this along with the CSS it misdescribes — fix only if the refactor
    slips.*
12. **The syntax-token classes are defined twice.** `.tok-comment`, `.tok-keyword`, `.tok-string`,
    `.tok-type` and the rest appear in `assets/code-theme.css` *and* again in `theme.rs`'s `APPLY` as
    `var(--wdoc-syn-*)` versions — two mechanisms, two files, one vocabulary. (ticket 13) *Resolved by
    ticket 13 decision 6; don't file.*
13. **Fn signatures are second-class: argument types are never checked, and `?` is a parse error.**
    `elem("div", none, none, [])` evaluates clean against parameters declared `identifier` and
    `list<utf8>` — the annotations are documentation, not constraints (arity *is* checked). Separately,
    an optional parameter type is rejected by the parser, and there are **zero** in the stdlib. Both
    verified; both shaped ticket 15's answer, and the first is why a positional constructor over a
    field-shaped variant fails silently. (ticket 15) *Not part of this effort — but if argument
    checking ever lands, revisit 15 decision 3: it is half of why SVG kept the named literal.*
