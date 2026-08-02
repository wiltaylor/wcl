# The wdoc substrate — specification

The deliverable of the [wdoc substrate refactor map](map.md). Fifteen decision tickets, resolved
one per session, consolidated into a buildable spec.

**This document invents nothing.** Every decision here traces to a resolved ticket, and the ticket
carries the argument, the rejected alternatives and the measurements. Where the route left something
undecided it is marked **OPEN** and says who owns it — those are not gaps to fill by inference.

## What is being built

Two halves of one substrate, plus the consumers that prove it:

- **An honest block/type system.** Today 57 `@block` types declare a `lower` that Rust intercepts and
  never calls; the three code backends each run a *different* Rust `match` on block kind; a heading
  smuggles its level through a CSS class; and a user-declared block silently renders nothing outside
  the book. The declared type system and the real one have diverged.
- **A typed templating layer.** Today a template receives its page as opaque pre-rendered HTML
  (`TemplateCtx.content: utf8`), named regions are an unchecked flat string namespace
  (`wdoc_region(c, "heor")` returns `""`, silently, forever), and a layout cannot declare what it
  needs.
- **The in-repo consumers migrate onto it as proof**: the docs site, `examples/`, the four wskills
  and WAD. Nothing gets a compatibility shim — a shim would preserve the exact seam this exists to
  kill.

## The parts

| | | |
|---|---|---|
| [01](01-language.md) | **Language** — `wcl_lang` | The generic facilities wdoc's extraction needs, syntax-only generics, the expander callback, typed schema introspection, else-less `if` |
| [02](02-blocks.md) | **Blocks** — the type system | The content IR / HTML vocabulary split, `@native`, one interface per output IR, exhaustive matching, the constructor DSL |
| [03](03-templates.md) | **Templates** — the content seam and slots | Block handles, the element DSL, the `slot` contract, template selection, collection templates |
| [04](04-css.md) | **CSS** — authoring | Typed selectors with raw declarations; every heredoc dies |
| [05](05-wskill.md) | **wskill** — format, CLI and curator | `crates/wcl_wskill`, mandatory edge reasons, the six subcommands, the curator contract, the authoring processes |
| [06](06-editor.md) | **Editor** — what breaks and what is added | The introspection breakage, slot editing surfaces, the audit view, search |
| [07](07-migration.md) | **Migration** — eight sweeps | What moves, in what order, and where the sweeps contend for the same files |
| [08](08-open.md) | **Open** — deferred, out of scope, unresolved | Everything the route deliberately did not settle |

## Dependency order

```
                    01 Language ─────────────┐
                        │                    │
              ┌─────────┴─────────┐          │
              ▼                   ▼          ▼
        02 Blocks ─────────► 03 Templates   06 Editor
              │                   │          ▲
              ▼                   │          │
           04 CSS                 │          │
                                  ▼          │
                            05 wskill ───────┘
                                  │
                                  ▼
                            07 Migration
```

Hard constraints on that order, each from a specific ticket:

- **01's `FieldShape` introspection cannot land after 02 and 03's renames.** `WdocBlock` becomes
  `ContentBlock`, `wdoc_slot` becomes `slot`, `Region` and `wdoc_content` die — and the editor's WAD
  Systems view reads those names through printed-string comparisons that fail *silently*
  (`blocks.rs:1542,1558`). (11, 14 decision 9)
- **01's expander error must land with 01's implementation, not after.** A missing expander is a hard
  error on demand, so `wcl get` / `eval` / the LSP / the editor's open paths must supply the wdoc
  Environment in the same change. Between the two, those commands fail loudly on every wdoc document.
  (14 decision 5)
- **04 follows 02.** The CSS vocabulary is four new `@block` types; it wants the settled type system
  underneath it. (13, deliberately-not-decided)
- **05's `why` flip must run before the schema tightens** — a bare edge will not parse afterwards.
  (08 decision 7)

## The non-negotiables

Invariants that survive every decision. A design that breaks one of these is wrong regardless of its
other merits.

1. **Slot fills are bare names, layout-agnostic.** One wskill unit body projects into four template
   sets (book, skill, training, deck) via `project { from = <unit>.body }`. Binding a fill to a named
   layout breaks the wskill model outright. Wil: *"make sure we can do that projection or wskill
   breaks."* (03 decision 6)
2. **`wcl check` keeps catching the component slot error.** `field 'labell' is not a slot of component
   'metric_card'` works today and must not regress. (14, stated constraint)
3. **Design mode's click-to-edit survives.** Rendered output must keep tracing back to source spans
   (`data-wcl-span` / `data-wcl-file`, stamped by `anchor_block` under `InlinePatterns::edit_mode()`).
   (01, stated constraint)
4. **PDF and Markdown keep rendering.** They are constraints on the type system, not subjects of it.
   (map, Out of scope)
5. **No compatibility shims.** Everything in-repo migrates.

## Cutting tickets from this

Each part is written so its numbered decisions map onto work. Guidance:

- **A decision is not a ticket.** Several decisions are one line (`:skill` added to the `backends`
  symbol set); several are a crate (`crates/wcl_wskill`). Group by file contention, not by decision
  count — part 07 records where the sweeps collide.
- **The measurements are load-bearing.** Where a decision cites a count (57 stub lowers, 258
  construction sites, 477 CSS rules, 736 bare edges), that number was verified and it sizes the work.
  Re-verify before quoting it in a brief; several map facts were corrected mid-route.
- **Read the ticket before re-opening a decision.** Every one records what was rejected and why. Four
  decisions were taken by Wil *against* the session's recommendation (03 decision 5 and 7, 12
  decisions 5–9, 14 decision 3, 15's first scope call) — those especially.
- **Three prototypes are executable** and are the evidence for their parts:
  [`proto-02-template-authoring/`](proto-02-template-authoring/),
  [`proto-13-css-authoring/`](proto-13-css-authoring/),
  [`proto-15-constructor-dsl/`](proto-15-constructor-dsl/). Run them before disputing what they
  found.

## Verification

The repo's standing bar (`CLAUDE.md`) applies to every slice:

```bash
just workspace-test
just workspace-lint
cargo fmt --all -- --check
```

Plus, for this effort specifically: **the docs site, `examples/`, the four wskills and WAD all build**
in HTML, Markdown, skill and PDF. That set *is* the proof the substrate works, and it is why the
migration is part of the deliverable rather than follow-up work.
