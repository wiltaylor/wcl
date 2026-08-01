# Map: wdoc substrate refactor

Label: `wayfinder:map`

## Resume

```
/wayfinder .scratch/wdoc-substrate/map.md            # take the next frontier ticket
/wayfinder .scratch/wdoc-substrate/map.md ticket 02  # take a specific one
```

**Frontier (open, unblocked, unclaimed): 02, 05, 06.** Resolved: 01, 04, 11.

Tickets live in `issues/NN-<slug>.md` — a `Status:` line (`open`/`claimed`/`resolved`) and a
`Blocked by:` line near the top; a ticket is unblocked when every file it lists is `resolved`.
**Claim a ticket by setting `Status: claimed` before doing any work.** Research findings are in
`research/`. One ticket per session (research excepted).

Read the **Measured facts** and **Decisions taken while charting** sections below before choosing —
they exist so a fresh session doesn't re-derive them, and two facts carry annotated corrections. The
downstream tickets 02, 03 and 05 each end with an **"Inherited from …"** section recording what an
upstream resolution already settled; don't re-litigate those.

## Destination

A **spec** for a refactored wdoc substrate — an honest block/type system, and a typed HTML
templating layer where you author real HTML templates and render wdoc content into declared
slots — **with the in-repo wskills and WAD migrated onto it as the proof it works**. Detailed
enough to break into tickets and hand to agents.

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
- **Prose guidance is not a mechanism.** The single most load-bearing finding below is that the
  wskill authoring rules are already written, already shipped to the agent, and already ignored. Do
  not resolve a ticket with "document it better".
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

**The template seam is stringly-typed.**
- `TemplateCtx.content: utf8` — the page body reaches a template as **opaque pre-rendered HTML**.
- `Region { name: utf8, content: utf8 }` — named regions likewise, keyed by **unchecked string
  name**. `wdoc_region(c, "heor")` silently returns `""`.
- A layout cannot declare the slots it needs; a page cannot be validated against them.
- **4** templates total (`webpage`, `book`, `website`, `presentation`).
- No middle gear for authoring: either construct typed `HtmlFundamental::Element` records
  (`website_header` = 25 lines of WCL for one `<header>`) or drop to
  `HtmlFundamental::Raw { html: <<HEREDOC }` and lose every guarantee.

**The wskill graph has collapsed.**
- The `wcl` wskill: **45 concepts, 2 entities, 18 facts**. The kind vocabulary is not discriminable
  in practice — almost everything becomes a `concept`.
- One unit carries **19** `related` ids, another 12; `linking_discipline` caps them at 3–5.
- `docs/wskills/wskill/data/fact/linking_discipline.wcl` already states the cap, the "a link costs
  two pages" rule, the hub-note anti-pattern ("a menu wearing a page's clothes") and a symptom
  table. `unit_decision_guide` and `writing_style` sit beside it. All are `audience = :both`, so they
  ship to the agent. **They are ignored.**

**The wskill toolchain is scattered and incomplete.**
- `crates/wcl/src/editor/graph.rs` (639 lines) **already computes** the structural model a curator
  needs — units, nested index trees, `related` edges, pin edges, per-unit block lists,
  `related_editable` flags, unindexed detection. It is locked behind an editor HTTP endpoint.
- There is **no `wcl wskill` subcommand at all**.
- The seven gates that do exist live in the repo-root justfile as grep/sed shell recipes:
  `wskill-check`, `wskill-coverage`, `wskill-crosstopic-check`, `wskill-artifact-check`,
  `wskill-template-check`, `wskill-schema-check`, `wskill-schema-sync`. **None checks graph shape.**
  Being repo-local, they contradict the format's own `selfcontained` philosophy unit.
- `docs/wskills/wskill/skill/` does not exist — the shipped AI skill bundles **zero** executables.

**Authoring processes derive structure from content.**
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

## Not yet specified

- **Migration sequencing.** Once the type system and template layer are settled, in what order do
  the docs site, `examples/`, the four in-repo wskills and WAD move — and is there a mechanical
  migration tool or is each hand-done? Can't be phrased sharply until the shape of the change is known.
- **How WAD's view set lands on the new template layer.** *Largely answered by ticket 11 — WAD barely
  touches the template layer, so its migration is about the block/data layer plus the
  `toc { chapter … page = <name> }` string contract, not the slot contract.* What remains foggy: whether
  the `wdoc_book_layout` cost (36% of a build, re-walking the toc 26082 times for 161 pages) is fixed by
  the new template layer or needs its own attention, and whether the 140-of-252 repeaters that are
  disguised `if` statements want a different primitive.
- **WAD improvements beyond substrate fit.** Wil flagged WAD as "another area that needs some work".
  This map treats WAD as a *consumer* that proves the substrate. Whatever remains wrong with WAD
  after it lands on the new substrate may need its own effort — revisit once the port is specified.
- **Whether the four projections stay four.** book / ai_skill / training / presentation are separate
  template sets over one model today. A typed slot contract might collapse or restructure them;
  can't tell yet.
- **What replaces `audience` scoping.** `Audience` (`:book`/`:ai`/`:both`) plus the `@only`/`@except`
  visibility system are two overlapping mechanisms for "which projection renders this". A typed
  template layer may subsume one.
- **CI shape after consolidation.** Once the seven recipes become `wcl wskill` subcommands, what the
  `just ci` gate looks like and which checks become hard failures.

## Out of scope

- **Migrating out-of-repo wskills and pages.** They're already broken and need migrating regardless;
  that happens as a separate effort after this lands.
- **Backwards compatibility for anyone outside this repo.** Explicitly ruled out — a compat shim
  preserves the seam being killed.
- **Hand-editing `.wcl` ergonomics.** Not Wil's loop; the browser editor is.
- **Redesigning the PDF and Markdown backends.** They must keep rendering, so they are *constraints
  on* the new type system — not subjects of it.

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
4. **`file` blocks in PDF** render and ship nothing — no `pdf/` dispatch site exists. Whether this is
   deliberate is unresolved (a question of intent, not behaviour). (ticket 04)
