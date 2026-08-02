# What does the editor need so you can audit AI output and navigate a big wskill?

Type: prototype
Status: resolved
Asset: [proto-10-editor-review/](../proto-10-editor-review/) — run
`python3 -m http.server 8791` there and open `audit-proto.html`
Blocked by: — (08 resolved)

## Question

Wil's editing pain is three of four loops: **Design mode**, **reviewing what an AI wrote**, and
**navigating a big wskill at all**. Explicitly *not* hand-editing `.wcl`.

The editor already ships a great deal — Design mode with click-to-edit and source-swap prose
sessions, the unit graph with a live force sim, an index panel with a full editable index tree, the
content modal with per-view visibility toggles and merged builds, per-block visibility gutters,
drag-to-reorder, the wskill view tabs. So "editing is bad" **despite all that** is the signal worth
chasing: the surfaces exist and don't answer the question you're actually asking.

Prototype the review surface. Take the `wcl` wskill as the subject — 65 units, 69 data files, one
unit with 19 `related` ids — and find what makes it auditable:

- **Reviewing AI output.** What did the agent just change, and is the shape sane? Today there's no
  diff view over the *graph* — you can see a file diff, but not "three units appeared, this one is
  now a hub, these five links were added". Is a structural diff the answer, or a health report, or
  something on the graph itself?
- **Surfacing lint findings where you're looking.** `08-wskill-cli`'s rule set produces findings; the
  graph view and content modal are where you'd act on them. Badges on offending nodes? A findings
  panel? The visibility gutter already doubles as a state indicator — precedent worth following.
- **Navigating 65 units.** The graph view exists and the index panel exists. Establish what's
  missing: search? "is this already covered" before adding? A path/neighbourhood view? The graph's
  filter chips fade rather than remove — is fading the right idiom at this scale?
- **Curator interaction.** The curator edits directly and headlessly. Do you watch it, review its
  output after the fact, or drive it from the editor? A "curate this index" button is a different
  product from a report you read afterwards.
- **What Design mode is missing for wskill work specifically**, as opposed to generic wdoc pages.

Blocked by `08-wskill-cli`: the review surface displays what the CLI computes, so the model and rule
set come first. Also downstream of `01-content-seam` / `03-slot-contract` — Design mode's
click-to-edit rests on `data-wcl-span` anchors surviving whatever the render pipeline becomes.

Link the prototype from this ticket as an asset.

## Inherited from ticket 03 (resolved)

**Show every slot a layout declares, filled or not, and allow editing them.** Wil's words:
*"think we need to show all the slots and allow editing them."* 03 put one `display:contents` wrapper
per resolved slot carrying the page attrs plus `data-wcl-slot`, **emitted for unfilled slots too in
edit mode** — an invisible hole cannot be filled by direct manipulation (same reasoning as the
wireframe empty-container placeholder). That wrapper is the editing surface, not just provenance.

Two details it hands you: "which slot does this block live in" becomes a DOM ancestor lookup,
structurally identical to the page-file lookup the client already does — no new client mechanism; and
a slot rendering its layout-declared **fallback** is layout-owned content whose provenance points at
the layout's file/span, so clicking a default footer must not try to edit a page that never wrote one.

## Inherited from ticket 07 (resolved) — do not re-litigate

07 settled the curator's contract and answered the **"Curator interaction"** bullet, while narrowing
the **"surfacing lint findings"** one.

**Curator interaction: you review its output after the fact, and there is no "watch it" mode.** The
curator runs headless as a terminal phase of every authoring session, edits directly, and takes **no
human gate** — Wil's explicit call, on the grounds that a mandatory review of every incremental pass
reintroduces the human into the loop the curator exists to remove, and a diff of 40 `related` edits is
exactly what a human skims and approves anyway. The backstop is **git**: clean tree at entry, one
commit per run, `git revert` for a bad one, plus a `--dry-run` that prints the op list. So the
editor's job here is **post-hoc audit of a commit**, not supervision — which sharpens this ticket's
first bullet (*"what did the agent just change, and is the shape sane?"*) into the primary question,
since the curator is now one more agent whose commits you must be able to read structurally.

A **"curate this index" button** remains open — 07 chose "on demand, but never automatic" for
whole-graph passes, and the editor is the obvious place to trigger one. That is a product decision
this prototype can take.

**Findings are already persisted, and already render where you're looking.** The curator files
everything it may not fix (hubs, non-atomic units, dishonest `why`s) as `comment` blocks with
`author = "curator"` in the wskill's existing `comments.wcl` sidecar — which the preview pane already
shows as pins. So the "badges vs findings panel" question is **not** about where to store findings; it
is about whether comment pins are a sufficient surface for machine-generated findings at volume, and
how they coexist with human review comments in the same sidecar. `author` is the discriminator that
exists today.

**A gap 07 handed you:** a finding about a unit with **no rendered page** — a body-less index, or a
unit whose page a given projection doesn't build — has no block to pin to, and "this unit shouldn't
exist" has no natural anchor at all. If comment pins are the findings surface, this is the case that
breaks them.

**Scale context for the navigation bullet:** the curator deliberately never holds the whole graph — it
works from a thresholded candidate list. If a human is expected to audit the graph whole at 65 units,
the editor is doing something the curator was explicitly designed not to.

**A live bug 03 found, worth confirming the fix in the prototype:** `build.rs:2079` wraps only
`content`, while regions render at `build.rs:2050` *before* the wrapper — so a block inside a region
has **no** page-provenance ancestor today, and the client cannot locate its `comments.wcl` sidecar.

## Inherited from ticket 08 (resolved) — do not re-litigate

08 built the CLI this ticket displays, and **closed the gap 07 handed you above**.

**The no-page finding is solved at the data layer, not the UI layer.** `Comment` gains
`object_kind: utf8?` + `object_id: utf8?`, `page` becomes optional, and at least one of the two is
required. A finding about a body-less index — or *"this unit shouldn't exist"* — is addressed to the
**object**, not to any rendering of it, mirroring the curator's id-addressed write set. The resolution
mechanism already exists: `/api/object/locate` takes `{kind, target}` and answers `{file, span}`. So
the question this ticket inherits is **not** "where do page-less findings go" but the narrower one 07
actually asked: whether comment pins are a sufficient surface for machine-generated findings *at
volume*, and how they coexist with human comments in one sidecar (`author` is the discriminator).

**The findings you surface have a fixed shape and three severities.** `wcl wskill lint --format json`
emits `{severity, rule, unit: {kind, id}, file, span?, message}` with `severity` ∈
`error` / `warn` / `candidate`. Three errors (dangling `related` id, duplicate unit id across kinds,
`related` naming a body-less index), three warnings (over-cap, unpinned, reaching zero projections),
five candidates (07's judgement screens). **`candidate` findings are nominations, not defects** — a UI
that badges them identically to errors would misrepresent the whole mechanical/judgement split. The
`span` field exists precisely so the editor can pin and jump.

**The model moves out from under you, deliberately.** All of `graph.rs`'s `NodeInfo` — spans and
`related_editable` included — becomes typed Rust in the new `crates/wcl_wskill`, and the editor keeps a
**thin adapter** that adds 60-char block previews, calls `layout_graph`, and serialises. The model
keeps spans specifically so this is not a degradation. `/api/nav/op` and `/api/graph` become adapters
over library functions; the editor's span-addressed `related_add` / `related_remove` become a
**resolver** that turns a drag into `(kind, id)` before calling the same op the curator calls.

**Wil expects a fuller editor rework after this** — *"we will have to rework editor later I think but
for now this is good."* The adapter is the interim shape, chosen so 08 could land without blocking on
an editor redesign. That rework is this ticket's territory if the prototype finds the adapter is the
thing in the way; don't treat the adapter as settled architecture.

**A trigger point for the "curate this index" button:** the curator's phase 1 is exactly
`wcl wskill lint --format json --severity candidate`, and a whole-graph pass is `wcl wskill op`
against the wskill root. Both run headless with no editor required, so an editor button is a
convenience over a working CLI — not a dependency.

## Inherited from ticket 09 (resolved) — do not re-litigate

09 made **both** authoring processes shape-first, which lands three things on the editor's surface:

- **The index tree is authored before the units exist**, from the scoping interview, and every node
  carries a `body`. The editor's NavPanel already edits `index` blocks and `related` lists — but it
  now edits a structure that is the *authoring* skeleton, not just navigation, and it must be able to
  give a node a body (ticket 06: a bodied index is a linkable node with its own page). Today only
  6 of 53 indexes carry one.
- **`adding_content` reorders to place-before-write**: the home is chosen before the prose. The
  editor's add-unit flow (`unit_create` + optional pin, one multi-file commit) already asks for
  placement — so it is *ahead* of the shipped process here, and the process is moving to match it.
  Check the reverse case: creating a unit with no pin should now be the exceptional path.
- **A research run ends with a whole-graph curator pass**, not the 1-hop one. Whatever this ticket
  decides about auditing AI output has to cope with a pass whose findings can span the entire graph
  and whose unfixables land in `comments.wcl` as `author = "curator"` (07) — including, per 08,
  `object_kind`/`object_id` findings with **no page to pin to**.

Distillation's pass A/B split is a process obligation, not a tool: 09 explicitly needed **no** new
`wcl wskill` subcommand.

---

## Resolution

Prototyped against **real data**: the unit graph of the `wcl` wskill extracted at two git revs and
diffed, with `99518181 docs(wskill/wcl): atomize language reference…` as the subject — a genuine
30-unit authoring commit (**+30 −5 units, +160 −19 edges, 8 new hubs**). Three candidate
representations were built and compared side by side; the artifact now carries the settled shape.
See [proto-10-editor-review/README.md](../proto-10-editor-review/README.md).

### The measurement that decided most of it

**Every one of the 30 new units landed unpinned.** The `language` index that houses them was not
authored until `ec19b309`, three commits later. An entire language reference was written with no
home in the index tree, and nothing said so at the time — this is ticket 09's place-before-write
failing in the historical record, and it is exactly the class of damage a file diff cannot show
you. The audit view's whole justification is that it makes this visible in one screen.

Two of the three candidate representations failed on inspection:

- **A health report names no unit.** 4 of 10 metrics moved worse (edges/unit 2.11 → 3.39, hubs
  3 → 11, over-cap units 0 → 10, reasonless edges 92 → 208) and told you nothing about *who*. It
  is the shape of ticket 07's gate, not of a review.
- **A graph overlay cannot show a deletion.** The editor's graph renders the after-state, so this
  commit's −5 units and −19 edges were simply absent from it. A surface that cannot show a removal
  cannot audit an agent that removes.

### Decisions

**1. One audit view, two representations, health as a header strip.** The changelog *is* the
surface (Wil: *"A, and B is just its header"*). The health metrics do not get a tab; the ones that
got **worse** render inline in the header alongside the counts, so the triage number and the named
units are on one screen. There is no separate health pane.

**2. The audit graph is the union graph — before ∪ after — with removals ghosted.** Wil chose to
fix the blindness rather than accept it or drop the representation. Removed units and edges render
in dashed red alongside the live ones; the final prototype shows the five deleted `builtins_*`
units this way. **This is the one thing the editor's live graph structurally cannot do**, and it is
why the audit graph must be a distinct view over a distinct model rather than a mode on
`GraphView` — the live graph draws what exists, and half of an audit is what stopped existing.

**3. Findings ride on the changed rows, scoped to the diff.** `unpinned` in the prototype is
literally lint warning #2 from ticket 08; `over cap` and `hub` are two more. Rather than a parallel
findings list, each changed unit carries its own findings as tags, so the view answers *"what
changed, and what is wrong with what changed"* in a single pass. Findings on **untouched** units
are not this view's job — they belong to the standing surfaces (graph, content modal), which
already have somewhere to put them. The three severities keep their distinction in the tag styling;
a `candidate` must never render as an error, per 08.

**4. The range is an arbitrary git range, defaulting to `HEAD~1`.** Not just the last commit: an
authoring session is often several commits, and a branch is the natural review unit. This matches
`wcl diff`, which already takes `<rev>:<path>` specifiers. **Implementation constraint this
creates:** `crates/wcl_wskill` must be able to load a wskill's model *at a git revision*, which
means the gitspec plumbing that today lives in `crates/wcl` (`src/gitspec.rs`, materialising a rev
via `git archive | tar`) has to be reachable from the library — it is not today. Whoever specs
`wcl_wskill` needs this in scope from the start; it is not an editor concern.

**5. `wcl wskill audit [<rev>..<rev>]` is a sixth subcommand** — ticket 08's five become six.
This deliberately breaks 08's own precedent (nomination became a *severity* inside `lint` rather
than a sixth command) and the justification is that `audit` is categorically different: **every
other subcommand takes one graph; this one takes two.** A `--since` flag on `graph` and `lint` was
the alternative and was rejected — it would make every one of those commands' outputs
conditionally a diff. Emits the audit model as `--format json` for the editor adapter, or a
terminal report; the editor tab is a thin adapter over it, exactly as 08 specified for the rest.

**6. Plain search, everywhere.** No search exists anywhere in Design mode today — grepping the
whole `components/design/` tree returns one hit and it is a colour constant. One find-a-unit box
over id / name / summary / body that jumps the graph, index panel or canvas to the hit. The boring
answer, and the missing one. It is *not* a coverage probe (the "is this already covered before I
add" idea was offered and not chosen) and it does **not** replace fixing index coverage — those are
separate questions, and one of them is now fog rather than a ticket.

**7. The "curate this index" button is taken, and it closes the loop.** Ticket 07 left it open.
The editor gets a trigger for a curator pass, and **when that pass commits, the audit view opens on
that commit**. Run it → read what it did → `git revert` if bad. That sequencing is the point: the
button is only worth having because decision 1's surface exists to read its output. Without the
audit view a button would just be a headless job with a progress spinner. Scope is not restricted
to an index — whole-graph passes are triggerable too, consistent with 07's "on demand, never
automatic".

### What this does not settle

The ticket's fifth bullet — *what Design mode is missing for wskill work specifically, as opposed
to generic wdoc pages* — was not resolved. The session spent its fidelity on the audit surface, and
the remaining question is not sharp enough to state as a decision. Returned to the map's fog.

Also untouched: whether comment pins scale as the surface for machine-generated findings at volume
(07's narrowed question). Decision 3 routes *diff-scoped* findings to row tags, which sidesteps it
for the audit view but leaves it open for the standing surfaces.
