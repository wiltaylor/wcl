# What does each of the four backends actually need from a block?

Type: research
Status: resolved
Blocked by: —

## Question

Four backends consume the block pipeline, and the type system was designed around one of them.
Establish, as fact, what each one really needs — this constrains every answer in
`05-block-type-system` and `01-content-seam`.

Investigate in this repo (`crates/wcl_wdoc/src/`):

- **HTML** — `src/render/` (`html.rs`, `lower.rs`, `svg/`, `theme.rs`)
- **PDF** — `src/pdf/` (incl. `collect.rs`)
- **Markdown** — `src/markdown/` (incl. `emit.rs`, `skill.rs`)
- **Claude skill folder** — `src/markdown/skill.rs`, the `skill` block in `lib/templates.wcl`

For each backend, report:

1. **How it dispatches a block.** Does it walk lowered fundamentals, or match on block kind in Rust,
   or both? Where exactly does it diverge from the others?
2. **Which blocks it special-cases**, and which it cannot represent at all. Cross-check against the
   24 stub-`lower` blocks and the 5 `lower_svg` fns — is the Rust special-casing the *same set* per
   backend, or does each backend special-case a different subset?
3. **What it does with SVG.** `render_lowered_svg_block` (`src/render/svg/standalone.rs`) is the
   shared entry all three graphical backends dispatch to — establish whether that's the only shared
   seam or one of several.
4. **What it needs that `HtmlFundamental` can't express** — the reason PDF and Markdown have their
   own collect/emit passes rather than consuming lowered HTML.
5. **Where the degradation rules live.** `demo` degrades to source + one static render outside HTML;
   `edit_object` emits nothing outside edit mode; `markdown_source` is book-only. Is degradation a
   per-block decision scattered across backends, or is there a pattern?

Also report the **honest inventory**: for each of the 134 `@block` types, which backends can render
it. The answer to `05-block-type-system` needs to know whether "this block works everywhere" is the
common case or the rare one.

Capture findings on a throwaway `research/backend-survey` branch and link them from this ticket.
This is fact-gathering only — no design decisions.

## Context

Findings land at `spec/wdoc-substrate/research/04-backend-survey-findings.md` (dispatched as a background /research agent).

## Answer

Findings: `spec/wdoc-substrate/research/04-backend-survey-findings.md` (904 lines, every claim
cited `path:line`, all 134 `@block` rows present).

**The special-casing is NOT the same set per backend.** Each of the three code backends runs its own
Rust `match` on `block.kind()` and the sets differ:

- HTML only — `column`, `li` (top-level arm), `markdown_source`, `edit_object`
- Markdown only — `code`
- PDF only — none
- `callout` — PDF + Markdown, **not** HTML (`markdown/emit.rs:200`, `pdf/collect.rs:137`)
- `file` — HTML + Markdown, **absent from PDF entirely** (`render/html.rs:560`,
  `markdown/emit.rs:190`; no `pdf/` site) — a `file` block ships nothing and renders nothing in a PDF

`crates/wcl_wdoc/src/kinds.rs:1-8` asserts all three backends "special-case the same block
vocabulary". That comment is **factually wrong**; three of its own constants are referenced by a
strict subset.

**Root cause: recursive lowering exists only in HTML.** `lower_recurse` (`render/lower.rs:317`, called
at 387 and 472) has no PDF or Markdown counterpart. That single asymmetry explains the divergence —
`callout` has a real WCL `lower`, so HTML follows it and needs no special case, while PDF and Markdown
must hand-reimplement it in Rust. Consequence: **a user-declared block whose `lower` returns another
custom variant works in the book and silently renders nothing anywhere else.** The extension mechanism
is HTML-only in practice.

**Fundamental-variant coverage** against the 10 declared `HtmlFundamental` variants
(`lib/diagram-core.wcl:488`): HTML 10/10, Markdown 6/10, PDF 5/10. `Table`, `Head`, `Children`, `Icon`
are HTML-only.

**Two facts on the map were wrong** (my grep artifacts, corrected and re-verified):
- stub `lower`s: **57**, not 24 — 24 returning `HtmlFundamental` plus **33** returning
  `SvgFundamental`. The original count hardcoded `HtmlFundamental`.
- `lower_svg` fns: **2**, not 5 — `sequence.wcl:279`, `statechart.wcl:404`. The original count
  matched doc-comment mentions.

**Inventory shape.** Of 134 `@block`s: 34 page content, 40 diagram children, 13 terminal primitives,
12 parent-consumed children, 6 structural, 29 configuration. 29 of the 34 page-content blocks reach
all three backends — but *reaches ≠ renders equivalently*. `chapter_header` loses kicker/meta,
`footnotes` loses title and numbering, `demo` loses its source in PDF. Only `p`, `text`, `h1`–`h6`,
`math` survive intact everywhere through the fundamental layer alone.

**Q3** — `render_lowered_svg_block` is one of **four** shared SVG seams, not the only one (also
`render_diagram*`, `render_terminal*`, `render_math_fundamental`).

**Q5** — degradation has four unrelated mechanisms and no central one. The declarative `@only`/`@except`
system does have a `backends` axis (`:html`/`:pdf`/`:markdown`), but there is **no `:skill` symbol**
(skill runs as `Backend::Markdown`), and the axis expresses instance-scoped *author intent*, not
kind-scoped *capability*. A vanished block emits no warning.

**Skill backend** has no block dispatch of its own — it is the Markdown emitter plus
layout/front-matter/link context, so it special-cases exactly Markdown's set. It diverges in
`include` fan-out, which PDF and plain Markdown ignore silently.

**Latent defect found, not part of this ticket:** a diagram containing a `node_table` loses all row
text in PDF — `card_rects` counts every `foreignObject` while `collect_card_blocks` counts only `card`
blocks, so the counts disagree and every box is emptied. Filed as a note on the map; it's a bug, not a
decision.

**Verified independently** (not taken on the agent's word): the two corrected counts, `lower_recurse`
being HTML-only, the `kinds.rs` comment text, and the `callout` / `file` dispatch sites. The remaining
per-backend set claims rest on the agent's survey.

**Open gaps the agent flagged honestly:** whether `file`-in-PDF is deliberate (a question of intent);
whether `foreignObject` survives an image-referenced `.svg` (not a fact about this repo).

Status: resolved
