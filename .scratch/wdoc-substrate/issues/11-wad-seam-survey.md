# How does WAD use the current template and block seam?

Type: research
Status: resolved
Blocked by: —

## Question

WAD is one of the two consumers that must prove the new substrate, and it's the heavier one. Before
anything is designed, establish what it currently does with the seam — as fact.

Investigate `.wad/` and `docs/wskills/wad/` in this repo, plus
`crates/wcl/src/scaffold/templates/wad.wcl` (the canonical schema heredoc) and
`crates/wcl/src/editor/systems.rs`.

Report:

1. **The template inventory.** `.wad/wdoc/book/` — how many template pages, what shapes recur, and
   which of them are doing work the current template model makes awkward. The Systems-view work notes
   a full WAD book eval is ~160 template pages and 63s in debug; establish where that cost sits.
2. **What WAD asks of `TemplateCtx`.** Which fields it reads, whether it uses `region`s at all, and
   where it works around the content-as-`utf8` seam.
3. **Schema-derived rendering.** WAD's Systems view derives its entire model from schema
   introspection — parent links from field names, edge kinds from `source`+`destination`, references
   from `identifier` fields. Report precisely which language/schema features that depends on
   (`kind_links`, `TypeDecl::effective_fields`, `resolved_type`, `Document::resolve_in`), because the
   type-system refactor could break it.
4. **The extractor boundary.** `.wad/scripts/` own `data/generated/`. What contract exists between
   extractor output and the templates, and is it affected by any of this?
5. **Where WAD hurts today**, from the code — repeated template boilerplate, things expressed as
   prose that should be data, per-view duplication, anything that reads as fighting the tooling.

Note the scope line: this map treats WAD as a **consumer that validates the substrate**, not as a
subject to redesign. Findings that are about WAD's own content model rather than its use of the
substrate should be flagged as such — they belong in the map's *Not yet specified* or a later effort.

Capture findings on a throwaway `research/wad-seam` branch and link them from this ticket.
Fact-gathering only — no design decisions.

## Context

Findings land at `.scratch/wdoc-substrate/research/11-wad-seam-findings.md` (dispatched as a background /research agent).

## Answer

Findings: `.scratch/wdoc-substrate/research/11-wad-seam-findings.md` (728 lines, `path:line` cited).

**WAD does not use the `TemplateCtx` / region seam at all.** Verified independently: zero hits for
`TemplateCtx`, `wdoc_region`, `region`, `template` or `HtmlFundamental` anywhere under `.wad/` or
`docs/wskills/wad/`. Its entire template coupling is **one line** — `default_template = :book`
(`.wad/wdoc/book/main.wcl:183`) — plus the `toc { chapter … page = <name> }` string contract. The
`Raw { html: c.content }` splice is done *for* it by the stdlib at `lib/templates.wcl:838`.

**So WAD validates the layer BELOW the template**, not the slot contract: repeaters, `project`,
`wdoc_component`/`wdoc_slot`, string page identity, gathers, schema introspection, `edit_object`. This
corrects the map's charting premise — see the Notes section there.

**Q1 — cost measured, not inherited.** 32 authored `page` blocks expand to **161 rendered pages**.
Debug at `ee90aa5a`: `check` on the model 1.92s, `check` on the book entry 2.22s, markdown build 25.2s,
full HTML build **55.4s**. Parse+validate is **4%** — page count is not the driver. Of 46.0s accounted
eval, two near-equal halves:
- **20.22s (36% of the whole build) inside `wdoc_book_layout`** — the built-in book template, called
  once per page and recursively re-walking the entire toc each time: `book_pageflow` **26082 calls**,
  `book_toc` 26082, `toc_active` 54115, for 161 pages. *A template-layer fact that argues for the
  refactor on performance grounds alone, independent of ergonomics.*
- 20.55s in repeater `each` fields, 17.35s of it two WAD helpers (`code_items_of_comp` 376ms/call ×34,
  `code_items_of` 381ms/call ×12) filtering a 74-instance gather over 9370 generated lines. The
  `filter` builtin is only 11.5ms; the remaining self time is NOT DETERMINED without a native profiler.

**Q3 (the priority) — pinned.** `kind_links` (`blocks.rs:1526-1579`) + `gathered_kinds`
(`:1442-1467`) + `gather_elem_decl` (`:1474-1485`). Full feature→call-site table in the findings, covering
`effective_fields` (7 sites), `resolved_type`→`resolve_in`, `type_decls`, `block_schema`, `inline_slot`,
`children_block_kind`. Note `is_descendant_of` appears **only** at `blocks.rs:1615`, gating
`diagram_kinds` — **not** the C4 model. **Nine load-bearing assumptions enumerated, and the fragile
ones are printed-string comparisons**: `bare_type(f) == "identifier"` / `== "list<identifier>"`
(`blocks.rs:1542,1558`), `to_string().starts_with("fn")`, `full_name().starts_with("wdoc.")`,
`is_descendant_of("wdoc.SvgBlock")`. Several fail **silently** — a reclassified field just stops being a
parent link, with no error. This is the concrete breakage risk a type-system refactor carries.

**Q4 — the extractor boundary is refactor-neutral.** Extractors emit only data blocks against
`@block`/`@children`; nothing under `.wad/scripts/` or `data/generated/` mentions a template construct.
But 10104 committed lines (87% of all WAD data) plus the byte-identity CI gate (`justfile:311-318`) make
any schema-shape change a synchronised six-emitter change. One extra coupling: `is_generated`
(`blocks.rs:1098-1114`) **parses** the `GENERATED` banner — the convention is machine-read, not just
documentation.

**Q5 — 140 of 252 repeaters (56%) are `if` statements in disguise** with discard loop variables. WAD is
now the only in-repo consumer of `edit_object`.

**Verified independently:** the absent template seam (zero-hit grep), the single `default_template`
coupling, and the missing `wad-template-check` recipe. The perf profile, call-site table and drift diff
rest on the agent's survey.

**Six residual gaps** marked NOT DETERMINED in the findings, each with what it would need.

Status: resolved
