# 04 — Backend survey: what each backend actually needs from a block

Fact-gathering for ticket `.scratch/wdoc-substrate/issues/04-backend-survey.md`.
Every claim below is cited as `path:line` against the source in this repo at the time of
writing (branch `main`, HEAD `ee90aa5a`). Paths are relative to `/home/wil/orca/wcl/`
unless stated. Where a fact could not be established it is marked **NOT DETERMINED**
with what would need checking.

Design implications are quarantined in the final section.

---

## 0. Corrections to the ticket's premises

Two premises in the ticket do not match the source and should be fixed before the
findings are used:

1. **"the 24 stub-`lower` blocks"** — there are **24 stubs returning
   `list<HtmlFundamental> []`** *and a further 33 returning `list<SvgFundamental> []`*,
   i.e. **57 stub `lower`s in total**. Counts from
   `grep -c 'lower = fn(.*) -> list<HtmlFundamental> \[\]$' crates/wcl_wdoc/lib/*.wcl`
   (24) and the `SvgFundamental` equivalent (33). Full lists in §2.1/§2.2.

2. **"the 5 `lower_svg` fns"** — there are **2**:
   `crates/wcl_wdoc/lib/sequence.wcl:279` (`SequenceDiagram.lower_svg`) and
   `crates/wcl_wdoc/lib/statechart.wcl:404` (`StateDiagram.lower_svg`). A repo-wide
   `grep -rn 'lower_svg' --include='*.wcl' --include='*.rs'` finds no others; the only
   Rust reader is `crates/wcl_wdoc/src/render/svg/standalone.rs:26`
   (`lower_to_values_named(doc, block, kind, "lower_svg")`), and the only two kinds
   routed to it are `sequence_diagram` / `state_diagram`
   (`crates/wcl_wdoc/src/render/html.rs:568`, `crates/wcl_wdoc/src/pdf/collect.rs:97`,
   `crates/wcl_wdoc/src/markdown/emit.rs:171`).

3. `grep -c '^@block(' crates/wcl_wdoc/lib/*.wcl` totals **134** — the ticket's number is
   correct.

---

## 1. How each backend dispatches a block

### 1.1 The one genuinely shared seam: `walk_structural`

All three code backends (HTML, PDF, Markdown) begin their per-block dispatch by
delegating to `crate::render::walk_structural`
(`crates/wcl_wdoc/src/render/expand.rs:277-357`):

- HTML — `crates/wcl_wdoc/src/render/html.rs:523`
- PDF — `crates/wcl_wdoc/src/pdf/collect.rs:54`
- Markdown — `crates/wcl_wdoc/src/markdown/emit.rs:134`

`walk_structural` owns, identically for all three
(`crates/wcl_wdoc/src/render/expand.rs:283-355`):

| behaviour | line |
|---|---|
| `@only`/`@except` visibility filtering (`visibility::block_visible`) → renders nothing | `expand.rs:283` |
| `notes` \| `frontmatter` → nothing | `expand.rs:287` |
| `partial` → children only when `show_here = true` | `expand.rs:288-297` |
| `collect` → gather matching `partial` bodies, cycle-guarded | `expand.rs:298-309` |
| `body` → nothing (reached only via `project`) | `expand.rs:314` |
| `project` → resolve `from` to a `body` and recurse its children, cycle-guarded | `expand.rs:315-353` |

It returns `Some(..)` when it handled the block and `None` for "run your own dispatch".
Its doc comment explicitly notes `fragment` is *deliberately not* handled here because
HTML wraps fragment children in a step-reveal `<div>` while the static backends treat it
as transparent (`crates/wcl_wdoc/src/render/expand.rs:274-276`).

### 1.2 After that: three separate `match`es on `block.kind()`

Every backend then runs **its own** Rust `match`/`if`-chain on `block.kind()`, and only
falls through to the WCL `lower` for kinds it does not name:

- **HTML** — `render_block`, `crates/wcl_wdoc/src/render/html.rs:512-628`; fall-through at
  `html.rs:619-625` → `doc.component_def(kind)` → `render_component`, else
  `lower_html_block`.
- **PDF** — `collect_block`, `crates/wcl_wdoc/src/pdf/collect.rs:44-199`; fall-through at
  `collect.rs:189-198` → `component_def` → `collect_component`, else `lower_to_values` +
  `walk_block_variant`.
- **Markdown** — `Emitter::block`, `crates/wcl_wdoc/src/markdown/emit.rs:128-247`;
  fall-through at `emit.rs:234-244` → `component_def` → `self.component`, else
  `lower_to_values` + `self.fundamental`.

So the answer to "walk lowered fundamentals vs match on kind in Rust" is **both, in all
three** — a Rust kind match in front, a fundamental walk behind. The *fundamental walk*
itself is also three separate implementations over the same value shape:

| fundamental variant | HTML | PDF | Markdown |
|---|---|---|---|
| dispatcher | `render_html_variant`, `crates/wcl_wdoc/src/render/lower.rs:423-476` | `walk_block_variant`, `crates/wcl_wdoc/src/pdf/collect.rs:228-316` | `Emitter::fundamental`, `crates/wcl_wdoc/src/markdown/emit.rs:274-335` |
| `paragraph` | `lower.rs:450` | `collect.rs:238` | `emit.rs:279` |
| `element` | `lower.rs:452` | `collect.rs:257` | `emit.rs:292` |
| `inline` | `lower.rs:465` | `collect.rs:286` | `emit.rs:313` |
| `highlighted` | `lower.rs:468` | `collect.rs:300` | `emit.rs:322` |
| `math` | `lower.rs:471` | `collect.rs:295` | `emit.rs:316` |
| `raw` | `lower.rs:454` | **absent** | `emit.rs:328` |
| `table` | `lower.rs:451` | **absent** | **absent** |
| `head` | `lower.rs:457` (+ hoisting via `head_fundamental_html`, `lower.rs:396`) | **absent** | **absent** |
| `icon` | `lower.rs:461` | **absent** | **absent** |
| `children` (slot sentinel) | `lower.rs:443` | **absent** | **absent** |
| custom variant → recurse into its type's `lower` | `lower.rs:472` (`lower_recurse`, `lower.rs:317`) | **absent** (falls to `_ => {}` at `collect.rs:314`) | **absent** (falls to `_ => {}` at `emit.rs:333`) |

**This is a hard divergence, not a nuance.** The recursive lowering the CLAUDE.md
architecture summary describes ("the renderer recurses until only fundamentals remain")
exists **only in the HTML backend**. In PDF and Markdown, a `lower` that returns a
non-fundamental custom variant is silently dropped (`collect.rs:314`, `emit.rs:333`).
The same is true of `HtmlFundamental::Table`, `Icon`, `Head` and `Children`: HTML
implements them, the other two do not.

Counted against the **declared** union (`union HtmlFundamental`,
`crates/wcl_wdoc/lib/diagram-core.wcl:488-538`, which declares exactly 10 variants —
`Paragraph`, `Table`, `Element`, `Raw`, `Head`, `Children`, `Icon`, `Inline`,
`Highlighted`, `Math`):

| backend | declared variants handled | missing |
|---|---|---|
| HTML | **10 / 10** | — |
| Markdown | **6 / 10** | `Table`, `Head`, `Children`, `Icon` |
| PDF | **5 / 10** | `Table`, `Raw`, `Head`, `Children`, `Icon` |

For contrast, the other two fundamental unions are **fully** handled by their single
shared renderer: `union SvgFundamental`
(`crates/wcl_wdoc/lib/diagram-core.wcl:478-486`, 7 variants — `Rect`, `Circle`, `Line`,
`Label`, `Polygon`, `Polyline`, `Link`) is handled 7/7 by `render_svg_variant`
(`crates/wcl_wdoc/src/render/lower.rs:358-390`) and 7/7 by the bbox fitter
(`crates/wcl_wdoc/src/render/svg/standalone.rs:110-164`); `union TermFundamental`
(`crates/wcl_wdoc/lib/tui.wcl:38-45`, 2 variants — `Text`, `Children`) is handled by
`draw_variant` (`crates/wcl_wdoc/src/terminal/widgets.rs:80`).

### 1.3 The skill-folder backend has no block dispatch of its own

`crates/wcl_wdoc/src/markdown/skill.rs` contains **no `block.kind()` content dispatch**.
It calls the Markdown emitter directly:

- `emit::emit_page_with_front_matter` for the start page → `SKILL.md`
  (`skill.rs:433-444`)
- `emit::emit_page` for every other page → `references/<name>.md` (`skill.rs:448-451`)
- `emit::body_to_markdown` for an `agent` block's `body` → `agents/<name>.md`
  (`skill.rs:320-331`)

Its differences from plain Markdown are **layout and context**, not block semantics:

| difference | line |
|---|---|
| requires `default_template = :ai_skill` on the site, else hard error | `skill.rs:364-369` |
| requires a `skill { name = … description = … }` child block | `skill.rs:370-377` |
| requires a `start = true` page (becomes `SKILL.md`) | `skill.rs:378-382` |
| start page's front matter comes from `yaml::skill_front_matter` instead of the page's own `frontmatter` | `skill.rs:432` |
| `patterns.set_skill_layout(start)` + per-page `set_skill_current_reference` (drives `../` asset/link prefixes) | `skill.rs:419`, `skill.rs:430` |
| document-level `agent` blocks written to `agents/<name>.md` — **skill target only** | `skill.rs:268-342` |
| `include` fan-out recursion builds member skills | `skill.rs:217-237` |
| inline engine mode is `Backend::Markdown` (same as Markdown) | `skill.rs:414`, `skill.rs:303` |

So for question 2's purposes, **"skill" special-cases exactly the set Markdown does.**
The only kinds it treats specially at all are `site`, `skill`, `page`, `file` (via the
registry copy, `skill.rs:469`) and `agent` — all of which are structure, not page content.

The `skill` **block** itself (`crates/wcl_wdoc/lib/templates.wcl:292`) is a
configuration child of `site`, not a `WdocBlock`: it declares `name`, `description`
and friends read by `yaml::skill_front_matter`
(`crates/wcl_wdoc/src/markdown/yaml.rs:55-100`), which emits exactly three canonical keys
from the `skill` block — `name`, `description`, `license`
(`yaml.rs:59`) — then merges any *non-canonical* keys from the start page's own
`frontmatter` block (e.g. `allowed-tools`), with the `skill` block authoritative for the
canonical three (`yaml.rs:77-99`). This is the **only** place a `frontmatter` block feeds
anything other than a plain Markdown page header. **Also worth noting**: the `frontmatter`
block is inert on the HTML and PDF paths (`crates/wcl_wdoc/src/render/expand.rs:287`), so
front matter is a Markdown/skill-only concept — a fifth kind of per-backend divergence
(a block whose *entire purpose* is one backend).

---

## 2. Which blocks each backend special-cases — the divergence table

**This is the headline finding: the Rust special-casing is NOT the same set per backend.**
Each backend names an overlapping but distinct subset, and a kind a backend does *not*
name falls through to its WCL `lower` — which for the 57 stub `lower`s means **it renders
nothing at all**.

### 2.1 Side-by-side dispatch table

Legend: **R** = special-cased in that backend's Rust dispatch; **L** = falls through to
the WCL `lower`; **∅** = falls through to a *stub* `lower` and therefore emits nothing.

| kind | HTML | PDF | Markdown / skill | notes |
|---|---|---|---|---|
| `column` | **R** `html.rs:533` | **∅** (no arm; stub `lower` at `lib/text.wcl:60`) | **∅** (no arm) | HTML-only. A `column` in a PDF/Markdown page silently vanishes. |
| `region` | **R** → `""` `html.rs:538` | **R** → children in place `collect.rs:72` | **R** → children in place `emit.rs:150` | same kind, three different meanings |
| `fragment` | **R** → `<div class="wdoc-fragment">` `html.rs:543` | **R** → children in place `collect.rs:64` | **R** → children in place `emit.rs:143` | deliberately excluded from `walk_structural` (`expand.rs:274`) |
| `edit_field` | **R** → children + edit-mode attrs `html.rs:600` | **R** → children `collect.rs:80` | **R** → children `emit.rs:157` | |
| `edit_object` | **R**, edit-mode-gated `html.rs:591-595` | **∅** (no arm; stub `lower` `lib/edit_object.wcl:35`) | **∅** (no arm) | |
| `table` | **R** `html.rs:544` | **R** `collect.rs:111` | **R** `emit.rs:183` | |
| `list` | **R** `html.rs:550` | **R** `collect.rs:104` | **R** `emit.rs:182` | |
| `li` | **R** own arm `html.rs:551` (+ anchored at `html.rs:991`) | inside `collect_li_group` only `collect.rs:579` | inside `li_group` only `emit.rs:387` | HTML alone can render a bare `li` |
| `code` | **L** → `Highlighted` fundamental (`lib/code.wcl`) | **L** → `highlighted` re-highlighted `collect.rs:300` | **R** `emit.rs:184` (reads `source` field directly) | Markdown bypasses the lowering entirely |
| `callout` | **L** → WCL `lower` (`lib/callout.wcl`) | **R** `collect.rs:137-151` | **R** `emit.rs:200` | HTML is the odd one out here |
| `image` | **R** `html.rs:555` | **R** `collect.rs:126` (reads file bytes) | **R** `emit.rs:185` | |
| `file` | **R** `html.rs:560` | **∅** — *no `kinds::FILE` arm in `pdf/collect.rs` at all* | **R** `emit.rs:190` | see §2.4 |
| `video` | **R** `html.rs:564` | **R** poster + link `collect.rs:133` | **R** link `emit.rs:195` | |
| `diagram` | **R** `render_diagram` `html.rs:565` | **R** `render_diagram` (**non-static**) `collect.rs:91` | **R** `render_diagram_static` `emit.rs:165` | see §3.2 |
| `sequence_diagram` | **R** `html.rs:568` | **R** `collect.rs:97` | **R** `emit.rs:171` | via `render_lowered_svg_block` |
| `state_diagram` | **R** `html.rs:568` | **R** `collect.rs:97` | **R** `emit.rs:171` (literal `"state_diagram"`, not `kinds::`) | |
| `terminal` | **R** `render_terminal` `html.rs:575` | **R** `render_terminal_pdf` `collect.rs:118` | **R** `render_terminal_pdf` `emit.rs:178` | |
| `markdown_source` | **R** `html.rs:581` | **∅** (no arm; stub `lower` `lib/markdown_source.wcl:31`) | **∅** (no arm) | book-only by construction |
| `demo` | **R** dual light/dark preview `html.rs:585` | **R** children only, no source `collect.rs:181` | **R** fenced source + children `emit.rs:204` | three different degradations |
| `wdoc_repeater` | **R** `html.rs:604` | **R** `collect.rs:157` | **R** `emit.rs:215` | |
| `wdoc_instance` | **R** `html.rs:607` | **R** `collect.rs:165` | **R** `emit.rs:224` | |
| `wdoc_content` | **R** → sentinel `WF_CONTENT_SLOT` `html.rs:611` | **R** → nothing `collect.rs:175` | **R** → nothing `emit.rs:233` | HTML uses a string sentinel + substitution (`lower.rs:127`, `html.rs:820`); the other two substitute structurally inside `collect_component`/`component` (`collect.rs:217`, `emit.rs:262`) |
| user `wdoc_component` instance | **R** `html.rs:620` | **R** `collect.rs:189` | **R** `emit.rs:237` | |
| everything else | **L** `lower_html_block` `html.rs:623` | **L** `lower_to_values` `collect.rs:193` | **L** `lower_to_values` `emit.rs:239` | |

### 2.2 Exact deltas (the answer to question 2)

Kinds special-cased in Rust by **exactly one** backend:

- **HTML only** — `column` (`html.rs:533`), `li` as a top-level arm (`html.rs:551`),
  `markdown_source` (`html.rs:581`), `edit_object` (`html.rs:591`).
- **Markdown only** — `code` (`emit.rs:184`).
- **PDF only** — none.

Kinds special-cased by exactly **two** backends:

- `callout` — PDF (`collect.rs:137`) + Markdown (`emit.rs:200`), **not** HTML.
- `file` — HTML (`html.rs:560`) + Markdown (`emit.rs:190`), **not** PDF.

Kinds special-cased by all three but with **different semantics**: `region`, `fragment`,
`demo`, `wdoc_content`, `diagram`, `video`, `image`, `terminal`.

The shared vocabulary is centralised in `crates/wcl_wdoc/src/kinds.rs` whose module doc
claims "The HTML, Markdown and PDF backends each special-case the same block vocabulary"
(`crates/wcl_wdoc/src/kinds.rs:3-8`). **That comment is inaccurate**: `kinds::CODE`
(`kinds.rs:20`) is referenced only by `markdown/emit.rs:184`, `kinds::FILE`
(`kinds.rs:22`) only by `html.rs:560` and `emit.rs:190`, and `kinds::CALLOUT`
(`kinds.rs:24`) only by `collect.rs:137` and `emit.rs:200`. Verified by
`grep -n 'kinds::[A-Z_]*' src/render/html.rs src/markdown/emit.rs src/pdf/collect.rs`.

### 2.3 The 24 `HtmlFundamental` stub `lower`s, cross-checked

A stub `lower = fn(x: T) -> list<HtmlFundamental> []` means: **this block renders nothing
unless the backend names it in Rust.** Cross-referencing the 24 stubs against §2.1:

| stub block | declared at | HTML | PDF | Markdown/skill |
|---|---|---|---|---|
| `sequence_diagram` | `lib/sequence.wcl:277` | R | R | R |
| `state_diagram` | `lib/statechart.wcl:402` | R | R | R |
| `diagram` | `lib/diagram-core.wcl:92` | R | R | R |
| `terminal` | `lib/terminal.wcl:74` | R | R | R |
| `table` | `lib/table.wcl:48` | R | R | R |
| `list` | `lib/list.wcl:51` | R | R | R |
| `image` | `lib/image.wcl:45` | R | R | R |
| `video` | `lib/video.wcl:39` | R | R | R |
| `demo` | `lib/demo.wcl:42` | R | R | R |
| `wdoc_content` | `lib/components.wcl:67` | R | R | R |
| `wdoc_repeater` | `lib/components.wcl:97` | R | R | R |
| `wdoc_instance` | `lib/components.wcl:122` | R | R | R |
| `region` | `lib/website.wcl:38` | R | R | R |
| `fragment` | `lib/presentation.wcl:83` | R | R | R |
| `edit_field` | `lib/edit_field.wcl:39` | R | R | R |
| `file` | `lib/file.wcl:35` | R | **∅ nothing** | R |
| `column` | `lib/text.wcl:60` | R | **∅ nothing** | **∅ nothing** |
| `markdown_source` | `lib/markdown_source.wcl:31` | R | **∅ nothing** | **∅ nothing** |
| `edit_object` | `lib/edit_object.wcl:35` | R (edit mode only) | **∅ nothing** | **∅ nothing** |
| `notes` | `lib/presentation.wcl:95` | ∅ by design (`walk_structural`, `expand.rs:287`) | ∅ by design | ∅ by design |
| `partial` | `lib/components.wcl:148` | handled by `walk_structural` (`expand.rs:288`) | same | same |
| `collect` | `lib/components.wcl:154` | handled by `walk_structural` (`expand.rs:298`) | same | same |
| `project` | `lib/project.wcl:72` | handled by `walk_structural` (`expand.rs:315`) | same | same |
| `footnote` | `lib/footnotes.wcl:21` | see §2.5 | **∅ nothing** | **∅ nothing** |

So of the 24 HTML-fundamental stubs: **15 are Rust-implemented in all three backends**,
**4 are handled by the shared structural walker**, **1 (`file`) is missing from PDF**,
**3 (`column`, `markdown_source`, `edit_object`) exist only in HTML**, and **1
(`footnote`) needs the §2.5 check**.

### 2.4 `file` in PDF — established as absent

`grep -n 'kinds::FILE\|crate::file' crates/wcl_wdoc/src/pdf/*.rs` returns nothing (see
§2.2's grep). `pdf/collect.rs` has no `file` arm, so a `file` block falls to
`collect.rs:193` → `lower_to_values` → the stub at `lib/file.wcl:35` → empty list → no IR
node. Consequence: in a PDF build a `file` block neither ships its asset nor renders its
download link.

**NOT DETERMINED**: whether this is deliberate (a PDF is a single distributable file, so a
sidecar asset has nowhere to live) or an oversight. There is no comment either way at
`crates/wcl_wdoc/src/pdf/collect.rs:189-198`, and `lib/file.wcl` carries no
backend-support note (**check**: `crates/wcl_wdoc/lib/file.wcl:22-36` and the docs page
`docs/pages/reference/wdoc/skills.wcl`).

### 2.5 `footnote` / `footnotes` — HTML-side post-processing, not a block dispatch

`footnote` has a stub `lower` (`crates/wcl_wdoc/lib/footnotes.wcl:21`) and there is no
`footnote` arm in any backend's dispatch. HTML runs a separate post-pass over the
already-rendered page string: `super::headings::process_footnotes(&content)` at
`crates/wcl_wdoc/src/render/html.rs:433`, called from `render_template`. Since that call
lives in `render_template`, footnote linking happens **only for templated HTML pages**.

Established since: `process_footnotes` (`crates/wcl_wdoc/src/render/headings.rs:152-173`)
is a **string substitution over the rendered HTML** — it scans for `[^id]` definitions,
numbers them in first-appearance order, and replaces each `[^id]` with
`<sup class="footnote-ref" id="fnref-id"><a href="#fn-id">N</a></sup>`. The `footnotes`
block's own `lower` (`crates/wcl_wdoc/lib/footnotes.wcl:28-58`) emits the definition list
(`Element{section}` → title `Raw` + `Element{ol}` of `Element{li}` with `Inline` + a `↩`
back-link `Raw`).

And an **untemplated** HTML page gets neither footnote linking nor heading anchors: the
no-template branch of `build_normal_page` returns the content verbatim
(`None => crate::render::Rendered { body: content.clone(), head: String::new() }`,
`crates/wcl_wdoc/src/build.rs:2144-2147`), bypassing both
`process_page_headings` and `process_footnotes` (`crates/wcl_wdoc/src/render/html.rs:432-433`).
So footnote *references* work on templated HTML pages only, and footnote *definitions*
degrade to loose paragraphs in PDF/Markdown (§6.3).

### 2.6 The 33 `SvgFundamental` stub `lower`s — diagram-shape special-casing

These are diagram children, dispatched by `render_shape`
(`crates/wcl_wdoc/src/render/svg/shapes.rs:417-502`), which is reached from
`render_layout_children` (`crates/wcl_wdoc/src/render/svg/diagram.rs:171-190`) — i.e.
**inside `render_diagram` only**. There is exactly **one** shape dispatcher; PDF and
Markdown reach it through the same `render_diagram*` call, so shape special-casing is
NOT per-backend (see §3).

Stub-`lower` SVG blocks, by file
(`grep -c 'lower = fn(.*) -> list<SvgFundamental> \[\]$' lib/*.wcl`):
`lib/wireframe.wcl` 16, `lib/diagram-core.wcl` 7, `lib/tree.wcl` 2,
`lib/node_table.wcl` 2, `lib/card.wcl` 1, `lib/dopesheet.wcl` 1, `lib/icons.wcl` 1,
`lib/map.wcl` 1, `lib/tilemap.wcl` 1, `lib/timeline.wcl` 1.

`render_shape`'s Rust arms (`crates/wcl_wdoc/src/render/svg/shapes.rs:424-499`):
`rect` (:425), `circle` (:426), `line` (:427), `label` (:428), `polygon` (:429),
`container` (:430), `boundary` → `None` (:434), `icon` (:436), `tilemap` (:439),
`dopesheet` (:450), `node_table` (:458), `tree` (:467), `image` (:472), `map` (:476),
`card` (:480), `timeline` (:483), the whole `wf_*` family via
`wireframe::is_wireframe_kind` (:493), and the fall-through
`lower_svg_block` (:498) for everything else (`process` / `decision` / `terminator` /
`node` / charts / user shapes).

The seven `diagram-core.wcl` SVG stubs are `container` (:228), `boundary` (:262),
`rect` (:307), `circle` (:350), `line` (:384), `label` (:428), `polygon` (:458) — i.e.
the five SVG *fundamentals* plus `container`/`boundary`; they are stubs because the Rust
side owns them directly (`shapes.rs:425-434`) and, for the fundamentals, because they are
also emitted as `SvgFundamental` variants rendered by `render_svg_variant`
(`crates/wcl_wdoc/src/render/lower.rs:358-364`).

### 2.7 Blocks no backend can render as page content

These are declared `@block`s that are *not* `WdocBlock`s at all — configuration,
schema, or slot vocabulary. They render nothing anywhere because they never reach a
content dispatch. Established for: `frontmatter` (`lib/core.wcl:204`, inert via
`expand.rs:287`), `body` (`lib/project.wcl:56`, inert via `expand.rs:314`),
`class`/`dark`/`light` (`lib/core.wcl:211,264,291` — consumed by the CSS pass,
`crates/wcl_wdoc/src/render/css.rs`), `stylesheet` (`lib/core.wcl:60`),
`page`/`site`/`template`/`toc`/`chapter`/`menu`/`item`/`sidebar_footer`/`button`/`skill`/
`agent` (`lib/core.wcl:152`, `lib/templates.wcl:112-327`),
`deck`/`section`/`slide` (`lib/presentation.wcl:34-47`),
`iconset`/`icon_def` (`lib/icons.wcl:18,40`), `tileset`/`tile` (`lib/tilemap.wcl:10,34`),
`theme`/`palette` (`lib/theme.wcl:66,84`), `inline_pattern` (`lib/inline.wcl:31`),
`include` (`lib/include.wcl:65`), `wdoc_component`/`wdoc_slot`/`wdoc_body`
(`lib/components.wcl:36,46,57`), `comment` (`lib/comment.wcl:30`),
`option` (`lib/answer.wcl:59`). See the inventory in §6 for the full classification.

---

## 3. What each backend does with SVG

### 3.1 `render_lowered_svg_block` is one of several shared seams, not the only one

`render_lowered_svg_block` (`crates/wcl_wdoc/src/render/svg/standalone.rs:20-68`) is
shared by all three backends for `sequence_diagram` / `state_diagram` only:
`html.rs:569`, `collect.rs:98`, `emit.rs:173`. Its own doc comment says exactly this
(`standalone.rs:6-9`).

The **other** shared SVG seams:

| seam | function | HTML | PDF | Markdown |
|---|---|---|---|---|
| diagrams | `render_diagram` / `render_diagram_static` (`crates/wcl_wdoc/src/render/svg/diagram.rs:19`, `:33`) | `render_diagram` `html.rs:565` | `render_diagram` `collect.rs:91` | `render_diagram_static` `emit.rs:165` |
| terminals | `terminal::render_terminal` vs `render_terminal_pdf` | `render_terminal` `html.rs:575` | `render_terminal_pdf` `collect.rs:120` | `render_terminal_pdf` `emit.rs:178` |
| math | `math::render_math_fundamental` (a `<svg>` string) | `lower.rs:471` | `collect.rs:295-297` | **not used** — Markdown emits raw `$$…$$` LaTeX instead (`emit.rs:316-319`) |
| shape dispatch | `render_shape` (`crates/wcl_wdoc/src/render/svg/shapes.rs:417`) | reached via `render_diagram` | same | same |
| shape lowering | `lower_svg_block` (`crates/wcl_wdoc/src/render/lower.rs:255`) | same | same | same |

So: **four** shared SVG production seams (`render_diagram*`, `render_lowered_svg_block`,
`render_terminal*`, `render_math_fundamental`), of which three are used by all three
backends and one (math SVG) by two.

### 3.2 What each does with the resulting SVG string

- **HTML** — embeds the string inline in the page body (`html.rs:565`, `:569`, `:575`).
  `render_diagram_inner` may wrap it in `<div class="wdoc-diagram-viewport">…</div>` with
  pan/zoom data attributes and JS-driven controls when `pan_zoom = true` or the diagram
  contains a `map` (`crates/wcl_wdoc/src/render/svg/diagram.rs:113-142`).
- **PDF** — carries the string in `BlockNode::Svg { svg }` or
  `BlockNode::Diagram { svg, viewbox, cards }` (`crates/wcl_wdoc/src/pdf/ir.rs:113`,
  `:139-144`) and parses it with usvg via `SvgEmbedder`
  (`crates/wcl_wdoc/src/pdf/svg_embed.rs:130`). Notably PDF calls the **non-static**
  `render_diagram` (`collect.rs:91`), so an interactive diagram arrives wrapped in the
  `<div>`; `svg_embed::extract_svg` (`svg_embed.rs:385`) exists to pull the `<svg>` back
  out. Before parsing, `svg_embed` substitutes `currentColor` with a concrete colour and
  supplies a class stylesheet, because usvg resolves neither
  (`crates/wcl_wdoc/src/pdf/svg_embed.rs:4-9`). It also **replaces every
  `<foreignObject>` with a plain box** (`replace_foreign_objects`, `svg_embed.rs:261`) and
  the card bodies are re-collected as native PDF blocks and painted over the boxes
  (`collect_diagram`, `crates/wcl_wdoc/src/pdf/collect.rs:347-392`).
- **Markdown** — writes the string to `<out_dir>/_wdoc/<page>-<kind>-<n>.svg` and emits
  `![alt](_wdoc/…)` (`Emitter::write_svg`, `crates/wcl_wdoc/src/markdown/emit.rs:352-367`;
  namespace injected defensively by `ensure_svg_namespace`, `emit.rs:592`). Because it
  uses `render_diagram_static`, pan/zoom degrades to the fitted static view
  (`crates/wcl_wdoc/src/render/svg/diagram.rs:27-32`).

### 3.3 Edit-mode SVG anchoring is HTML-preview-only

`shape_anchor_attrs` returns an empty string outside edit mode
(`crates/wcl_wdoc/src/render/svg/shapes.rs:385-401`), and its doc comment states that
plain / comment-mode / Markdown / PDF builds are byte-identical (`shapes.rs:397-400`).
Same for `anchor_block` on the HTML side (`crates/wcl_wdoc/src/render/html.rs:640-643`).

---

## 4. What PDF and Markdown need that `HtmlFundamental` can't express

Established from the code, per backend.

### 4.1 PDF

`HtmlFundamental` carries *HTML strings*; the PDF painter needs measurable, paginatable,
typed geometry. Evidence:

- The PDF IR is a distinct, paint-agnostic model —
  `BlockNode` / `InlineRun` / `TextStyle` / `CodeSpan` / `ListLine` / `CardSpec`
  (`crates/wcl_wdoc/src/pdf/ir.rs:9-155`) — with an explicit three-phase pipeline
  comment: collect → layout/paginate → paint (`crates/wcl_wdoc/src/pdf/ir.rs:1-7`).
- **Font family and style must be typed, not CSS**: `FontFamily::{Serif,Sans,Mono}`
  (`ir.rs:12-19`) mapped to bundled Noto faces by `pdf::text::FontBook`
  (`ir.rs:9-10`); `TextStyle::heading()` / `body()` / `code()` (`ir.rs:29-57`).
- **Code needs per-token RGB, not CSS classes**: `CodeSpan { text, color: (u8,u8,u8) }`
  (`ir.rs:73-77`); `collect.rs:300-313` re-runs `highlight::highlight_spans` to get
  colours, because the `Highlighted` fundamental only carries `source` + `language`
  (`crates/wcl_wdoc/src/render/lower.rs:468`, rendered to `<span class="tok-…">` for HTML
  at `crates/wcl_wdoc/src/render/html.rs:1192-1198`).
- **Lists must be pre-flattened with resolved markers**, because there is no CSS counter:
  `ListLine { depth, marker, runs }` (`ir.rs:79-86`) built by `collect_li_group`
  (`collect.rs:569-622`), with a hand-rolled bullet cycle `• ◦ ▪` (`collect.rs:624-630`)
  and hand-built `1.2.` number paths (`collect.rs:581-586`). HTML instead emits
  `<ol class="wdoc-list-numbered">` and lets a CSS counter do it
  (`crates/wcl_wdoc/src/render/html.rs:920-949`).
- **Callout accent must be an RGB triple**, not a CSS custom property:
  `callout_accent` (`collect.rs:530-542`) hard-codes the same colours the WCL
  `--callout-accent` uses, with a comment saying so (`collect.rs:528-529`).
- **Tables must be run-lists, not HTML**: `Table { header: Row, rows: Vec<Row> }` where
  `Cell = Vec<InlineRun>` (`ir.rs:99-102`, `:119`), built by `collect_table`
  (`collect.rs:410-444`) so the layout pass can measure cells.
- **Images must be embedded bytes**: `BlockNode::Image { bytes, disp_w, disp_h }`
  (`ir.rs:131-135`), read from disk at collect time (`collect_image`, `collect.rs:448-480`)
  — there is no asset-copy step, and the code says exactly that (`collect.rs:466-470`).
- **`<foreignObject>` is unsupported by usvg**, so any HTML-in-SVG has to be re-collected
  natively: `replace_foreign_objects` (`crates/wcl_wdoc/src/pdf/svg_embed.rs:261`) plus
  `collect_diagram`'s card pairing (`collect.rs:347-392`). This is the sharpest single
  example: the `card` block's content is HTML in the HTML backend and native PDF IR here.
- **`currentColor` and CSS classes don't exist**: the embedder substitutes a concrete
  foreground colour and synthesises a `style_sheet` (`svg_embed.rs:4-9`, `:70-88`).

### 4.2 Markdown

- **Markdown has no template**, so `region` (which is an HTML template slot) becomes
  transparent (`emit.rs:150-155`).
- **Markdown has no theming**, so the `demo` block's dual light/dark preview cannot exist
  — stated in the comment at `emit.rs:201-204`.
- **Markdown cannot inline SVG usefully**, so every SVG becomes a written-out file plus
  `![](…)` (`emit.rs:352-367`).
- **Markdown wants the *raw* source, not highlighted markup**: `Emitter::code` reads the
  block's `source` field and language label directly and emits a fence
  (`emit.rs:451-455`), and `fence` widens the backtick run to survive embedded fences
  (`emit.rs:569-573`). The `Highlighted` fundamental path also exists as a fallback
  (`emit.rs:322-326`) — proof that the fundamental *can* be consumed, but the special-case
  is preferred.
- **Markdown wants textual math**, not an SVG: `$$\n{latex}\n$$` (`emit.rs:316-319`),
  explicitly "the Markdown target keeps math textual rather than rasterizing it".
- **Cell / link text needs Markdown-specific escaping**: `escape_cell` (`emit.rs:608`),
  `escape_link_text` (`emit.rs:600`) — pipes, brackets, newlines.
- **Callouts must map onto GitHub alert keywords**: `callout_alert` (`emit.rs:527-539`)
  maps the callout's CSS type class to `NOTE`/`TIP`/`WARNING`/`CAUTION`.
- **A separate inline engine mode**: `InlinePatterns::render_markdown` (used at
  `emit.rs:339`) vs HTML's `render` (`html.rs:1146`) vs PDF's `render_runs`
  (`collect.rs:411` and elsewhere) — three renderings of the same inline text, selected by
  `crate::inline::Backend` (`crates/wcl_wdoc/src/markdown/skill.rs:303`, `:414`).

### 4.3 What the shared fundamental layer *does* buy them

Both non-HTML backends consume `lower_to_values` (`crates/wcl_wdoc/src/render/lower.rs:180`)
— the *raw `Value` variants*, not rendered HTML (`collect.rs:193`, `emit.rs:239`). The
module docs of both name this as the deliberate shared seam
(`crates/wcl_wdoc/src/pdf/collect.rs:3-12`, `crates/wcl_wdoc/src/markdown/emit.rs:1-14`).
So the reason for separate passes is **not** that they can't reach the lowering; it's that
the *fundamental vocabulary itself* is HTML-shaped (`Element { tag, attrs }`, `Raw { html }`,
`Highlighted` without colours, `Paragraph` with `class` strings) and only a subset of it
survives translation — see the fundamental-coverage table in §1.2.

---

## 5. Where the degradation rules live

**Finding: degradation is a per-block decision, scattered across the backends, with no
central mechanism.** There are four distinct patterns in use.

### Pattern A — a stub `lower` plus a Rust arm in only some backends

The block renders in the backends that name it and vanishes in the rest. No declaration
anywhere says which. Examples: `column` (HTML only, `html.rs:533`), `markdown_source`
(HTML only, `html.rs:581`), `edit_object` (HTML only, `html.rs:591`), `file` (not PDF,
§2.4). The only trace is prose in the HTML comments — e.g. `markdown_source`'s
"Book-only; in other backends its stub `lower` makes it render empty"
(`crates/wcl_wdoc/src/render/html.rs:579-580`).

### Pattern B — an explicit divergent Rust arm per backend

Each backend hand-writes its own degraded form. `demo` is the clearest:

| backend | behaviour | line |
|---|---|---|
| HTML | source listing + live preview under both palettes | `html.rs:585` → `crates/wcl_wdoc/src/demo.rs` |
| Markdown | fenced ```` ```wcl ```` source (`demo::demo_source`) + one render of children | `emit.rs:204-212` |
| PDF | children only — **no source listing at all** | `collect.rs:181-186` |

`region`, `fragment`, `video` and `wdoc_content` follow the same pattern with different
per-backend bodies (see §2.1).

### Pattern C — a mode/flag test inside one backend

`edit_object` is gated on `patterns.edit_mode()` at `html.rs:591-595`; the mode is
invisible to a WCL `lower`, which the comment states (`html.rs:586-590`).
`anchor_block` (`html.rs:640`) and `shape_anchor_attrs`
(`crates/wcl_wdoc/src/render/svg/shapes.rs:385`) do the same for anchoring.
`render_diagram_static` vs `render_diagram` is a variant of this — a boolean threaded into
one shared renderer (`crates/wcl_wdoc/src/render/svg/diagram.rs:42-47`, `:114`).

### Pattern D — a declarative, block-authored axis: `@only` / `@except`

The one *declarative* degradation mechanism is the visibility system:
`crate::visibility::block_visible(block, patterns)` inside `walk_structural`
(`crates/wcl_wdoc/src/render/expand.rs:283`), implemented in
`crates/wcl_wdoc/src/visibility.rs` (152 lines) with the decorators declared in
`crates/wcl_wdoc/lib/visibility.wcl`. This is uniform across all three backends because
it lives in the shared walker.

**It does have a backend axis** — three axes in fact
(`crates/wcl_wdoc/src/visibility.rs:63-72`, declared at
`crates/wcl_wdoc/lib/visibility.wcl:21-33`):

| axis | current value read from | values |
|---|---|---|
| `sites` | `InlinePatterns::vis_site()` | the `site` block's `@inline(0) name` |
| `templates` | `InlinePatterns::vis_template()` | the site's `default_template` (`:webpage` / `:book` / `:presentation`) |
| `backends` | `InlinePatterns::backend().symbol()` (`crates/wcl_wdoc/src/inline.rs:49-55`) | **`:html` / `:pdf` / `:markdown` — only three** |

Semantics (`visibility.rs:41-58`): within an axis values OR; specified axes AND; a block
renders iff `@only` (if present) matches AND `@except` (if present) does not fully match;
an axis whose current value is unknown never matches (`visibility.rs:81-84`). The whole
predicate is bypassed in the editor's merged all-views preview (`visibility.rs:34-36`).

Two facts about that axis worth recording:

- **There is no `:skill` value.** `Backend` is a three-variant enum
  (`crates/wcl_wdoc/src/inline.rs:40-44`) and the skill target constructs its
  `InlinePatterns` with `crate::inline::Backend::Markdown`
  (`crates/wcl_wdoc/src/markdown/skill.rs:303`, `:414`) — so `@except(backends=[:markdown])`
  hides a block from the skill folder too, and nothing can target one and not the other.
- It is **per-instance authoring**, not a per-kind declaration: it lets an author say "not
  in the PDF"; it cannot say "this block *kind* has no PDF implementation".

**Summary answer to Q5**: there is no pattern in the sense of a shared mechanism. There
are four unrelated mechanisms; only one (D) is declarative, and it is instance-scoped
author intent rather than kind-scoped capability. The *closest* thing to a convention for
capability is "give the block a stub `lower` and let
absence-of-Rust-arm mean absence-of-output", which is Pattern A — and it is silent: a
block that vanishes in PDF produces no warning. (Contrast: a kind with *no* `lower` at
all is a hard build error with a diagnostic, `crates/wcl_wdoc/src/render/lower.rs:207-220`.)

---

## 6. Honest inventory — all 134 `@block` types vs backend support

### 6.1 How to read it

- **`lower`** — `real` = a WCL `lower` with a body; `stub` = `lower = fn(..) -> list<..> []`
  (renders nothing unless a backend names the kind in Rust); `—` = the type declares no
  `lower` at all (it is not a renderable block).
- **Column codes** — `R` = a Rust arm in that backend's dispatch; `L` = consumed via the
  WCL `lower` + that backend's fundamental walker; `D` = a diagram child, rendered by the
  shared `render_shape` seam inside a `diagram`; `T` = a terminal primitive, drawn inside a
  `terminal`; `∅` = falls through to a stub `lower` and emits **nothing**; `n/a` = not page
  content (configuration, a schema slot, or a child consumed by its parent).
- Every cell cites the dispatch site; unqualified `html.rs` / `collect.rs` / `emit.rs` /
  `expand.rs` / `shapes.rs` / `diagram.rs` are under `crates/wcl_wdoc/src/`
  (`render/html.rs`, `pdf/collect.rs`, `markdown/emit.rs`, `render/expand.rs`,
  `render/svg/shapes.rs`, `render/svg/diagram.rs`). `lib/*.wcl` are under
  `crates/wcl_wdoc/lib/`.

### 6.2 Population breakdown (134 total)

| category | count | how it renders |
|---|---|---|
| **page content** (`extends WdocBlock`, appears in a `page` body) | **34** | the three per-backend dispatches |
| **diagram children** (`extends SvgBlock` / `Widget`) | **40** | one shared seam: `render_shape` (`crates/wcl_wdoc/src/render/svg/shapes.rs:417`) inside a `diagram` |
| **terminal primitives** (`TermPrimitive` / `TuiWidget`) | **13** | one shared seam: `terminal::widgets::draw_variant` (`crates/wcl_wdoc/src/terminal/widgets.rs:80`) inside a `terminal` |
| **children consumed by their parent** (`participant`, `state`, `pin`, `span`, `wf_node`, …) | **12** | the parent's `lower` / Rust renderer |
| **structural** (handled by `walk_structural`) | **6** | `crates/wcl_wdoc/src/render/expand.rs:277` |
| **configuration / schema, never page content** | **29** | registries, CSS, templates, site config |

Counts derived by classifying the table below; the classification script's own tally is
reproduced here (34 + 40 + 13 + 12 + 6 + 29 = 134).

### 6.3 Is "renders everywhere" the common case?

**Among the 34 page-content blocks: 29 render in all three backends and 5 do not.**
The five that do not:

| kind | HTML | PDF | Markdown / skill |
|---|---|---|---|
| `column` (`lib/text.wcl:50`) | ✅ `html.rs:533` | ✗ nothing | ✗ nothing |
| `edit_object` (`lib/edit_object.wcl:23`) | ✅ edit mode only, `html.rs:591` | ✗ nothing | ✗ nothing |
| `markdown_source` (`lib/markdown_source.wcl:20`) | ✅ `html.rs:581` | ✗ nothing | ✗ nothing |
| `file` (`lib/file.wcl:22`) | ✅ `html.rs:560` | ✗ nothing (§2.4) | ✅ `emit.rs:190` |
| `footnote` (`lib/footnotes.wcl:14`) | ✅ via the `footnotes` lower | ✗ nothing | ✗ nothing |

**But "renders in all three" is not the same as "renders equivalently."** Of the 29 that
reach all three backends, at least these lose content or meaning outside HTML — all
established from the code paths above:

| kind | what is lost outside HTML | evidence |
|---|---|---|
| `chapter_header` | `kicker`, `reading_time`, `updated`, `version` all vanish — they are `HtmlFundamental::Raw`, which PDF drops (`collect.rs:314`) and Markdown's `gather_inline_text` drops (`accessors.rs:344`). Only the `Paragraph{heading-1}` title survives. | `lib/chapter_header.wcl:26-56` |
| `footnotes` | the "Footnotes" title and the `<ol>` numbering are `Raw` / `ol`-tag structure; PDF/Markdown emit each note as a loose paragraph. `[^id]` references stay literal text (linking is `process_footnotes`, `html.rs:433`). | `lib/footnotes.wcl:28-58`, `crates/wcl_wdoc/src/render/headings.rs:152` |
| `code` | PDF loses the code-card filename header (a `Raw`); Markdown bypasses highlighting entirely. | `lib/code.wcl:29-36`, `emit.rs:451` |
| `callout` | the icon (`HtmlFundamental::Icon`) has no PDF/Markdown renderer; both re-derive the callout from the block instead. | `lib/callout.wcl:45`, `collect.rs:137`, `emit.rs:200` |
| `demo` | PDF loses the source listing entirely; both lose the dual light/dark preview. | `collect.rs:181`, `emit.rs:204` |
| `video` | never plays; PDF = poster + link, Markdown = link only. | `collect.rs:486`, `emit.rs:496` |
| `region` / `fragment` | their layout/step-reveal semantics disappear (transparent wrappers). | `collect.rs:64,72`, `emit.rs:143,150` |
| `diagram` containing `node_table` | **in PDF the row text is lost**: `card_rects` counts every `<foreignObject>` (`svg_embed.rs:349`) but `collect_card_blocks` counts only `card` blocks (`collect.rs:398`), so a `node_table`'s header + row foreignObjects (`node_table.rs:177,222`) make the counts disagree and `collect_diagram` falls back to `BlockNode::Svg` (`collect.rs:361-363`) — after which `replace_foreign_objects` turns **every** foreignObject into an empty box (`svg_embed.rs:261`, `card_box` "no content", `svg_embed.rs:291`). | as cited |
| `diagram` containing `map` | map pin popup cards are HTML overlays spliced *outside* the `<svg>` only in interactive mode (`diagram.rs:133-142`). Markdown uses `render_diagram_static` ⇒ overlays never emitted. PDF uses the interactive path but `extract_svg` takes only the `<svg>` (`svg_embed.rs:385`) ⇒ overlays dropped. | as cited |
| `diagram` containing `card` / `timeline` cards in Markdown | the `<foreignObject>` XHTML is written verbatim into a standalone `.svg` file referenced by `![](…)`. **NOT DETERMINED** whether that renders — it depends on the consuming Markdown renderer's treatment of HTML inside an image-referenced SVG, which is not a fact about this repo. | `emit.rs:352`, `card.rs:82` |

### 6.4 The 40 diagram children and 13 terminal primitives are backend-uniform

This is the *good* news half of the inventory and it is worth stating precisely: those 53
blocks have exactly **one** dispatcher each (`render_shape`,
`crates/wcl_wdoc/src/render/svg/shapes.rs:417`; `draw_variant`,
`crates/wcl_wdoc/src/terminal/widgets.rs:80`), reached by all three backends through
`render_diagram*` / `render_terminal*`. Adding a diagram shape or a TUI widget therefore
costs one implementation, not three — whereas adding a page block costs up to three plus
the fundamental-walker coverage.

### 6.5 Full table

| # | kind | declared | extends | `lower` | HTML | PDF | Markdown / skill |
|---|---|---|---|---|---|---|---|
| 1 | `agent` | `lib/templates.wcl:312` | — | — | n/a — skill target only (skill.rs:268) | n/a | n/a |
| 2 | `bar_chart` | `lib/charts.wcl:297` | SvgBlock | real | D lower_svg_block | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 3 | `body` | `lib/project.wcl:56` | — | — | shared walker expand.rs:314 | shared walker expand.rs:314 | shared walker expand.rs:314 |
| 4 | `boundary` | `lib/diagram-core.wcl:241` | SvgBlock | stub | D diagram.rs render_boundaries (shapes.rs:434 → None) | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 5 | `button` | `lib/templates.wcl:164` | — | — | n/a — sidebar_footer child | n/a | n/a |
| 6 | `callout` | `lib/callout.wcl:13` | WdocBlock | real | L (lib/callout.wcl:26) | R collect.rs:137 | R emit.rs:200 |
| 7 | `card` | `lib/card.wcl:20` | SvgBlock | stub | D shapes.rs:480 (foreignObject) | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 8 | `chapter` | `lib/templates.wcl:112` | — | — | n/a — toc child | n/a | n/a |
| 9 | `chapter_header` | `lib/chapter_header.wcl:12` | WdocBlock | real | L full (kicker + h1 + meta) | L → title only; Raw dropped (collect.rs:314) | L → title only; Raw kept only at block level (emit.rs:328) — nested Raw dropped by gather_inline_text (accessors.rs:344) |
| 10 | `circle` | `lib/diagram-core.wcl:310` | SvgBlock | stub | D shapes.rs:426 | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 11 | `class` | `lib/core.wcl:211` | — | — | n/a — CSS — HTML only (render/css.rs) | n/a | n/a |
| 12 | `code` | `lib/code.wcl:16` | WdocBlock | real | L → Highlighted (lib/code.wcl:28) | L → highlighted collect.rs:300 | R emit.rs:184 |
| 13 | `collect` | `lib/components.wcl:151` | WdocBlock | stub | shared walker expand.rs:298 | shared walker expand.rs:298 | shared walker expand.rs:298 |
| 14 | `column` | `lib/text.wcl:50` | WdocBlock | stub | R html.rs:533 | ∅ (no arm) | ∅ (no arm) |
| 15 | `comment` | `lib/comment.wcl:30` | — | — | n/a — review sidecar (comments.rs) | n/a | n/a |
| 16 | `container` | `lib/diagram-core.wcl:126` | SvgBlock | stub | D shapes.rs:430 | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 17 | `dark` | `lib/core.wcl:264` | — | — | n/a — class child | n/a | n/a |
| 18 | `decision` | `lib/flowchart.wcl:107` | SvgBlock | real | D lower_svg_block | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 19 | `deck` | `lib/presentation.wcl:47` | — | — | n/a — read_deck — HTML presentation only (build.rs:1474) | n/a | n/a |
| 20 | `demo` | `lib/demo.wcl:28` | WdocBlock | stub | R html.rs:585 (src + dual preview) | R collect.rs:181 (children only) | R emit.rs:204 (fence + children) |
| 21 | `diagram` | `lib/diagram-core.wcl:5` | WdocBlock | stub | R html.rs:565 render_diagram | R collect.rs:91 render_diagram (non-static) | R emit.rs:165 render_diagram_static → .svg file |
| 22 | `dopesheet` | `lib/dopesheet.wcl:18` | SvgBlock | stub | D shapes.rs:450 | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 23 | `edit_field` | `lib/edit_field.wcl:28` | WdocBlock | stub | R html.rs:600 | R collect.rs:80 | R emit.rs:157 |
| 24 | `edit_object` | `lib/edit_object.wcl:23` | WdocBlock | stub | R html.rs:591 (edit mode only) | ∅ (no arm) | ∅ (no arm) |
| 25 | `file` | `lib/file.wcl:22` | WdocBlock | stub | R html.rs:560 | ∅ (no arm — see §2.4) | R emit.rs:190 |
| 26 | `footnote` | `lib/footnotes.wcl:14` | WdocBlock | stub | consumed by footnotes lower | ∅ (stub lower) | ∅ (stub lower) |
| 27 | `footnotes` | `lib/footnotes.wcl:24` | WdocBlock | real | L + [^id] linking html.rs:433 (templated pages only) | L → bare paragraphs; no title/numbering | L → bare paragraphs; no title/numbering |
| 28 | `fragment` | `lib/presentation.wcl:78` | WdocBlock | stub | R html.rs:543 (step-reveal div) | R collect.rs:64 (transparent) | R emit.rs:143 (transparent) |
| 29 | `frontmatter` | `lib/core.wcl:204` | — | — | shared walker expand.rs:287 | shared walker expand.rs:287 | shared walker expand.rs:287 |
| 30 | `h1` | `lib/headings.wcl:5` | WdocBlock | real | L → Paragraph(heading-N) | L → Heading collect.rs:243 | L → "# …" emit.rs:283 |
| 31 | `h2` | `lib/headings.wcl:16` | WdocBlock | real | L → Paragraph(heading-N) | L → Heading collect.rs:243 | L → "# …" emit.rs:283 |
| 32 | `h3` | `lib/headings.wcl:27` | WdocBlock | real | L → Paragraph(heading-N) | L → Heading collect.rs:243 | L → "# …" emit.rs:283 |
| 33 | `h4` | `lib/headings.wcl:38` | WdocBlock | real | L → Paragraph(heading-N) | L → Heading collect.rs:243 | L → "# …" emit.rs:283 |
| 34 | `h5` | `lib/headings.wcl:49` | WdocBlock | real | L → Paragraph(heading-N) | L → Heading collect.rs:243 | L → "# …" emit.rs:283 |
| 35 | `h6` | `lib/headings.wcl:60` | WdocBlock | real | L → Paragraph(heading-N) | L → Heading collect.rs:243 | L → "# …" emit.rs:283 |
| 36 | `icon` | `lib/icons.wcl:61` | SvgBlock | stub | D shapes.rs:436 | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 37 | `icon_def` | `lib/icons.wcl:40` | — | — | child of parent: icons.rs | same | same |
| 38 | `iconset` | `lib/icons.wcl:18` | — | — | n/a — IconRegistry::load | n/a | n/a |
| 39 | `image` | `lib/image.wcl:15` | WdocBlock, SvgBlock | stub | R html.rs:555 | R collect.rs:126 (embedded bytes) | R emit.rs:185 |
| 40 | `include` | `lib/include.wcl:65` | — | — | n/a — `include.rs`, HTML fan-out (build.rs:1069) | n/a — **inert** (no `include::` reference in `src/pdf/`) | n/a — **inert** in plain Markdown; **fan-out in the skill target** (skill.rs:225) |
| 41 | `inline_pattern` | `lib/inline.wcl:31` | — | — | n/a — InlinePatterns::load | n/a | n/a |
| 42 | `item` | `lib/templates.wcl:135` | — | — | n/a — menu child | n/a | n/a |
| 43 | `label` | `lib/diagram-core.wcl:389` | SvgBlock | stub | D shapes.rs:428 | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 44 | `layer` | `lib/map.wcl:72` | — | — | child of parent: map.rs | same | same |
| 45 | `li` | `lib/list.wcl:54` | ListNode | — | R html.rs:551 / 956 | via collect_li_group collect.rs:579 | via li_group emit.rs:387 |
| 46 | `light` | `lib/core.wcl:291` | — | — | n/a — class child | n/a | n/a |
| 47 | `line` | `lib/diagram-core.wcl:353` | SvgBlock | stub | D shapes.rs:427 | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 48 | `line_chart` | `lib/charts.wcl:342` | SvgBlock | real | D lower_svg_block | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 49 | `list` | `lib/list.wcl:41` | WdocBlock, ListNode | stub | R html.rs:550 | R collect.rs:104 | R emit.rs:182 |
| 50 | `map` | `lib/map.wcl:26` | SvgBlock | stub | D shapes.rs:476 | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 51 | `markdown_source` | `lib/markdown_source.wcl:20` | WdocBlock | stub | R html.rs:581 | ∅ (no arm) | ∅ (no arm) |
| 52 | `math` | `lib/math.wcl:22` | WdocBlock | real | L → Math fundamental lower.rs:471 | L → Svg node collect.rs:295 | L → $$latex$$ emit.rs:316 |
| 53 | `menu` | `lib/templates.wcl:151` | — | — | n/a — read_menu — HTML only (build.rs:1473) | n/a | n/a |
| 54 | `message` | `lib/sequence.wcl:228` | — | — | child of parent: sequence lower_svg | same | same |
| 55 | `node` | `lib/flowchart.wcl:173` | SvgBlock | real | D lower_svg_block | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 56 | `node_row` | `lib/node_table.wcl:57` | SvgBlock | stub | D via node_table | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 57 | `node_table` | `lib/node_table.wcl:21` | SvgBlock | stub | D shapes.rs:458 | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 58 | `note` | `lib/sequence.wcl:242` | — | — | child of parent: sequence lower_svg | same | same |
| 59 | `notes` | `lib/presentation.wcl:91` | WdocBlock | stub | shared walker expand.rs:287 | shared walker expand.rs:287 | shared walker expand.rs:287 |
| 60 | `option` | `lib/answer.wcl:59` | — | — | n/a — answer mode (answer.wcl — NOT in the wdoc prelude) | n/a | n/a |
| 61 | `p` | `lib/p.wcl:12` | WdocBlock | real | L → Element p + Inline | L → Paragraph collect.rs:261 | L → para emit.rs:296 |
| 62 | `page` | `lib/core.wcl:152` | — | — | n/a — page shell | n/a | n/a |
| 63 | `palette` | `lib/theme.wcl:66` | — | — | n/a — theme child | n/a | n/a |
| 64 | `partial` | `lib/components.wcl:143` | WdocBlock | stub | shared walker expand.rs:288 | shared walker expand.rs:288 | shared walker expand.rs:288 |
| 65 | `participant` | `lib/sequence.wcl:214` | — | — | child of parent: sequence lower_svg (sequence.wcl:279) | same | same |
| 66 | `pie_chart` | `lib/charts.wcl:406` | SvgBlock | real | D lower_svg_block | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 67 | `pin` | `lib/map.wcl:91` | — | — | child of parent: map.rs | same | same |
| 68 | `polygon` | `lib/diagram-core.wcl:431` | SvgBlock | stub | D shapes.rs:429 | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 69 | `process` | `lib/flowchart.wcl:47` | SvgBlock | real | D lower_svg_block shapes.rs:498 | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 70 | `project` | `lib/project.wcl:69` | WdocBlock | stub | shared walker expand.rs:315 | shared walker expand.rs:315 | shared walker expand.rs:315 |
| 71 | `rect` | `lib/diagram-core.wcl:265` | SvgBlock | stub | D shapes.rs:425 | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 72 | `region` | `lib/website.wcl:33` | WdocBlock | stub | R html.rs:538 (→ empty; hoisted by build.rs:2051) | R collect.rs:72 (children inline) | R emit.rs:150 (children inline) |
| 73 | `section` | `lib/presentation.wcl:40` | — | — | n/a — deck child | n/a | n/a |
| 74 | `sequence_diagram` | `lib/sequence.wcl:252` | WdocBlock | stub | R html.rs:568 | R collect.rs:97 | R emit.rs:171 |
| 75 | `sidebar_footer` | `lib/templates.wcl:175` | — | — | n/a — read_sidebar_footer — HTML only (build.rs:1478) | n/a | n/a |
| 76 | `site` | `lib/templates.wcl:183` | — | — | n/a — site config | n/a | n/a |
| 77 | `skill` | `lib/templates.wcl:292` | — | — | n/a — skill target front matter (skill.rs:370) | n/a | n/a |
| 78 | `slide` | `lib/presentation.wcl:34` | — | — | n/a — deck child | n/a | n/a |
| 79 | `span` | `lib/text.wcl:38` | — | — | child of parent: text lower (text.wcl:26) | same | same |
| 80 | `state` | `lib/statechart.wcl:341` | — | — | child of parent: statechart lower_svg (statechart.wcl:404) | same | same |
| 81 | `state_diagram` | `lib/statechart.wcl:379` | WdocBlock | stub | R html.rs:568 | R collect.rs:97 | R emit.rs:171 |
| 82 | `stylesheet` | `lib/core.wcl:60` | — | — | n/a — CSS — HTML only | n/a | n/a |
| 83 | `table` | `lib/table.wcl:33` | WdocBlock | stub | R html.rs:544 | R collect.rs:111 | R emit.rs:183 |
| 84 | `template` | `lib/templates.wcl:327` | — | — | n/a — HTML template only (render_template, build.rs:1969) | n/a | n/a |
| 85 | `term_box` | `lib/tui.wcl:114` | TuiWidget | real | T inside `terminal` (widgets.rs:80) | T (render_terminal_pdf) | T (render_terminal_pdf) |
| 86 | `term_fill` | `lib/tui.wcl:153` | TuiWidget | real | T inside `terminal` (widgets.rs:80) | T (render_terminal_pdf) | T (render_terminal_pdf) |
| 87 | `term_glyph` | `lib/tui.wcl:137` | TuiWidget | real | T inside `terminal` (widgets.rs:80) | T (render_terminal_pdf) | T (render_terminal_pdf) |
| 88 | `term_text` | `lib/terminal.wcl:82` | TermPrimitive | — | T inside `terminal` (widgets.rs:80) | T (render_terminal_pdf) | T (render_terminal_pdf) |
| 89 | `terminal` | `lib/terminal.wcl:36` | WdocBlock | stub | R html.rs:575 render_terminal | R collect.rs:118 render_terminal_pdf | R emit.rs:178 render_terminal_pdf → .svg file |
| 90 | `terminator` | `lib/flowchart.wcl:235` | SvgBlock | real | D lower_svg_block | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 91 | `text` | `lib/text.wcl:11` | WdocBlock | real | L → Element p + Inline/span | L → Paragraph collect.rs:261 | L → para emit.rs:296 |
| 92 | `theme` | `lib/theme.wcl:84` | — | — | n/a — HTML CSS + baked SVG palette (resolve_ui_theme) | n/a | n/a |
| 93 | `tile` | `lib/tilemap.wcl:34` | — | — | child of parent: tileset.rs | same | same |
| 94 | `tilemap` | `lib/tilemap.wcl:49` | SvgBlock | stub | D shapes.rs:439 | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 95 | `tileset` | `lib/tilemap.wcl:10` | — | — | n/a — TilesetRegistry::load | n/a | n/a |
| 96 | `timeline` | `lib/timeline.wcl:38` | SvgBlock | stub | D shapes.rs:483 | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 97 | `toc` | `lib/templates.wcl:123` | — | — | n/a — read_toc — HTML template + PDF outline (pdf/mod.rs:324); Markdown ignores | n/a | n/a |
| 98 | `transition` | `lib/statechart.wcl:365` | — | — | child of parent: statechart lower_svg | same | same |
| 99 | `tree` | `lib/tree.wcl:22` | SvgBlock | stub | D shapes.rs:467 | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 100 | `tree_node` | `lib/tree.wcl:56` | SvgBlock | stub | D via tree | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 101 | `tui_button` | `lib/tui.wcl:215` | TuiWidget | real | T inside `terminal` (widgets.rs:80) | T (render_terminal_pdf) | T (render_terminal_pdf) |
| 102 | `tui_checkbox` | `lib/tui.wcl:346` | TuiWidget | real | T inside `terminal` (widgets.rs:80) | T (render_terminal_pdf) | T (render_terminal_pdf) |
| 103 | `tui_dropdown` | `lib/tui.wcl:297` | TuiWidget | real | T inside `terminal` (widgets.rs:80) | T (render_terminal_pdf) | T (render_terminal_pdf) |
| 104 | `tui_group` | `lib/tui.wcl:430` | TuiWidget | real | T inside `terminal` (widgets.rs:80) | T (render_terminal_pdf) | T (render_terminal_pdf) |
| 105 | `tui_input` | `lib/tui.wcl:270` | TuiWidget | real | T inside `terminal` (widgets.rs:80) | T (render_terminal_pdf) | T (render_terminal_pdf) |
| 106 | `tui_panel` | `lib/tui.wcl:403` | TuiWidget | real | T inside `terminal` (widgets.rs:80) | T (render_terminal_pdf) | T (render_terminal_pdf) |
| 107 | `tui_progress` | `lib/tui.wcl:177` | TuiWidget | real | T inside `terminal` (widgets.rs:80) | T (render_terminal_pdf) | T (render_terminal_pdf) |
| 108 | `tui_radio` | `lib/tui.wcl:372` | TuiWidget | real | T inside `terminal` (widgets.rs:80) | T (render_terminal_pdf) | T (render_terminal_pdf) |
| 109 | `tui_spinner` | `lib/tui.wcl:243` | TuiWidget | real | T inside `terminal` (widgets.rs:80) | T (render_terminal_pdf) | T (render_terminal_pdf) |
| 110 | `video` | `lib/video.wcl:22` | WdocBlock | stub | R html.rs:564 | R collect.rs:133 (poster + link) | R emit.rs:195 (link) |
| 111 | `wdoc_body` | `lib/components.wcl:57` | — | — | n/a — component body | n/a | n/a |
| 112 | `wdoc_component` | `lib/components.wcl:36` | — | — | n/a — component definition | n/a | n/a |
| 113 | `wdoc_content` | `lib/components.wcl:65` | WdocBlock | stub | R html.rs:611 (sentinel) | R collect.rs:175/217 | R emit.rs:233/262 |
| 114 | `wdoc_instance` | `lib/components.wcl:119` | WdocBlock | stub | R html.rs:607 | R collect.rs:165 | R emit.rs:224 |
| 115 | `wdoc_repeater` | `lib/components.wcl:86` | WdocBlock | stub | R html.rs:604 | R collect.rs:157 | R emit.rs:215 |
| 116 | `wdoc_slot` | `lib/components.wcl:46` | — | — | n/a — component slot | n/a | n/a |
| 117 | `wf_browser` | `lib/wireframe.wcl:200` | Widget | stub | D wireframe | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 118 | `wf_button` | `lib/wireframe.wcl:91` | Widget | stub | D wireframe | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 119 | `wf_checkbox` | `lib/wireframe.wcl:131` | Widget | stub | D wireframe | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 120 | `wf_column` | `lib/wireframe.wcl:266` | Widget | stub | D wireframe | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 121 | `wf_dropdown` | `lib/wireframe.wcl:119` | Widget | stub | D wireframe | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 122 | `wf_grid` | `lib/wireframe.wcl:290` | Widget | stub | D wireframe | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 123 | `wf_input` | `lib/wireframe.wcl:105` | Widget | stub | D wireframe | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 124 | `wf_label` | `lib/wireframe.wcl:79` | Widget | stub | D wireframe shapes.rs:493 | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 125 | `wf_link` | `lib/wireframe.wcl:332` | — | — | child of parent: wireframe.rs | same | same |
| 126 | `wf_node` | `lib/wireframe.wcl:314` | — | — | child of parent: wireframe.rs | same | same |
| 127 | `wf_node_graph` | `lib/wireframe.wcl:342` | Widget | stub | D wireframe | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 128 | `wf_panel` | `lib/wireframe.wcl:249` | Widget | stub | D wireframe | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 129 | `wf_phone` | `lib/wireframe.wcl:215` | Widget | stub | D wireframe | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 130 | `wf_radio` | `lib/wireframe.wcl:145` | Widget | stub | D wireframe | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 131 | `wf_row` | `lib/wireframe.wcl:278` | Widget | stub | D wireframe | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 132 | `wf_tablet` | `lib/wireframe.wcl:232` | Widget | stub | D wireframe | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 133 | `wf_toggle` | `lib/wireframe.wcl:159` | Widget | stub | D wireframe | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |
| 134 | `wf_window` | `lib/wireframe.wcl:178` | Widget | stub | D wireframe | D (same seam, via render_diagram) | D (same seam, via render_diagram_static) |

---

## 7. Gaps — things I could NOT establish

1. **`skill` block field set** — I did not read `crates/wcl_wdoc/src/markdown/yaml.rs`, so
   the exact mapping from the `skill` block (`crates/wcl_wdoc/lib/templates.wcl:292`) to
   SKILL.md front matter is uncited. **Check** `yaml::skill_front_matter` and
   `yaml::agent_front_matter`.
2. ~~**`footnotes` on an untemplated HTML page**~~ — **RESOLVED**: an untemplated HTML page
   gets **neither** heading anchors nor footnote linking. `process_page_headings` /
   `process_footnotes` are called only inside `render_template`
   (`crates/wcl_wdoc/src/render/html.rs:432-433`), and the no-template branch of
   `build_normal_page` returns the raw content unchanged:
   `None => crate::render::Rendered { body: content.clone(), head: String::new() }`
   (`crates/wcl_wdoc/src/build.rs:2144-2147`). So `[^id]` stays literal text on a bare page.
3. **Whether `file`-in-PDF is deliberate** — no comment either way at
   `crates/wcl_wdoc/src/pdf/collect.rs:189-198`. **STILL NOT DETERMINED** (intent, not
   behaviour; behaviour is established in §2.4).
4. ~~**`include` in the PDF and plain-Markdown targets**~~ — **RESOLVED**: neither fans
   includes out. `grep -rn 'collect_includes\|include::' src/pdf/ src/markdown/ src/build.rs`
   finds `include::` only in `crates/wcl_wdoc/src/build.rs` (the HTML build, e.g. `:1069`)
   and `crates/wcl_wdoc/src/markdown/skill.rs:52,225`. `crates/wcl_wdoc/src/pdf/mod.rs`
   and `crates/wcl_wdoc/src/markdown/mod.rs` contain no reference to the module, so an
   `include` block is inert in `wcl wdoc pdf` and `wcl wdoc markdown` — a **fourth**
   support level for one block kind (HTML ✅, skill ✅, PDF ✗, Markdown ✗), and one more
   place where the skill target differs from plain Markdown despite sharing
   `Backend::Markdown`.
5. **Does `<foreignObject>` survive a standalone `.svg` referenced from Markdown?** Not a
   fact about this repo (see §6.3). **NOT DETERMINED.**
6. **`typedoc.wcl`'s `type_table` / `block_reference`** are `wdoc_component`s, not
   `@block`s (`crates/wcl_wdoc/lib/typedoc.wcl:78`, `:130`), so they are outside the 134
   and outside this survey. Their backend behaviour follows the generic
   `wdoc_component` expansion path.
7. **Presentation / website / book template internals** — I established only that
   `render_template` is HTML-only (called at `crates/wcl_wdoc/src/build.rs:1969` and
   `:2124`, nowhere else). The internals of the templates in
   `crates/wcl_wdoc/lib/templates.wcl` / `presentation.wcl` / `website.wcl` were **not**
   surveyed.
8. ~~**Exhaustive fundamental variant lists**~~ — **RESOLVED**: all three unions are
   declared in `crates/wcl_wdoc/lib/diagram-core.wcl:478` (`SvgFundamental`, 7 variants),
   `:488` (`HtmlFundamental`, 10 variants) and `crates/wcl_wdoc/lib/tui.wcl:38`
   (`TermFundamental`, 2 variants). Per-backend coverage is tabulated in §1.2. No declared
   variant is unhandled *everywhere*; four (`Table`, `Head`, `Children`, `Icon`) are
   handled *only* by HTML.
9. **Whether any `@block` outside `crates/wcl_wdoc/lib/` participates** — the inventory
   covers `crates/wcl_wdoc/lib/*.wcl` only (the wdoc stdlib). Documents may declare their
   own `@block(...) extends WdocBlock` / `SvgBlock` types, and `.wad/schema/` and the
   wskill schemas do. Those are **out of scope** here; they take the `L` (WCL `lower`)
   path, which per §1.2 means they are HTML-complete and partially supported elsewhere.

---

## 8. Observations for the design discussion

*Strictly separated from the findings; these are things noticed while gathering, not
recommendations.*

1. **The `kinds.rs` module doc is factually wrong today** (`crates/wcl_wdoc/src/kinds.rs:3-8`
   claims all three backends special-case the same vocabulary). Three of its constants are
   referenced by a strict subset of the backends. The comment describes an invariant the
   code does not hold.
2. **The recursive-lowering property is HTML-only.** `lower_recurse`
   (`crates/wcl_wdoc/src/render/lower.rs:317`) has no PDF or Markdown counterpart, so a
   user-declared `@block(...) extends WdocBlock` whose `lower` returns another custom
   variant works in the book and silently produces nothing in PDF/Markdown. The
   documented extension mechanism is therefore backend-conditional.
3. **`HtmlFundamental::Raw` is a one-way door.** It is the escape hatch every stdlib block
   reaches for (`code`, `chapter_header`, `footnotes`), and it is exactly the variant PDF
   drops and Markdown only passes through at *block* level. Content that goes through
   `Raw` is content that cannot cross backends.
4. **The three backends' differences are of three different kinds**, mixed together in one
   dispatch: *capability* differences (no theming in Markdown, no interactivity in PDF),
   *representation* differences (RGB vs CSS class, embedded bytes vs copied asset), and
   *plain omissions* (`file` in PDF, `column` outside HTML). Nothing in the code
   distinguishes the third kind from the first two.
5. **Degradation has no declaration site.** The `@except(backends=[:pdf])` axis exists
   (`crates/wcl_wdoc/lib/visibility.wcl:26-33`, `crates/wcl_wdoc/src/visibility.rs:71`)
   and is *author*-facing, per-instance. There is no *type*-level statement of which
   backends a block kind supports, so a vanished block is indistinguishable from a
   deliberate omission — and no warning is emitted (contrast the hard error for a kind
   with no `lower` at all, `crates/wcl_wdoc/src/render/lower.rs:207-220`).
6. **There is no `:skill` backend symbol.** The skill target runs as
   `Backend::Markdown` (`crates/wcl_wdoc/src/markdown/skill.rs:303`, `:414`), so
   `@only(backends=[…])` cannot distinguish a skill build from a Markdown build, even
   though the two differ in layout, front matter, link resolution, and the existence of
   `agent` output.
7. **`stub lower` is load-bearing but not typed.** 57 of 134 blocks carry a stub `lower`
   whose only purpose is to satisfy the `WdocBlock`/`SvgBlock` interface
   (`crates/wcl_wdoc/lib/core.wcl:92-98`); `lib/list.wcl:49-50` and
   `lib/terminal.wcl:72-73` say so in comments. The interface asks for a function the
   renderer will never call, and nothing in the type system marks "this kind is
   Rust-native."
8. **`p`, `text`, `h1`–`h6`, `math` are the only page blocks whose *whole* content
   survives all three backends through the fundamental layer alone.** Everything richer
   is either Rust-special-cased three times or loses parts.
9. **Stale comment**: `crates/wcl_wdoc/src/pdf/svg_embed.rs:141-142` says foreignObjects
   are converted to "a native SVG box + wrapped text", but `card_box`
   (`svg_embed.rs:291-292`) emits no text and says so.
10. **Some blocks exist *for* one backend, and the schema does not say so.**
    `frontmatter` (`crates/wcl_wdoc/lib/core.wcl:204`) is Markdown/skill-only —
    inert everywhere else (`crates/wcl_wdoc/src/render/expand.rs:287`); `agent`
    (`crates/wcl_wdoc/lib/templates.wcl:312`) and `skill`
    (`lib/templates.wcl:292`) are skill-only (`crates/wcl_wdoc/src/markdown/skill.rs:268`,
    `:370`); `markdown_source` (`lib/markdown_source.wcl:20`) is HTML-book-only;
    `class` / `stylesheet` / `menu` / `sidebar_footer` / `deck` are HTML-only. The
    `@document Site` schema (`crates/wcl_wdoc/lib/core.wcl:13-53`) presents all of them
    as peers.
11. **`include` has a fourth support level.** HTML fans includes out, the skill target fans
    them out, PDF and plain Markdown ignore them silently (§7 item 4). Since PDF/Markdown
    also share the `:markdown` visibility symbol with skill, no author-visible mechanism
    can express or predict this.
12. **`diagram` is the widest single seam and the most reliable one.** 40 of the 134
    `@block` types (30%) render through `render_shape` alone, and 13 more through
    `draw_variant`. The per-backend divergence is concentrated almost entirely in the 34
    page-content blocks.

