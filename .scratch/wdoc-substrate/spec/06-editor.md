# 06 — The editor

Source: ticket [10](../issues/10-editor-review.md), with the breakage inventory from
[11](../issues/11-wad-seam-survey.md) and the constraints handed on by
[03](../issues/03-slot-contract.md), [07](../issues/07-curator-contract.md),
[08](../issues/08-wskill-cli.md) and [09](../issues/09-research-flow.md).

The browser editor is **the human loop** for this whole effort — Wil does not hand-edit `.wcl`
(see [08](08-open.md), Out of scope). So it is both the biggest breakage surface and the place the
wskill half's review story has to land.

---

## 6.1 What breaks, and why it breaks silently

### 6.1.1 Schema introspection rests on printed-string comparisons

The WAD Systems view derives its **entire model** from `kind_links` (`blocks.rs:1526-1579`) plus
`gathered_kinds` (`:1442-1467`) and `gather_elem_decl` (`:1474-1485`). Its load-bearing tests are
string equality on rendered type names:

| test | site | broken by |
|---|---|---|
| `bare_type(f) == "identifier"` | `blocks.rs:1542` | any field reclassification |
| `bare_type(f) == "list<identifier>"` | `blocks.rs:1558` | same |
| `to_string().starts_with("fn")` | `blocks.rs` | interface changes |
| `full_name().starts_with("wdoc.")` | `blocks.rs:1460` | [02](02-blocks.md)'s renames |
| `is_descendant_of("wdoc.SvgBlock")` | `blocks.rs:1615` | `WdocBlock` → `ContentBlock` |

**Several fail silently** — a reclassified field just stops being a parent link, with no error. This is
the concrete breakage risk the type-system refactor carries, and it is this map's own theme in
miniature.

**Fix:** [01](01-language.md) §1.6's `TypeField::shape() -> FieldShape`, swapped in **with**
[02](02-blocks.md)'s renames, never after.

Note `is_descendant_of` appears **only** at `blocks.rs:1615`, gating `diagram_kinds` — **not** the C4
model. So the Systems view survives an interface rename; it is the `bare_type` comparisons that carry
the risk.

### 6.1.2 The palette must route through derived schemas

[01](01-language.md) §1.2 makes component kinds' schemas **lazily derived** rather than present in
`type_decls()`. Anything introspecting by walking declarations will not find them — including
`/api/palette`, which special-cases `wdoc_component` at `blocks.rs:1686`.

### 6.1.3 Eight hardcoded wdoc block names in `crates/wcl`

`editor/blocks.rs:1686,1699`, `editor/nav.rs` ×3, `editor/mod.rs`. These are **legitimate** — the editor
is a wdoc consumer and is allowed to know wdoc — but they break on [02](02-blocks.md)/[03](03-templates.md)'s
renames, so they are migration surface. (`crates/wcl` was uncounted by the original 12-site survey.)

### 6.1.4 Anchors must survive the render-pipeline change

Design mode's entire click-to-edit surface rests on `data-wcl-span` / `data-wcl-file` stamped by
`anchor_block` under `InlinePatterns::edit_mode()`. Under [03](03-templates.md) §3.2, render time
becomes **handle-resolve time** — the anchors themselves are fine, but the stamping site moves.

### 6.1.5 A live provenance bug the slot work fixes

`build.rs:2079` wraps only `content`, while regions render at `build.rs:2050` — *before* the wrapper —
so **a block inside a region has no page-provenance ancestor today**, and the client cannot locate its
`comments.wcl` sidecar or the file to edit. [03](03-templates.md) §3.6.10's per-slot wrapper fixes it.

---

## 6.2 Slots become an editing surface

From [03](03-templates.md) §3.6.10, and Wil's ask: *"think we need to show all the slots and allow
editing them."*

- One `display:contents` wrapper **per resolved slot**, carrying the page attrs plus `data-wcl-slot`.
- **Emitted for unfilled slots too, in edit mode** — an invisible hole cannot be filled by direct
  manipulation (same reasoning as the wireframe empty-container placeholder).
- **"Which slot does this block live in" becomes a DOM ancestor lookup** — structurally identical to the
  page-file lookup the client already does. No new client mechanism.
- **A slot rendering its layout-declared fallback is layout-owned content**, so its wrapper points
  provenance at the *layout's* file/span. Clicking a default footer must not try to edit a page that
  never wrote one.

---

## 6.3 The audit view

Prototyped against **real data** — the `wcl` wskill's unit graph extracted at two git revs and diffed,
with `99518181 docs(wskill/wcl): atomize language reference…` as the subject: a genuine 30-unit
authoring commit (**+30 −5 units, +160 −19 edges, 8 new hubs**). See
[`proto-10-editor-review/`](../proto-10-editor-review/).

### 6.3.1 The measurement that decided most of it

**Every one of the 30 new units landed unpinned.** The `language` index that houses them was not
authored until `ec19b309`, three commits later. An entire language reference was written with no home
in the index tree, and nothing said so at the time.

That is [05](05-wskill.md) §5.5.2's place-before-write failing in the historical record, and it is
exactly the class of damage **a file diff cannot show you**. The audit view's whole justification is
that it makes this visible in one screen.

Two of the three candidate representations failed on inspection:

- **A health report names no unit.** 4 of 10 metrics moved worse (edges/unit 2.11 → 3.39, hubs 3 → 11,
  over-cap units 0 → 10, reasonless edges 92 → 208) and told you nothing about *who*. It is the shape of
  the curator's gate, not of a review.
- **A graph overlay cannot show a deletion.** The editor's graph renders the after-state, so this
  commit's −5 units and −19 edges were simply absent from it. **A surface that cannot show a removal
  cannot audit an agent that removes.**

### 6.3.2 One audit view, two representations, health as a header strip

The changelog **is** the surface (Wil: *"A, and B is just its header"*). The health metrics do not get a
tab; the ones that got **worse** render inline in the header alongside the counts, so the triage number
and the named units are on one screen. **There is no separate health pane.**

### 6.3.3 The audit graph is the union graph — before ∪ after — with removals ghosted

Removed units and edges render in dashed red alongside the live ones.

**This is the one thing the editor's live graph structurally cannot do**, and it is why the audit graph
must be a distinct view over a distinct model rather than a mode on `GraphView`: the live graph draws
what exists, and **half of an audit is what stopped existing**.

### 6.3.4 Findings ride the changed rows, scoped to the diff

Rather than a parallel findings list, each changed unit carries its own findings as tags, so the view
answers *"what changed, and what is wrong with what changed"* in a single pass. `unpinned` is literally
lint warning #2 from [05](05-wskill.md) §5.3.4; `over cap` and `hub` are two more.

**Findings on untouched units are not this view's job** — they belong to the standing surfaces (graph,
content modal), which already have somewhere to put them.

**The three severities keep their distinction in the tag styling. A `candidate` must never render as an
error** — `candidate` findings are *nominations, not defects*, and badging them identically would
misrepresent the whole mechanical/judgement split.

### 6.3.5 It is a thin adapter over `wcl wskill audit`

`wcl wskill audit [<rev>..<rev>] --format json` emits the audit model; the editor tab renders it,
exactly as [05](05-wskill.md) §5.2 specifies for the rest. Range defaults to `HEAD~1` and accepts an
arbitrary git range.

---

## 6.4 Plain search, everywhere

**No search exists anywhere in Design mode today** — grepping the whole `editor-ui/src/components/design/`
tree returns **one** hit and it is a colour constant. Not in the graph view, not in the index panel, not
in the content modal, at 65 units.

One find-a-unit box over id / name / summary / body that jumps the graph, index panel or canvas to the
hit. The boring answer, and the missing one.

It is **not** a coverage probe (the "is this already covered before I add" idea was offered and not
chosen) and it does **not** replace fixing index coverage — those are separate questions.

---

## 6.5 The "curate this index" button

The editor gets a trigger for a curator pass, and **when that pass commits, the audit view opens on that
commit**. Run it → read what it did → `git revert` if bad.

**That sequencing is the point**: the button is only worth having because §6.3's surface exists to read
its output. Without the audit view a button would just be a headless job with a progress spinner.

Scope is not restricted to an index — whole-graph passes are triggerable too, consistent with
"on demand, never automatic".

**It is a convenience over a working CLI, not a dependency.** The curator's phase 1 is exactly
`wcl wskill lint --format json --severity candidate` and a whole-graph pass is `wcl wskill op` against
the wskill root; both run headless with no editor required.

**There is no "watch it" mode.** The curator runs headless, edits directly, and takes **no human gate**
— Wil's explicit call. The editor's job is **post-hoc audit of a commit**, not supervision.

---

## 6.6 The NavPanel edits an authoring skeleton now

From [05](05-wskill.md) §5.5: the index tree is authored **before the units exist**, from the scoping
interview, and every node carries a `body`.

- The NavPanel already edits `index` blocks and `related` lists — but it now edits a structure that is
  the *authoring* skeleton, not just navigation, and **it must be able to give a node a body**
  (a bodied index is a linkable node with its own page). Today only 6 of 53 indexes carry one.
- The add-unit flow (`unit_create` + optional pin, one multi-file commit) already asks for placement, so
  it is *ahead* of the shipped process — the process is moving to match it. **Check the reverse case:
  creating a unit with no pin should now be the exceptional path.**
- A research run ends with a **whole-graph** curator pass, so the review surface must cope with findings
  spanning the entire graph, including `object_kind`/`object_id` findings with **no page to pin to**.

---

## 6.7 The adapter is interim, deliberately

[05](05-wskill.md) §5.2 makes `/api/graph` and `/api/nav/op` thin adapters over `crates/wcl_wskill`.
**Wil expects a fuller editor rework after this** — *"we will have to rework editor later I think but
for now this is good."* The adapter was chosen so the CLI could land without blocking on an editor
redesign.

**Do not treat the adapter as settled architecture.** If the rework happens, the adapter is the first
thing to reconsider.

---

## OPEN

- **What Design mode is missing for wskill work specifically**, as against generic wdoc pages. Ticket
  10's fifth bullet, returned unresolved — that session spent its fidelity on the audit surface. Ask
  again once the audit view, the search box and the curator trigger exist; the answer may be smaller
  than it looks, or may be the fuller rework above.
- **Whether comment pins scale as the surface for machine-generated findings at volume.** §6.3.4 routes
  *diff-scoped* findings to row tags, which sidesteps it there — but the standing surfaces still have to
  show findings on units nobody just touched, and `comments.wcl` with `author = "curator"` is where they
  live. `author` is the discriminator that exists today. Sharp enough to ticket only once the lint rule
  set is producing real volume on a real wskill.

## Checklist for this part

- [ ] `FieldShape` swapped in **with** [02](02-blocks.md)'s renames; Systems view verified against a real WAD
- [ ] Palette routes through derived schemas
- [ ] The 8 hardcoded wdoc block names in `crates/wcl` updated
- [ ] Anchors re-stamped at handle-resolve time; Design mode click-to-edit verified end to end
- [ ] Per-slot wrappers rendered incl. unfilled; slot ancestor lookup; layout-owned fallback provenance
- [ ] `wcl wskill audit` + the audit tab: union graph with ghosted removals, health header strip, findings as row tags
- [ ] Search box over id / name / summary / body
- [ ] "Curate this index" trigger opening the audit view on the resulting commit
- [ ] NavPanel can give an index a `body`; unpinned unit creation becomes the exceptional path
