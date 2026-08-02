# Ticket 11 — How WAD uses the current wdoc template and block seam

Status: COMPLETE for questions 1–5, with residual gaps marked **NOT DETERMINED**.
Ticket: `.scratch/wdoc-substrate/issues/11-wad-seam-survey.md`
Method: primary sources only — `.wad/`, `docs/wskills/wad/`, `crates/wcl/src/scaffold/templates/wad.wcl`,
`crates/wcl/src/editor/{systems,blocks}.rs`, `crates/wcl_lang/src/doc{.rs,/views.rs}`,
`crates/wcl_wdoc/lib/templates.wcl`, `justfile`. CLAUDE.md was used only to *find* files; every claim
below is cited `path:line` against the file that owns it. Measurements were taken with the checked-in
`target/debug/wcl` at `ee90aa5a`.

---

## 0. Layout of the WAD, as fact

`.wad/` is 64 files (`find .wad -type f | wc -l` = 64), in three physically separate layers:

| layer | files | lines |
|---|---|---|
| schema (`.wad/schema/`) | `base.wcl` 964, `extensions.wcl` 61, `kinds.wcl` 88 | **1113** |
| data, hand-authored (`.wad/data/**` minus `generated/`) | ~30 files across 13 view folders | **1515** |
| data, extractor-owned (`.wad/data/generated/`) | 7 files (`modules.wcl` alone is 9370) | **10104** |
| projection / templates (`.wad/wdoc/`) | `book/main.wcl` 326, `book/comments.wcl` 4, 13 files under `pages/` | **1943** |

`.wad/wad.wcl:13-16` is the whole opt-in:

```
import <wdoc.wcl>
import "./schema/base.wcl"
import "./schema/extensions.wcl"
import "./data/main.wcl"
```

`.wad/wad.wcl:18` pins `schema_version = "0.6.0"`. The document's single `wad` block is
`.wad/wad.wcl:20-34`.

The build entry is `.wad/wdoc/book/main.wcl`, which imports the *root* (line 15) then the 13 page
files (lines 17–29). Its header states the architectural rule and the coupling it accepts:

> `.wad/wdoc/book/main.wcl:2-3` — "This template contains NO architecture content — only projections
> over the blocks gathered by the root @document (systems, relations, ADRs…)."

> `.wad/wdoc/book/main.wcl:8-12` — "this file holds the site + toc + the shared `let` helper layer
> (node index, name/link resolution, edge roll-up). Each view's landing page and its generated
> per-entity pages live in one file under pages/ — **those files resolve the `let`s defined here, so
> they are only valid as part of this document.**"

Recipes: `justfile:383-385` (`wad-build`), `justfile:376-377` (`wad-serve`), `justfile:389-390`
(`wad-md`), `justfile:303-305` (`wad-check`, which checks **both** `.wad/wad.wcl` and
`.wad/wdoc/book/main.wcl`). `wad-build`, `wad-check`, `wad-schema-check`, `wad-extract-check` and
`wad-facts-check` are all in the CI gate at `justfile:347`.

---

## 1. Template inventory

### 1.1 How many template pages

**32 literal `page` blocks**, across the 13 files under `.wad/wdoc/pages/` (`grep -c "^ *page "`):
articles 1, context 1, documentation 2, domain 3, externals 2, infrastructure 2, overview 3,
personas 3, specs 2, standards 2, sysadmin 2, systems 6, build 3.

Only **6 of the 32 are static landing pages**; the other 26 sit inside a `wdoc_repeater`, so the
rendered count is data-driven. In `.wad/wdoc/pages/systems.wcl` the six are `systems_page` (line 64,
static) then `system_${s.id}` (82), `container_${c.id}` (131), `component_${co.id}` (211),
`code_${ci.id}` (442), `screen_${sc.id}` (457).

**Measured: a full build writes 161 pages.** `./target/debug/wcl wdoc build .wad/wdoc/book/main.wcl
--out …` prints `wrote 161 pages`, and `find … -name '*.html' | wc -l` = 161. So the "~160 template
pages" in the Systems-view note (`crates/wcl/src/editor/systems.rs:565`, quoted below) is the
rendered page count, produced by repeater expansion from 32 authored `page` blocks.

### 1.2 The recurring shapes

`wdoc_repeater` occurrences (`grep -c wdoc_repeater`) and how many are the conditional idiom
(`grep -c "each = match"`):

| file | repeaters | of which `each = match …` |
|---|---|---|
| `.wad/wdoc/pages/systems.wcl` | 66 | 39 |
| `.wad/wdoc/book/main.wcl` | 31 | 0 |
| `.wad/wdoc/pages/build.wcl` | 27 | 19 |
| `.wad/wdoc/pages/personas.wcl` | 21 | 13 |
| `.wad/wdoc/pages/specs.wcl` | 17 | 12 |
| `.wad/wdoc/pages/infrastructure.wcl` | 16 | 10 |
| `.wad/wdoc/pages/overview.wcl` | 16 | 13 |
| `.wad/wdoc/pages/sysadmin.wcl` | 14 | 8 |
| `.wad/wdoc/pages/domain.wcl` | 12 | 7 |
| `.wad/wdoc/pages/externals.wcl` | 10 | 7 |
| `.wad/wdoc/pages/standards.wcl` | 7 | 4 |
| `.wad/wdoc/pages/context.wcl` | 6 | 2 |
| `.wad/wdoc/pages/articles.wcl` | 5 | 3 |
| `.wad/wdoc/pages/documentation.wcl` | 4 | 3 |
| **total** | **252** | **140** |

**Shape A — `wdoc_repeater` is the only conditional.** wdoc has no `if` block; `wdoc_repeater` is
Rust-special-cased in `crates/wcl_wdoc/src/render/expand.rs:76`. Every optional piece of a WAD page
is a repeater over a one-or-zero-element list. Two spellings, verbatim:

```
wdoc_repeater { each = match s.body { none => [], _ => [true] }  as = :_b
  project { from = s.body }
}
```
— `.wad/wdoc/pages/systems.wcl:91-93`

```
wdoc_repeater { each = match len(systems) { 0 => [true], _ => [] }  as = :_e
  p "_No systems recorded._"
}
wdoc_repeater { each = match len(systems) { 0 => [], _ => [true] }  as = :_t
  table { … }
}
```
— `.wad/wdoc/pages/systems.wcl:67-77` (the empty-state / non-empty pair, written twice, inverted;
same shape at `.wad/wdoc/pages/overview.wcl:34-44` and `:47-58`, `.wad/wdoc/pages/build.wcl:35-45`,
`:48-53`, `:56-67`, `:70-82`)

**140 of the 252 repeaters (56%) are this idiom, not iteration.** The loop variable is a discard,
named by convention with a leading underscore (`:_b :_e :_t :_cd :_r :_s :_al :_ar :_fl :_ex :_mg
:_db :_api :_pp :_rr :_e5 :_rt2 :_dc :_ld :_n :_g :_o …`) to signal "this is an `if`, not a `for`".

**Shape B — one page per instance, addressed by an interpolated string.**
`wdoc_repeater { each = <gather> as = :x   page $"<prefix>_${x.id}" { … } }` at
`.wad/wdoc/pages/systems.wcl:81, 130, 210, 441, 456`, `.wad/wdoc/pages/overview.wcl:90`,
`.wad/wdoc/pages/build.wcl:105, 148`, and equivalents in the other ten page files. Cross-page links
are Markdown links built from the same string convention by hand —
`$"[${s.name}](system_${s.id})"` at `.wad/wdoc/pages/systems.wcl:74`. **Page identity is a string
convention repeated in three unconnected places** (the `page` block name, the toc `chapter … page =`,
and `link_of`'s Markdown link); nothing checks they agree.

**Shape C — a shared `let` helper layer the page files depend on by name.**
`.wad/wdoc/book/main.wcl:31-179`, ~150 lines:
- `type NodeRef` + `all_nodes` — a hand-built id→(name, href, level) index over **ten** gathers
  (`.wad/wdoc/book/main.wcl:37-49`), then `node_of` (50-52), `name_of` (53), `link_of` (55-58).
- `owner_container` / `owner_system` — parent-link roll-up (63-71).
- `kind_text` — a 12-arm symbol→prose map (74-82); `rel_label` (83).
- `type RolledEdge` (86) + `dedup_edges` (89-96).
- `ctx_rolled` / `ctx_edges` (100-107), `sys_rolled` (110-121), `sys_neighbors` (125-130),
  `rels_touching` (133-139), `rel_rows` (140-144).
- `containers_of` (147), `components_of` (150), `code_items_of` (155), `code_items_of_comp` (158),
  `screens_of` (161), `screens_of_comp` (164), `stakeholder_names` (167), `articles_in` (175),
  `envs_sorted` (179).

Note that `all_nodes`/`node_of`/`link_of`/`owner_container` are a **re-derivation in interpreted WCL
of exactly the parent-link and reference model `crates/wcl/src/editor/blocks.rs:1526-1579`
(`kind_links`) derives in Rust from the schema** (§3). The two share no code and no data.

**Shape D — `wdoc_component` is the only reuse unit.** Two exist in the whole WAD:
`cli_command_view` (`.wad/wdoc/pages/systems.wcl:262`, `wdoc_slot c` at :263) and `code_item_view`
(`:314`, `wdoc_slot ci` at :315). `code_item_view` is instantiated three times — `:188` (container
page), `:239` (component page), `:447` (the code item's own page) — with the reason stated at
`:311-313`: "Instantiated on the owning component's page (the drill-in view) AND on the code item's
own page, so both stay identical."

`code_item_view` is a **five-way manual dispatch on a symbol field**, each arm a conditional
repeater: `:module_graph` (`:318`), `:db_schema` (`:331`), `:class_diagram` (`:347`), `:module_api`
(`:363`), `:api` (`:387`), each spelled
`wdoc_repeater { each = match ci.kind { :module_graph => [true], _ => [] } as = :_mg … }`.
Polymorphic rendering is expressed as five mutually-exclusive conditional repeaters in one body.

**Shape E — `edit_object` trailers.** Every per-instance page ends with one:
`.wad/wdoc/pages/systems.wcl:125` (`kind = "system"`), `:205` (`container`), `:254` (`component`),
`:451` (`code_item`), `:479` (`screen`); `.wad/wdoc/pages/overview.wcl:62` (`wad`), `:130` (`adr`);
`.wad/wdoc/pages/build.wcl:143` (`repository`), `:186` (`pipeline`). Both operands are strings:
`kind = "system"  target = $"${s.id}"`. **WAD is the surviving in-repo consumer of `edit_object`** —
CLAUDE.md records it as removed from the wskill component templates on 2026-07-19.

**Shape F — one inline stylesheet.** `.wad/wdoc/book/main.wcl:321-326`: a 2-rule
`stylesheet "wad-book"` heredoc (dashed external-card tint, metadata paragraph spacing). This is the
only raw text WAD injects into the output, and it is CSS, not HTML.

**Shape G — the toc re-walks the page tree by hand.** `.wad/wdoc/book/main.wcl:191-316` is a
125-line `toc` whose 31 `wdoc_repeater`s traverse the *same* gathers and rebuild the *same*
interpolated page names the page files emit. `.wad/wdoc/book/main.wcl:216-239` nests
systems → containers → components → code items → screens five repeaters deep, duplicating the
containment walk of `.wad/wdoc/pages/systems.wcl:81-256`. **It calls the same helpers**:
`code_items_of_comp(co.id)` appears at `.wad/wdoc/book/main.wcl:222` *and*
`.wad/wdoc/pages/systems.wcl:234` — and that duplication is measurably expensive (§1.3).

**Shape H — the templates are duplicated into the scaffold, ungated.** All 14 projection files exist
twice: live under `.wad/wdoc/`, and as heredocs in `crates/wcl/src/scaffold/templates/wad.wcl`
(`file "wdoc/book/main.wcl"`, `file "wdoc/pages/articles.wcl"` … `file "wdoc/pages/systems.wcl"` at
lines 1985, 2149, 2221, 2270, 2367, 2467, 2593, 2730, 2858, 2975, 3035, 3121 and the book entry).
Extracting each heredoc and diffing against the live copy:

| file | live | scaffold | verdict |
|---|---|---|---|
| `wdoc/book/main.wcl` | 326 | 326 | **DIFF** (1 line: a stale `--comment` flag in the live copy's header comment, line 5) |
| `wdoc/pages/build.wcl` | 188 | 159 | **DIFF** (29 lines: the live copy renders this WAD's two extension blocks — `release_triggers` at :55-67, `dev_commands` at :92-101 — and says so at :7-9) |
| `wdoc/pages/systems.wcl` | 481 | 476 | **DIFF** (7 lines: the live copy adds `routing = :straight` plus a 5-line comment at :364-369 — a CI-only elbow-routing failure fix) |
| the other 11 files | — | — | byte-identical |

**There is no drift gate on the templates.** `justfile:287-299` (`wad-schema-extract` /
`wad-schema-sync` / `wad-schema-check`) covers **only** `schema/base.wcl`. `justfile:123`
(`wskill-template-check`) and `justfile:335-343` (`wplan-template-check`) exist for the other two
scaffolds; there is no `wad-template-check`. So the `routing = :straight` bug fix has been in the
live WAD and absent from every newly-scaffolded WAD since it landed
(`4dc7b172 fix(wad): straight routing for module-API diagrams — un-break CI wad-build`), with
nothing to notice.

### 1.3 Where the ~63s cost actually sits — MEASURED

The 160/63s figure's primary source is a code comment, not a benchmark:

> `crates/wcl/src/editor/systems.rs:561-565` — "The WAD model root: the file declaring the `wad`
> block — an aggregator importing the whole data model but none of the book's templates or pages. The
> screen editor builds its synthetic unit previews from it: same content, same real-file anchors, at
> a fraction of the book entry's parse+eval cost (no ~160 template pages to evaluate)."

Measured here with `target/debug/wcl` (debug build, single run each):

| command | wall |
|---|---|
| `wcl check .wad/wad.wcl` (data model only) | **1.92 s** |
| `wcl check .wad/wdoc/book/main.wcl` (model + templates, no render) | **2.22 s** |
| `wcl wdoc markdown .wad/wdoc/book/main.wcl` (161 pages, no HTML template, no SVG) | **25.16 s** |
| `wcl wdoc build .wad/wdoc/book/main.wcl` (161 HTML pages) | **55.43 s** |

So **parse + import + schema validation is 2.2 s — 4% of the build.** Page *count* is not the
driver on its own either: 55.4 s / 161 = 344 ms per page, and the per-page work is what costs.

`wcl wdoc build --profile` records the document-evaluation call tree. Aggregating **self** time
(node total minus children) by node, 46.0 s of the 55.4 s wall is accounted for inside document
evaluation:

| self | % of accounted | calls | node |
|---|---|---|---|
| 12.77 s | 27.8% | 34 | `user_fn code_items_of_comp` |
| 9.87 s | 21.5% | 153371 | anonymous closures |
| 5.60 s | 12.2% | 26082 | `user_fn book_pageflow` |
| 4.57 s | 9.9% | 12 | `user_fn code_items_of` |
| 2.28 s | 5.0% | 509 | `user_fn owner_container` |
| 1.49 s | 3.2% | 26082 | `user_fn book_toc` |
| 1.34 s | 2.9% | 54115 | `user_fn toc_active` |
| 1.07 s | 2.3% | 2804 | `field each` |
| 1.02 s | 2.2% | 53868 | `builtin fold` |
| 1.00 s | 2.2% | 25921 | `user_fn book_toc_link` |
| 0.72 s | 1.6% | 3871 | `field id` |

Rolled up by subtree total (the two dominant depth-0 nodes are siblings, so roughly additive):

- **`wdoc_book_layout` — the built-in `book` TEMPLATE — 20.22 s over 161 calls (~126 ms/page),
  i.e. 36% of the whole build.** Its children:
  `wdoc_part_book_content` 11.95 s → `book_pagenav` 11.94 s → `book_pageflow`;
  `wdoc_part_sidebar` 8.22 s → `book_toc` 8.18 s;
  `wdoc_part_book_rail` 0.033 s; `wdoc_part_book_css` 0.009 s.
  The cause is visible in the stdlib: `book_pageflow` (`crates/wcl_wdoc/lib/templates.wcl:665-671`)
  is a recursive flatten of the whole toc tree, `book_toc` (`:636-659`) a recursive render of it, and
  `toc_active` (called per branch) another recursive fold. Both are called **fresh for every page**
  from `wdoc_part_sidebar` (`:811-830`) and `wdoc_part_book_content` (`:833-846`) via
  `wdoc_book_layout` (`:899-906`). Measured call counts confirm it: `book_pageflow` 26082 calls,
  `book_toc` 26082, `toc_active` 54115, `book_toc_link` 25921 — for 161 pages, i.e. ~162
  `book_pageflow` and ~336 `toc_active` calls *per page*, all recomputing the same tree.
  Named book-template fns account for 9.61 s of self time; most of the 9.87 s of anonymous-closure
  self time is the `map`/`fold` lambdas inside them.
- **`field each` — the repeaters — 20.55 s over 2804 evaluations.** 17.35 s of that (84%) is two WAD
  helpers: `code_items_of_comp` (12.78 s / 34 calls = **376 ms per call**) and `code_items_of`
  (4.57 s / 12 calls = **381 ms per call**). Both are one-line `filter`s:
  `.wad/wdoc/book/main.wcl:155-157` and `:158-160`. Drilling in, the `filter` builtin itself accounts
  for only **11.5 ms** of `code_items_of_comp`'s 12780 ms — the remaining 12.77 s is self time in the
  helper's own frame, and the profiler records **no `code_items` field node at all**. The
  `code_items` gather is large: `.wad/data/generated/modules.wcl` declares 74 `code_item` blocks
  containing 434 `code_node`s and 1416 `code_member` rows.
  **NOT DETERMINED:** the exact internal cause of that 376 ms/call. The profile localises it to
  resolving/materialising the `code_items` gather inside the fn body (not to `filter`, not to any
  instrumented field), but confirming whether the gather is re-materialised per call or the closure
  capture is the cost needs a native profiler (`perf`/`samply`) on the evaluator, which was not run.
- WAD-authored helper fns total **20.51 s** of self time; the built-in book template's named fns
  **9.61 s**.
- The unaccounted **~9.4 s** (55.4 − 46.0) is the Rust render side: diagram layout/routing, syntax
  highlighting, HTML emission, IO. Consistent with the markdown build (25.2 s) sitting ~30 s below
  the HTML build, since Markdown runs neither the book template nor SVG layout.

**Summary answer to the ticket's question:** the cost is *not* page count and *not* parse/validate
(2.2 s). It is roughly (a) 20 s of built-in book *template* re-evaluation — the toc and pagenav
tree re-walked from scratch, recursively, in interpreted WCL, once per page — and (b) 20 s of
interpreted gather filtering inside repeaters, ~17 s of which is two `filter(code_items, …)` calls
made once per component *and again* from the toc.

---

## 2. What WAD asks of `TemplateCtx`

**Finding: WAD asks nothing of it directly. It does not touch the `TemplateCtx` seam at all.**

- `grep -rn "TemplateCtx" --include=*.wcl .` returns **zero hits under `.wad/`** and zero under
  `docs/wskills/wad/`. The in-repo `TemplateCtx` consumers are
  `crates/wcl/src/scaffold/templates/website.wcl:69`, `…/wskill-registry.wcl:487`,
  `…/wskill.wcl:2956,2992`, `docs/pages/reference/wdoc/websites.wcl:26`,
  `docs/pages/reference/wdoc/sites.wcl:170,191`, `docs/wskills/wdoc/wdoc/training/main.wcl:398,434`,
  `examples/wdoc_template.wcl:21,30`, `examples/wdoc_website.wcl:36`. WAD is absent from that list.
- WAD declares **no `template` block**. Its entire template interaction is one word:
  `default_template = :book` at `.wad/wdoc/book/main.wcl:183`.
- **No `region` blocks, no `wdoc_region(…)` call, no `page_region`.** `grep -rn "region" .wad/
  --include=*.wcl` hits only `.wad/schema/kinds.wcl:43` (the word "region" inside the `InfraKind`
  symbol vocabulary: `cloud region network vpc cluster …`) and a release-note string at
  `.wad/data/generated/releases.wcl:115`. The unchecked-string `Region { name: utf8  content: utf8 }`
  seam (`crates/wcl_wdoc/lib/templates.wcl:25`) is **not exercised by WAD**.
- **No `HtmlFundamental`, no `HtmlFundamental::Raw`, no `Head`** anywhere in `.wad/`.

What WAD *supplies* to the ctx is the `site` block's fields, `.wad/wdoc/book/main.wcl:182-317`:
`default_template`, `title`, `summary`, `root`, `theme`, `accent`, `theme_toggle`, `search`, `toc`.
It declares no `menu`, no `sidebar_footer`, no `deck` and no regions.

Which ctx fields therefore matter to WAD, via the built-in `book` chain
(`crates/wcl_wdoc/lib/templates.wcl:899-906` → `:811-830`, `:833-846`, `:868-891`):

| ctx field | declared at | exercised by WAD |
|---|---|---|
| `content: utf8` | `templates.wcl:78` | **yes** — spliced by `HtmlFundamental::Raw { html: c.content }` at `templates.wcl:838` |
| `title` | `:80` | yes (`.wad/…/main.wcl:184`) |
| `toc: list<TocEntry>` | `:86` | **yes, heavily** (`.wad/…/main.wcl:191-316`; the §1.3 hot path) |
| `on_this_page` | `:88` | yes (implicit, from `h2`/`h3`) |
| `theme_toggle` | `:94` | yes (`:188`) |
| `search` | `:96` | yes (`:189`) |
| `home_href` / `home_title` | `:98`, `:100` | `root = true` (`:186`), so both empty |
| `regions: list<Region>` | `:84` | **no** — always empty |
| `menu`, `footer`, `deck` | `:90`, `:92`, `:92`+ | **no** — always empty |
| `pages`, `page_name` | `:82`, `:81` | not read by the book chain |

**Where WAD works around content-as-`utf8`: nowhere — it never sees it.** The `Raw { html:
c.content }` splice is done for it by the stdlib book template at
`crates/wcl_wdoc/lib/templates.wcl:838`. WAD's coupling to the template layer is exactly two things:
the `:book` symbol, and the `toc { chapter … page = <name> }` string contract.

What WAD *does* stress is the **block/page layer below the template**: 252 repeaters,
`project { from = … }` for prose bodies (`crates/wcl_wdoc/src/render/expand.rs:315`),
`wdoc_component` + `wdoc_slot` for reuse, `edit_object`, and interpolated string page names.

---

## 3. Schema-derived rendering — the Systems view (highest-priority answer)

The Systems view derives the entire WAD model from schema introspection. Its own module docs state
the contract:

> `crates/wcl/src/editor/systems.rs:6-21` — "Nothing about the WAD is hardcoded here — containment is
> derived from the schema: a **parent link** is a scalar `identifier` field whose NAME is another
> gathered kind's name (`component.container`, `container.system`, `system.boundary`,
> `screen.component`), plus `parent` for self-nesting (`infra_node.parent`). Declaration order is the
> preference order… a **reference** is any other `identifier` / `list<identifier>` field… an **edge
> kind** is a gathered kind carrying both a `source` and a `destination` identifier field."

### 3.1 The derivation core: `kind_links`

`crates/wcl/src/editor/blocks.rs:1526-1579`. Signature
`pub(super) fn kind_links<'a>(doc: &'a Document) -> Vec<KindInfo<'a>>`; `KindInfo` is declared at
`blocks.rs:1500-1509` with fields `kind`, `schema: wcl_lang::TypeDecl<'a>`,
`parents: Vec<(String,String)>`, `refs: Vec<(String,bool)>`, `edge: Option<(String,String)>`.

The algorithm, line by line:

1. `blocks.rs:1527` — `gathered_kinds(doc)`, then `:1528` snapshots the kind **names**. Those names
   are the entire vocabulary the parent-link rule matches against.
2. `blocks.rs:1536` — `for f in schema.effective_fields()`.
3. `blocks.rs:1537` — skip non-scalars via `is_scalar` (`blocks.rs:1519-1523`), which is
   `f.child_kind_or_union().is_none() && f.children_kind_or_union().is_none() &&
   f.connection_schema().is_none()`.
4. `blocks.rs:1541` — `bare_type(&f)` (`blocks.rs:1512-1515`) = `f.type_ref().to_string()` with a
   trailing `?` stripped. **The classification is a string comparison on the printed type**:
   `blocks.rs:1542` `if ty == "identifier"`, `blocks.rs:1558` `else if ty == "list<identifier>"`.
5. `blocks.rs:1543-1547` — a field literally *named* `source` / `destination` and typed `identifier`
   marks the edge candidate.
6. `blocks.rs:1548-1550` — `f.inline_slot().is_some()` ⇒ `continue`: the `@inline(0) id: identifier`
   slot names the block itself and is never a link.
7. `blocks.rs:1551-1557` — the parent rule: name `== "parent"` ⇒ self-nesting parent; else
   `names.contains(&name)` ⇒ parent link to the kind of that name; else it is a `ref`.
8. `blocks.rs:1562-1569` — `edge` is `Some` only when both `source` and `destination` were found, and
   then those two fields are removed from `parents`/`refs`.

`gathered_kinds` (`blocks.rs:1442-1467`) is the other half:

- `blocks.rs:1445` — `doc.type_decls()`.
- `blocks.rs:1446` — keep only decls carrying `@document` (`decl.decorators().any(|d| d.name() ==
  "document")`). This is what makes the merged-`@document` composition load-bearing: WAD's
  `WadDoc` (`.wad/schema/base.wcl:933-964`) *and* its extension `WclWadExtensions`
  (`.wad/schema/extensions.wcl:57-61`) both contribute.
- `blocks.rs:1449` — `decl.effective_fields()`.
- `blocks.rs:1450` — `field.children_block_kind()` — only `@children("kind")` gathers count.
- `blocks.rs:1457` — `gather_elem_decl(&field).or_else(|| doc.block_schema(&kind))`.
- `blocks.rs:1460` — drop anything whose `schema.full_name()` starts with `"wdoc."`, i.e. exclude
  wdoc's own infrastructure gathers by declaring namespace.

`gather_elem_decl` (`blocks.rs:1474-1485`) is the namespace-correctness fix, and its doc comment
names the exact hazard:

> `blocks.rs:1469-1473` — "The type a gather field's element names, resolved through the field's own
> declared type. Namespace-correct where a bare `Document::block_schema` name lookup is not: a WAD's
> `wcl.wad.Container` and wdoc's diagram `container` shape share a *kind* name, and the name lookup
> answers whichever happens to be declared first."

Its body is `named(field.resolved_type())`, unwrapping `ResolvedType::List` / `::Reference` to reach
`ResolvedType::Named(d)` (`blocks.rs:1477-1484`).

### 3.2 The language/schema features this depends on, with call sites

| feature | declared at | used by WAD's Systems view at |
|---|---|---|
| `Document::type_decls()` | `crates/wcl_lang/src/doc.rs:1433-1462` | `blocks.rs:1445` (`gathered_kinds`), `blocks.rs:1607` (`diagram_kinds`) |
| `TypeDecl::decorators()` / `@document`, `@block` | — | `blocks.rs:1446`, `blocks.rs:1608-1612` |
| `TypeDecl::effective_fields()` | `crates/wcl_lang/src/doc/views.rs:1099-1101` (delegates to `build_effective_fields` over the transitive `extends` chain) | `blocks.rs:1449` (gathers), `blocks.rs:1536` (**the link derivation**), `blocks.rs:1633` (`kind_entry` form fields), `systems.rs:293` (`child_families`), `systems.rs:342` (`body_block`), `systems.rs:399` (`suggestions`), `systems.rs:552` (`id_field`) |
| `TypeField::resolved_type()` | `views.rs:1327-1329`, body = `self.doc.resolve_in(&self.ast.ty, self.file_ns)` | `blocks.rs:1484` (`gather_elem_decl` — **namespace-correct gather element**), `blocks.rs:1655` (symbol-set enumeration for form selects) |
| `Document::resolve_in()` | `crates/wcl_lang/src/doc.rs:1570-1606`; rationale at `doc.rs:1563-1569` ("a WAD's `wcl.wad.Container` and wdoc's diagram `wdoc.Container` are both named `Container`") | reached only *through* `TypeField::resolved_type` (`views.rs:1328`) — the Systems view never calls it directly |
| `ResolvedType::{Named, List, Reference, SymbolSet}` | `crates/wcl_lang/src/doc.rs:1571-1605` | `blocks.rs:1477-1484`, `blocks.rs:1655-1660` |
| `TypeField::type_ref()` → `to_string()` | `views.rs:1482` | `blocks.rs:1513` (`bare_type`) and thence the `== "identifier"` / `== "list<identifier>"` tests at `blocks.rs:1542, 1558`; also the `starts_with("fn")` filter at `blocks.rs:1650` |
| `TypeField::children_block_kind()` | `views.rs:1369-1374` (returns `None` for the union form) | `blocks.rs:1450`, `systems.rs:295`, `systems.rs:344` |
| `TypeField::child_block_kind()` | `views.rs:1359-1364` | `systems.rs:294`, `systems.rs:343`, `blocks.rs:1634` (`body` detection) |
| `TypeField::child_kind_or_union()` / `children_kind_or_union()` | `views.rs:1379-1392` | `blocks.rs:1520-1521` (`is_scalar`), `blocks.rs:1637-1643` (`accepts_children`) |
| `TypeField::connection_schema()` | `views.rs:1397-1415` | `blocks.rs:1522` (`is_scalar`), `blocks.rs:1643` |
| `TypeField::inline_slot()` | `views.rs:1333-1336` (`@inline(N)`) | `blocks.rs:1548` (**id slot excluded from links**), `systems.rs:401` (suggestions), `systems.rs:554` (`id_field`) |
| `TypeField::default_value()`, `doc_comment()`, `optional()`, `name()` | `views.rs:1344-1352`, `:1318`, — | `blocks.rs:1661-1669` (generated forms) |
| `TypeDecl::full_name()` | `views.rs:584` | `blocks.rs:1460` (namespace exclusion), `systems.rs:252`, `systems.rs:548` (echoed as `type_name` so a cross-namespace kind name resolves on create) |
| `TypeDecl::namespace()` | — (used at `blocks.rs:1121-1128`) | `kin_file`'s same-namespace fallback for file placement |
| `TypeDecl::is_descendant_of()` | `views.rs:1125-1134` (transitive `extends` walk) | `blocks.rs:1615` — `!decl.is_descendant_of("wdoc.SvgBlock")` in `diagram_kinds`. **Not used by the Systems C4 model itself**; it gates the diagram-shape palette that the Systems view's wireframe **Screen** surface editor consumes (`diagram_kinds` is served on `/api/palette`, `blocks.rs:1604-1623`) |
| `Document::block_schema()` | `crates/wcl_lang/src/doc.rs:1759` | `blocks.rs:1457` and `systems.rs:310` — the **fallback** when `gather_elem_decl` cannot resolve; this is the namespace-ambiguous path, kept only as a last resort |
| `Document::blocks()` / `blocks_with_source()` | — | `systems.rs:397` (suggestions), `systems.rs:451` (**the model walk**), `systems.rs:203`, `:210`, `systems.rs:567` (`model_entry`), `blocks.rs:1046`, `blocks.rs:1413` (`is_wad`) |

### 3.3 What the derivation produces from the actual WAD schema

The gathers are `.wad/schema/base.wcl:933-964` — 28 `@children` fields on `@document type WadDoc` —
merged with `.wad/schema/extensions.wcl:57-61` (`dev_commands`, `release_triggers`). The comment
above them records the merged-namespace hazard that forced `sw_components`:

> `.wad/schema/base.wcl:929-932` — "@document, so they must not reuse its field names (pages,
> templates, components, sites, bodies, …). That is why component instances gather as
> `sw_components` — bare `components` is wdoc's wdoc_component list, and the collision breaks
> template iteration."

Parent links the rule finds in the base schema (each is a scalar `identifier` field whose name equals
a gathered kind name):
- `System.boundary: identifier?` — `.wad/schema/base.wcl:260`
- `Container.system: identifier` — `.wad/schema/base.wcl:277`
- `Component.container: identifier` — `.wad/schema/base.wcl:294`
- `InfraNode.parent: identifier?` — `.wad/schema/base.wcl:518` (the `parent` special case,
  `blocks.rs:1551`)
- `DeployTarget.{container, environment, infra_node}` — `.wad/schema/base.wcl:534, 536, 538`; all
  three are parent links, and `systems.rs:497-504` records **every** parent field the instance sets
  so the canvas can nest it differently per perspective (`systems.rs:492-496`).

References the rule finds: `System.owner: identifier?` (`base.wcl:262` — `owner` is not a kind name),
`System.repos: list<identifier>` (`:263`), `Container.repo: identifier?` (`:283`),
`InfraNode.environments: list<identifier>` (`:521`).

Edge kind: `relation` — `.wad/schema/base.wcl:158-179` carries both `source` and `destination`
`identifier` fields, so `blocks.rs:1562` marks it `edge` and `blocks.rs:1567-1568` strips those two
from `parents`/`refs`.

### 3.4 The one curated (non-derived) part

`PERSPECTIVES` at `crates/wcl/src/editor/systems.rs:67-79` — three hardcoded seed sets
(`systems` = `["boundary","system","external_system"]`, `personas` = `["persona"]`,
`deployment` = `["environment","infra_node","deploy_target"]`), closed transitively over the derived
parent links by `perspectives()` (`systems.rs:85-125`), with seeds treated as exclusive
(`systems.rs:106`). `TITLE_FIELDS` (`systems.rs:53`) is a second small curated list:
`["name","title","term","version","activity","path"]`, tried in order at `systems.rs:510-513`.

`suggestions()` (`systems.rs:392-422`) is derived, not curated: a `utf8`/`ascii` scalar whose values
repeat across instances (`systems.rs:415`, capped at `MAX_SUGGESTIONS = 40`, `systems.rs:382`)
becomes a picker. `MAX_CHILD_DEPTH = 4` at `systems.rs:272`.

### 3.5 Exposure to a type-system refactor — the load-bearing assumptions

Read off the code, the Systems view will break if any of these change:

1. **`bare_type(f) == "identifier"` / `== "list<identifier>"`** (`blocks.rs:1542, 1558`) — the whole
   model is classified by *string-matching a printed type name*, via `TypeRef::to_string()`
   (`views.rs:1482`, `blocks.rs:1513`). Any change to how `identifier` or `list<T>` prints silently
   reclassifies every parent link and reference as neither.
2. **`ty.to_string().starts_with("fn")`** (`blocks.rs:1650`) — how fn-typed fields are excluded from
   generated forms; same fragility.
3. **Parent links keyed on `field name == gathered kind name`** (`blocks.rs:1553`). This couples the
   *field naming* of the schema to the *gather kind* vocabulary. It is why `Container.system` works
   and why a rename either side breaks containment with no error.
4. **`@children("kind")` string form only** — `children_block_kind()` returns `None` for the union
   form (`views.rs:1369-1374`), so a schema that gathered a union instead of a named kind would
   vanish from `gathered_kinds` entirely.
5. **`@document` merge semantics** (`blocks.rs:1446` iterating *all* `@document` decls) — the
   extension gathers only appear because merged `@document`s compose.
6. **Namespace exclusion by `full_name().starts_with("wdoc.")`** (`blocks.rs:1460`) — a string prefix
   test on the stdlib's namespace name.
7. **`resolved_type()` → `resolve_in` namespace-first resolution** (`views.rs:1327-1329`,
   `doc.rs:1570`) — without it, a WAD `container` gather resolves to wdoc's diagram `Container`
   shape. The fallback `doc.block_schema(&kind)` (`blocks.rs:1457`, `systems.rs:310`) is explicitly
   the wrong-answer path.
8. **`is_descendant_of("wdoc.SvgBlock")`** (`blocks.rs:1615`) — a fully-qualified *string* naming a
   stdlib type, gating the widget palette the Screen surface editor depends on. Renaming or
   re-parenting `SvgBlock` silently empties `diagram_kinds`.
9. **`inline_slot() == Some(0)` as "this is the id"** (`blocks.rs:1548`, `systems.rs:554`).

There is a unit test guarding the derivation rules. `crates/wcl/src/editor/systems.rs:638-640`:
"A miniature schema exercising every derivation rule: a three-level containment chain, a
two-candidate parent, a self-parent, a plain reference and an edge kind." The fixture schema is a
`const SCHEMA: &str` starting at `systems.rs:641`, declaring `@block("zone")`, `@block("system")`
with `zone: identifier?` (parent link) and `repo: identifier?` (reference), plus `@block("wparam")` /
`@block("wendpoint")` for the two-level recursive child payload (`systems.rs:643-660`). I did not
read the assertion bodies past line 660.

---

## 4. The extractor boundary

### 4.1 What exists

Six extractors plus a README: `.wad/scripts/extract_cargo.py`, `extract_ci.py`,
`extract_http_api.py`, `extract_justfile.py`, `extract_modules.py`, `extract_releases.py`,
`.wad/scripts/README.md`.

Run by `justfile:394-395`:
```
wad-extract: && wad-check
    for s in .wad/scripts/extract_*.py; do echo "==> $s" >&2; uv run "$s"; done
```

Output lands in `.wad/data/generated/`: `cargo.wcl` 39, `ci.wcl` 50, `http_api.wcl` 40,
`justfile.wcl` 311, `modules.wcl` 9370, `releases.wcl` 279, `main.wcl` 15 — **10104 lines, 87% of
all WAD data.**

### 4.2 The contract, as written

`.wad/scripts/README.md:17-59` states eight conventions. The load-bearing ones:

- `README.md:32-34` — "**One script, one output file.** `extract_cargo.py` owns
  `data/generated/cargo.wcl` — nothing else writes it… Re-running is always a **full overwrite**,
  never an append or merge."
- `README.md:36-41` — "**A generated banner, always first**" (`// GENERATED by … — do not hand-edit`).
  Verified: `.wad/data/generated/modules.wcl:1`, `.wad/data/generated/justfile.wcl:1`.
- `README.md:43-45` — "**Deterministic output.** No timestamps; sort every collection. A re-run with
  an unchanged source must be byte-identical."
- `README.md:47-49` — "**Stable ids derived from source names**… Hand-authored data may *reference*
  generated ids but must never reuse them."
- `README.md:51-52` — "**Empty result still writes the file** (banner + `namespace` line), so the
  committed import line never dangles."
- `README.md:54-55` — "**Output must pass `wcl check`.**"
- `README.md:57-59` — "**Generated files are committed.**"

The same rules are restated inside the data tree at `.wad/data/generated/main.wcl:1-6`
("SCRIPT-OWNED zone… Never hand-edit anything under generated/; fix the extractor and re-run").

CI enforces staleness at `justfile:311-318` (`wad-extract-check`): re-run everything, require a quiet
`git status` over `.wad/data/generated`, **with `releases.wcl` exempt** because it is git-tag-derived
and the release tag is created after the commit CI builds from (`justfile:307-310`, `:317-318`).

### 4.3 What the contract actually is, at the seam

**The contract is the schema, and nothing else.** Concretely, the interface between an extractor and
a template is: the extractor writes `namespace wcl.wad` plus block instances of kinds declared in
`.wad/schema/{base,extensions}.wcl`; `.wad/data/generated/main.wcl:9-14` imports each file; the root
`@document` gathers them; templates iterate the gather.

- `extract_modules.py` emits `code_item` blocks (74 of them) with nested `code_node` / `code_member`
  children — `.wad/data/generated/modules.wcl:6-`, matching `@block("code_item")`
  `.wad/schema/base.wcl:475-489` and `@block("code_node")` `:380-392`.
- `extract_justfile.py` emits `dev_command` blocks matching the *extension* type
  `.wad/schema/extensions.wcl:28-39`, gathered by `.wad/schema/extensions.wcl:59`. Its declaration
  comment names its owner: `.wad/schema/extensions.wcl:25-27` — "EXTRACTOR-OWNED — emitted by
  scripts/extract_justfile.py into data/generated/justfile.wcl".
- The templates read those gathers by name and nothing else: `dev_commands` at
  `.wad/wdoc/pages/build.wcl:92, 97`; `code_items` via `code_items_of` / `code_items_of_comp`
  (`.wad/wdoc/book/main.wcl:155-160`); `releases` at `.wad/wdoc/pages/build.wcl:70, 73, 77`.

**Is the extractor boundary touched by a template/type refactor?** Established facts:

- **No extractor emits template constructs.** Nothing under `.wad/scripts/` or
  `.wad/data/generated/` mentions `page`, `wdoc_repeater`, `TemplateCtx`, `region` or
  `HtmlFundamental` (the only "region" hit in the whole of `.wad/` is the `InfraKind` symbol at
  `.wad/schema/kinds.wcl:43`). Extractors emit *data blocks only*.
- Therefore the extractor↔template coupling is **indirect and one-directional**: extractors depend on
  the `@block` type declarations and the `@children` gather names; templates depend on the same. A
  refactor that leaves `@block` / `@children` / `namespace` semantics and the printed field types
  intact does not touch the extractors. One that changes them changes 10104 lines of generated data
  and six Python emitters at once — and `justfile:311-318` will fail the build until every emitter is
  updated.
- One extra, concrete coupling worth recording: `crates/wcl/src/editor/blocks.rs:1098-1114`
  (`is_generated`) reads the `GENERATED` banner from the first five comment lines and makes
  `place_unit` (`blocks.rs:1039-1091`, specifically `:1054`) refuse to create new objects in
  extractor-owned files. So the banner convention is not only documentation — it is parsed by the
  editor.

**NOT DETERMINED:** whether any extractor's *output shape* (as opposed to its block kinds) is tuned
to how a template renders it. I read `.wad/scripts/README.md` in full and the heads of
`modules.wcl` / `justfile.wcl`, but did not read the six Python sources line by line. To settle it,
read each `extract_*.py`'s emit section and check for ordering/grouping choices that only matter to a
specific template (e.g. `COMPONENT_PREFIXES`, mentioned at `.wad/scripts/README.md:13`, groups module
APIs onto components — that grouping is visible in `.wad/wdoc/pages/systems.wcl:234`'s per-component
iteration, which *suggests* such a coupling exists, but I did not verify the direction).

---

## 5. Where WAD hurts today, read off the code

All of these are about WAD's use of the template/block seam.

1. **56% of repeaters are `if` statements in disguise** — 140 of 252 (§1.2), each with a discard loop
   variable, because there is no conditional block. Concentrated where the data is richest:
   `.wad/wdoc/pages/systems.wcl` has 39, `build.wcl` 19.
2. **The empty-state pattern is written twice, inverted.** `{0 => [true], _ => []}` for the
   "_No X recorded._" paragraph then `{0 => [], _ => [true]}` for the table — e.g.
   `.wad/wdoc/pages/systems.wcl:67-77`, `.wad/wdoc/pages/overview.wcl:34-44` and `:47-58`,
   `.wad/wdoc/pages/build.wcl:35-45`, `:48-53`, `:56-67`, `:70-82`.
3. **Page identity is an unchecked string convention in three places** — the interpolated `page`
   name, the toc's `page =`, and the Markdown link `link_of` builds
   (`.wad/wdoc/book/main.wcl:55-58`). A typo in any one is a dead link, not an error.
4. **The toc duplicates the page tree's containment walk** — `.wad/wdoc/book/main.wcl:216-239` (five
   nested repeaters) versus `.wad/wdoc/pages/systems.wcl:81-256`. Both call
   `code_items_of_comp(co.id)` (`main.wcl:222`, `systems.wcl:234`), which §1.3 measures at 376 ms per
   call — so the duplication is not merely cosmetic, it is a measurable share of a 55 s build.
5. **`all_nodes` hand-maintains an id index over ten gathers**
   (`.wad/wdoc/book/main.wcl:37-49`). Adding a kind to the schema means editing this literal list,
   and the fallback for an unknown id is silent: `node_of` returns
   `{ id: nid, name: $"${nid}", href: "", level: "unknown" }` (`main.wcl:51`) and `link_of` then
   degrades to a bare name (`:55-58`) rather than reporting a dangling reference.
6. **Union dispatch is five conditional repeaters in one component body** — `code_item_view`,
   `.wad/wdoc/pages/systems.wcl:314-438`. `CodeItem` carries three mutually-exclusive child families
   for this reason (`nodes` / `tables` / `endpoints`, `.wad/schema/base.wcl:485-487`), plus a `kind`
   symbol that the template must switch on manually.
7. **The page files are not standalone documents.** `.wad/wdoc/book/main.wcl:10-12` — they "are only
   valid as part of this document" because they resolve the entry's `let`s. So a page file cannot be
   checked, previewed or rebuilt in isolation, and the editor's targeted rebuild has to go through
   the book entry (which is precisely what `model_entry`, `systems.rs:561-573`, exists to route
   *around*).
8. **The built-in template is the single largest cost in the build** — 20.22 s of 55.4 s inside
   `wdoc_book_layout`, recomputing the same toc tree 161 times (§1.3). WAD cannot influence this: it
   selects `:book` with one symbol and has no way to hoist or memoise the chrome.
9. **Interpreted gather filtering is the other half** — `code_items_of_comp` at 376 ms/call ×34 and
   `code_items_of` at 381 ms/call ×12 (§1.3), over a 74-instance gather backed by 9370 generated
   lines. Every WAD helper of the form `filter(<gather>, fn(x) -> bool { x.<parent> == id })` is an
   O(n) scan re-run per repeater evaluation; there is no index and nothing in the language to build
   one. `owner_container` (`main.wcl:63-66`) alone costs 2.28 s over 509 calls.
10. **Templates are duplicated into the scaffold heredoc with no drift gate** (§1.2 Shape H). Three
    of 14 files have drifted; two of those drifts are real fixes/features stranded on one side. There
    is a `wad-schema-check` (`justfile:295-299`) but no `wad-template-check`, while both the wskill
    and wplan scaffolds have one (`justfile:123`, `:335`).
11. **Symbol→prose mapping is hand-written in the template** — `kind_text`
    (`.wad/wdoc/book/main.wcl:74-82`) maps 12 `RelationKind` symbols to English with a `_ =>
    "relates to"` fallback; `adr_status_text` and `adr_status_class`
    (`.wad/wdoc/pages/overview.wcl:6-12`) do the same for `AdrStatus`. Adding a symbol to
    `.wad/schema/kinds.wcl` silently falls through to the default.
12. **`edit_object` is addressed by stringly-typed `(kind, target)` pairs** — nine sites (§1.2 Shape
    E), each repeating a kind name as a string literal already known to the schema.

---

## Findings about WAD's own content model — OUT OF SCOPE for this effort

Flagged separately per the ticket's scope line. These are about *what WAD models*, not about its use
of the wdoc template/block seam, and belong to a later effort.

- **Data volume is 87% extractor-owned.** 10104 generated lines vs 1515 hand-authored. Whether the
  hand-authored architecture content is thin relative to the machine-extracted detail is a content
  question.
- **Prose that is arguably data.** `.wad/schema/base.wcl:129` and 12 further sites attach
  `@child("body") body: WdocAddressableBody?` to nearly every block kind; `Adr` additionally carries
  free-text `context` / `decision` / `consequences` (`.wad/wdoc/pages/overview.wcl:97-118` renders
  each as a single `p`). Whether those should be typed is a WAD content-model question.
- **`article` as the escape hatch.** `.wad/schema/extensions.wcl:12-14` names "an `article` block …
  needs NOTHING here — file a freeform prose page under any view" as option 1 for custom content;
  `.wad/wdoc/book/main.wcl:198-313` threads `articles_in(<section>)` into all twelve chapters. Every
  view therefore has a prose bucket beside its typed data.
- **The `wad` wskill's own template layer is a different consumer.** `docs/wskills/wad/` is a
  *wskill* (topic `wad`, `docs/wskills/wad/wskill.wcl:34`), with artifacts
  `book` → `wdoc/book/main.wcl` and `ai_skill` → `wdoc/skill/main.wcl`
  (`docs/wskills/wad/wskill.wcl:61-62`) and eight component templates under
  `docs/wskills/wad/wdoc/component/`. It is documentation *about* WAD, not a WAD, and it likewise
  contains no `TemplateCtx` / `region` usage (`grep -rln` returns nothing). Its unit counts:
  11 concepts, 6 entities, 17 facts, 2 checklists in `data/reference/`, plus a `wplan`/`wissue`/
  `wbuild` set (20 + 4 + 3). Its own graph shape is a wskill question, not a WAD-seam one.
- **Twelve fixed chapters as a content decision.** `docs/wskills/wad/data/reference/concepts.wcl:17`
  — "The fixed chapter set of every WAD book, and which block families feed each." The chapter set is
  hardcoded in the toc (`.wad/wdoc/book/main.wcl:191-316`).
- **`wad-facts-check` gates hand-reflected schema tables.** `justfile:322-325` requires
  `docs/wskills/wad/data/reference/facts.wcl` to carry the string
  `"hand-reflected from schema <version>"` matching the scaffold's `schema_version`. The fact tables
  themselves are hand-maintained duplicates of the schema (e.g.
  `docs/wskills/wad/data/reference/facts.wcl:327` restates the `InfraKind` vocabulary). That is a
  wskill/WAD documentation-duplication issue, not a substrate one.

---

## Observations for the design discussion

Strictly separated from the findings above; no proposals, just the implications I noticed.

1. **WAD does not currently validate the `TemplateCtx`/region half of the substrate at all.** It
   never declares a template, never reads a ctx field, never uses a region. If WAD is meant to *prove*
   a typed slot contract, that will be new surface for it, not a migration of existing usage. What WAD
   validates today is the layer *below*: repeaters, `project`, `wdoc_component`/`wdoc_slot`, string
   page identity, and `edit_object`.
2. **The measured cost lands on the template layer, which is the thing being replaced.** 20.2 s of a
   55.4 s build is inside `wdoc_book_layout` re-deriving the toc and pagenav per page in interpreted
   WCL. Whatever replaces the template layer inherits this as its headline benchmark.
3. **A second, independent 20 s sits in interpreted gather filtering** and is *not* a template
   problem — it is `filter(<gather>, …)` re-scanning per repeater evaluation. A template refactor
   alone would leave roughly half the build cost untouched.
4. **The schema-derived Systems view is coupled to the type system by printed strings, not by types.**
   `bare_type(f) == "identifier"`, `to_string().starts_with("fn")`,
   `full_name().starts_with("wdoc.")`, `is_descendant_of("wdoc.SvgBlock")`. Any type-system refactor
   needs these enumerated as an explicit compatibility surface; several fail *silently* (a
   reclassified field simply stops being a parent link).
5. **`gather_elem_decl` vs `block_schema` is a live namespace hazard, already bitten once.** The
   comments at `blocks.rs:1469-1473` and `doc.rs:1563-1569` both cite the same WAD `container` vs wdoc
   `Container` collision, and `.wad/schema/base.wcl:929-932` records the `sw_components` rename forced
   by the merged-`@document` gather namespace. A refactor that unifies or flattens namespaces re-opens
   all three.
6. **Template↔scaffold duplication is unguarded and already drifted.** Any migration doubles the work
   (live WAD + scaffold heredoc) unless the duplication is addressed, and the existing 3-of-14 drift
   means the two sides are not currently a single artifact.
7. **The extractor boundary looks refactor-neutral** — extractors emit only data blocks against
   `@block`/`@children` — but 10104 committed lines and a CI byte-identity gate
   (`justfile:311-318`) make any schema-shape change a synchronised six-emitter change.

---

## Residual gaps (NOT DETERMINED)

- **The internal cause of `code_items_of_comp`'s 376 ms/call.** Profile localises it to self time in
  the fn frame (not `filter`, 11.5 ms of 12780 ms; no instrumented `code_items` node exists). Needs a
  native profiler on the evaluator.
- **Whether any extractor's output shape is tuned to a specific template.** `COMPONENT_PREFIXES`
  (`.wad/scripts/README.md:13`) is suggestive; I did not read the six Python sources line by line.
- **The body of the `systems.rs` derivation unit test** (`systems.rs:634+`). Its existence and stated
  coverage are recorded; I did not read the assertions.
- **Whether the 63 s in the Systems-view note was measured on the same machine/build as my 55.4 s.**
  Mine is a single debug run at `ee90aa5a`; the 63 s figure is an uncited code comment
  (`systems.rs:565` says only "~160 template pages", the 63 s appears in CLAUDE.md). Treat 55.4 s as
  the measurement and 63 s as consistent-but-unverified.
- **`wcl check` may not force every lazy field**, so the 1.92 s / 2.22 s figures bound
  parse+import+validate, not full evaluation. The `--profile` numbers are the reliable eval
  measurement.
