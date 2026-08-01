# What does the editor need so you can audit AI output and navigate a big wskill?

Type: prototype
Status: open
Blocked by: 08

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
