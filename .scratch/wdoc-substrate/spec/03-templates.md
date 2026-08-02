# 03 — The templating layer

Source: tickets [01](../issues/01-content-seam.md), [02](../issues/02-template-authoring.md),
[03](../issues/03-slot-contract.md), [12](../issues/12-template-selection.md), with the performance
measurement from [11](../issues/11-wad-seam-survey.md).

## The problem, measured

- **`TemplateCtx.content: utf8`** — the page body reaches a template as opaque pre-rendered HTML.
  `Region { name: utf8, content: utf8 }` likewise, keyed by **unchecked string name**.
  `wdoc_region(c, "heor")` returns `""`. Silently. Forever.
- A layout **cannot declare** the slots it needs; a page cannot be validated against them.
- **No template today inspects content at all.** Across all four templates `c.content` appears exactly
  **three** times (`templates.wcl:570,840`, `website.wcl:213`) and every one is
  `HtmlFundamental::Raw { html: c.content }` — pure paste. The only inspection anywhere is *emptiness*
  tests on regions.
- **The opacity hurts the renderer, not the template.** It works around it two ways: pre-extraction
  into `TemplateCtx` fields (`render/headings.rs`), and **string-searching its own output** —
  `first_h1_text` (`build.rs:2267`) does `html.find("heading-1")`, finds `>`, finds `</p>`, and
  converts HTML back to text to recover a heading the renderer just produced.
- **Regions are all but unused.** **7** `region "…"` blocks in the entire repo — 4 names across 3 files.
  Only the `website` template consumes them; book, presentation, webpage, WAD and every wskill
  projection use **zero**.
- **The book template is 36% of a build.** WAD: 32 authored `page` blocks → 161 rendered pages; full
  HTML build 55.4s debug, of which **20.22s is inside `wdoc_book_layout`** — called once per page,
  recursively re-walking the whole toc each time. `book_pageflow` **26082 calls** for 161 pages;
  `book_toc` 26082; `toc_active` 54115. Parse+validate is only **4%**. *The template layer needs to
  change on performance grounds alone.*
- **A checked bidirectional slot contract already exists, one abstraction over.** `wdoc_component` /
  `wdoc_slot`: unknown field *and* missing-required-slot are both errors, verified by running
  `wcl check` (`schema_check.rs:353-412`).

---

## 3.1 What a template receives — the authored block tree

A template walks the WCL blocks **as authored**: a `callout` with its fields, a `code` with its
language, a heading as a heading. This enables semantic queries — "the h1", "every callout", "first
paragraph as a summary".

*Rejected:* the **lowered fundamentals tree** (querying it is querying HTML by another name — "the
Paragraph whose class is `heading-1`" is the same string-matching as `first_h1_text`, just typed); a
**purpose-built third model** (more design work, and a third thing to keep in sync with both the block
schemas and the fundamentals).

**A template sees authored blocks, not lowered output.** A user's `my_widget` appears as `my_widget`,
not as whatever it lowers to.

**The seam is read-only.** A template reorders content by *placing* handles in the order it wants, not
by mutating the tree.

## 3.2 Placement — typed block handles, resolved after template eval

The template emits handles; the renderer resolves them once evaluation is done. **No phase inversion
and no re-entrancy** — template eval never calls back into the renderer.

This was the ticket's stated risk and it is avoidable, because **the pattern already exists twice** as
magic Unicode sentinels substituted into rendered HTML (`WF_CHILDREN_SLOT` / `WF_CONTENT_SLOT`,
`render/lower.rs:120,127`; U+FFF9 chosen because it "can't appear in document content"). The pattern is
proven; the implementation is a hack. This spec makes it a typed node — and
[02](02-blocks.md) §2.8 deletes both sentinels.

Query access is cheap because it is read-only, and WCL already passes block instances as values —
that is what `project { from = c.body }` and repeater `each = <gather>` do.

## 3.3 Query scope — page-local free, cross-page memoised

A template queries its own page's block tree freely (O(page)). Cross-page queries are permitted but
**must** go through a memoised interface.

This is a direct response to the 36% measurement. Unrestricted whole-document query access would make
that pathology the default rather than an accident. Forbidding cross-page queries outright was rejected
as too strict — a template could then never invent a new kind of navigation.

**The concrete mechanism: a page-metadata builtin.** Memoised, computed once, Rust-side, **metadata
only** — so reading a neighbour's title never forces its body through lazy evaluation. Templates get
prev/next, active-path and reading-order answers from it instead of hand-walking `c.toc`.

**This is enforcement by construction** — the fast path is the ergonomic path — which is what the map's
"prose guidance is not a mechanism" preference demands. Model B otherwise *keeps the pathology
available*: "just write a fn" is the model, so the natural thing to write is the slow thing.

## 3.4 The `TemplateCtx` fields split by the same rule

- **Page-local ones become derivable** and stop being pre-extracted in Rust. `on_this_page` loses its
  dedicated module (`render/headings.rs`); **`first_h1_text` dies outright**.
- **Site-level ones stay supplied**, computed once per site: `toc`, `pages`, `menu`, `home_href`,
  `deck`.

## 3.5 Templates are authored in WCL — the element DSL

**Model B.** Templates stay in WCL and stay typed; constructing an element stops costing eight lines.
The DSL itself is specified in [02](02-blocks.md) §2.10 (`el` / `ela` / `eli` + leaf helpers), because
it is a change to the *fundamentals' constructor surface* and every block's `lower` writes them too.

**Model C collapses into A.** A heredoc cannot loop, so the first dynamic `<ul>` chops the markup into
three fragments — `<ul class="menu">`, the loop, `</ul>` — and **none is well-formed HTML**. The
parse-and-check property that distinguishes C from string concatenation is exactly what is lost at
every loop. Add `{% for %}` to fix it and C *is* A.

**Model A — external `.html` + an expression language — is ruled out, not merely unchosen.** It works;
[`proto-02-template-authoring/`](../proto-02-template-authoring/) runs it and catches five typos that
are silent today. But it means hand-writing a second language (lexer, parser, checker, span-carrying
miette diagnostics, a `{% %}` dialect that grows forever) in a repo whose conventions are "no parser
generators" and "keep the dependency list minimal". What paid for that was the paste-a-design story,
and **Wil ruled it out explicitly**: *"exact html is not a problem. Can get AI to migrate to WCL or a
script."* With that gone, A's remaining advantage is syntax preference.

**Hugo parity, scoped.** Hugo's template layer *is* Model A — `{{ range }}` / `{{ if }}` /
`{{ partial }}` is `{% for %}` / `{% if %}` / macros with different sigils. So parity is with Hugo's
**capabilities**, not its authoring surface. Already present, so not gaps: shortcodes
(`wdoc_component` / `wdoc_slot`), menus, output formats, sections and nesting (`include`). Ruled out of
parity by Wil: **i18n**, **`baseof` inheritance**, and with it **themes** — see [08](08-open.md).

**Also settled:**

- **The `wdoc_part_*` family survives.** Twelve exported composable layout pieces are WCL fns composing
  WCL fns, which is precisely what Model B is.
- **All four templates port.** `webpage`, `website` and `presentation` are mechanical shortening.
  **`book` is the one real rewrite** — and it is a rewrite because of §3.3's computation split, not
  because of syntax.
- **Site-level asset handling is unchanged**: `site.stylesheets` / `scripts` / `fonts`, the `assets`
  folder copy, the `Head` fundamental. Those are *site* declarations, not template ones.

---

## 3.6 The slot contract

**One `slot` mechanism, declared by whatever holds content, typed, checked at build.**
`region` / `wdoc_region` / `Region` / `TemplateCtx.regions` all die.

### 3.6.1 Author-declared containers survive; queries do not replace them

A **slot is an obligation** ("this layout does not work without a hero") — checkable, and a query
cannot express it, because a query finding nothing returns nothing rather than saying the page is
wrong. A **query is an opportunity**. Every query [01](../issues/01-content-seam.md) justified was a
*derivation* (`on_this_page`, `first_h1_text`), never placement. Query-only would also make the fill
site invisible — you could never point at a block and say which hole it lands in.

**What dies is the hardcoded vocabulary**: `website_layout` naming `banner`/`hero`/`sidebar`/`footer`
in the stdlib (`website.wcl:245-246`). A layout declares its own.

### 3.6.2 One mechanism for layouts and components

wdoc had three fill mechanisms with three levels of rigour:

| | declared by | filled by | checked |
|---|---|---|---|
| `wdoc_slot label` | component | instance **field** | ✅ both directions |
| `wdoc_content` | component body | nested blocks — unnamed, singular | n/a (positional) |
| `region "hero"` | *nobody* | page's `region` block | ❌ neither |

The layout slot is the middle row made **named and plural**. Components cannot do that today either —
a card cannot have a `header` slot *and* a `body` slot — so unifying closes a real component gap as a
side effect. `wdoc_content` is subsumed.

A `template` is **already a block** (`templates.wcl:327`), so slots have an obvious home:
`@children("slot")` on their declaring block.

### 3.6.3 The declaration grammar

```wcl
slot label:   utf8              // parameter, required
slot status:  utf8 = "ok"       // parameter, optional via default
slot hero:    content           // content slot, required
slot sidebar: content?          // content slot, optional
slot shapes:  content<SvgBlock> // content slot, restricted by what it accepts
slot content: content*          // MANY — this makes it a collection template (§3.8)
```

The parameter/content split falls out of the **type**, not a second keyword. Required / optional /
defaulted reuse `?` and `= …` rather than duplicating optionality machinery, and there is one uniform
declaration for the editor palette and Design mode's slot forms to introspect.

`content<SvgBlock>` needs [01](01-language.md) §1.3's syntax-only generics, and the check is done by
the `@children(SvgBlock)` the derivation emits alongside it.

**`*` is an addition from [12](../issues/12-template-selection.md)** — write it in briefs as an explicit
addition so nobody reads it as part of the original grammar.

### 3.6.4 The default body is declared, filled implicitly, and `content` is reserved

Every book, wskill and WAD page in the repo is 100% loose blocks, so requiring pages to wrap them
(`content { … }`) was off the table — hundreds of rewrites for zero information. But leaving the body
hole *magic* was rejected too, and the deciding case is the **blog list page**: a layout that renders a
repeater over child posts and no prose legitimately has **no** body hole. Magic cannot say that, so
prose put there vanishes silently.

Declared-but-implicitly-filled gets both: zero ceremony at the fill site, and `wcl wdoc build` telling
you the layout declares no `content` slot. It also removes the special case from the checker — one rule
over declared slots, none of them magic.

Reserving the name `content` beats a `@default` marker: it is already the name in
`TemplateCtx.content`, in `wdoc_content` and in every doc comment.

### 3.6.5 Checked at `wcl wdoc build`, not `wcl check`

*(Wil overruled the recommendation here, and took the layering with it — see
[01](01-language.md).)* Once wdoc concepts leave the language crate, `wcl_lang` **cannot** check wdoc
slots. And the counter-argument was weaker than it looked: Design mode's commit path already
targeted-rebuilds the current page, so a build-time slot error **does** surface in the preview pane,
just on rebuild rather than on keystroke.

Where the layout cannot be resolved statically (a computed `template =`, a repeater-generated page) the
check **degrades to silence rather than a false positive**. Precedent: `schema_check.rs:218-235`, which
already suppresses connection-operand errors a `@dynamic` connection could plausibly generate.

### 3.6.6 Fills are bare names, layout-agnostic — non-negotiable

See [README](README.md) non-negotiable #1. Scoping is then solved by construction: slots are
`@children("slot")` on their declaring `template`, nothing resolves a slot name globally, and
`CLAUDE.md`'s merged-`@document` collision trap does not reproduce.

**Accepted deliberately:** two layouts may declare `hero` differently, so a page can be **valid under
one layout and invalid under another** — a slot error can appear because a page's section or template
changed, without the page changing. That is the contract working, and build time is the right moment
for it to surface.

### 3.6.7 The severity table

| violation | verdict |
|---|---|
| unconditional fill names no slot on the resolved layout | **error** |
| conditional fill (`?`) and the layout has no such slot | content **dropped**, no diagnostic |
| conditional fill naming a slot **no** layout in the site declares | **error** — typo, still caught |
| required slot (no `?`, no default) unfilled | **error** |
| same slot filled twice on one page | **error** — ambiguous; concatenation must be explicit |
| fill content violates the slot's accepts-type | **error** |
| template's `render` references a slot the template doesn't declare | **error** at render (§3.6.9) |

The site-wide union of declared slot names does real work: it is what keeps `?` from becoming a blanket
typo amnesty.

*Wil overruled here too, and improved it.* The recommendation was to **degrade** an
unknown-to-this-layout fill into `content`. Wil's counter — error, but give the author a way to say
"fill this only if it exists" — moves the conditionality from a rule the *system* infers to a marker
the *author* writes at a specific site.

### 3.6.8 `?` marks a conditional fill

```wcl
page intro {
  hero   { h1 "…" }   // must exist on this layout — error if not
  aside? { p  "…" }   // fill if the layout has one, else dropped
}
```

`?` on the **declaration** means "this slot need not be filled"; `?` on the **fill** means "this layout
need not have the slot" — same sigil, same meaning (*this may be absent*), read from whichever side it
is written on.

Dropping is normally the sin this map kills, but here **the author wrote the marker**: the silence is
requested, at a visible site. Falling inline instead was rejected as worse — hero chrome rendered as
mid-page prose is a wrong-looking page, and a wrong-looking page is harder to notice than an absent one.

**This answers the fallback question by ownership**: the page says "fill this if you can" (`?` at the
fill), the layout says "render this when nobody filled me" (`= …` on the declaration) — which is exactly
the `website_footer`-falls-back-to-`c.title` case, and it stops being an `if` buried in template code.

### 3.6.9 Slot references resolve at render

`slot(c, :hero)` returns a handle (§3.2); the reference errors at render if the rendering template
declares no `hero`.

**This corrects [02](../issues/02-template-authoring.md)**, which claimed the check "comes free — a slot
is a symbol, so `:heor` is already a symbol-set violation." That holds only if slot names live in one
global symbol set, which §3.6.6 killed by scoping slots to their declaring template.

Render is the first moment the pairing of *this reference* with *this template's slot set* is known, so
it is the first moment the check can be correct. This matters because of `wdoc_part_*` — twelve
exported fns taking a `TemplateCtx`, **shared across templates**, so a part cannot be checked against
any one slot set at declaration time. Under this rule a part is checked once per calling template.

*Rejected:* parts declaring their own slot requirements with a calling template inheriting the
obligation — real contract composition, catches it earlier, but a second declaration layer over twelve
stdlib fns to buy a diagnostic the build already gives you.

### 3.6.10 Provenance — one `display:contents` wrapper per slot

The wrapper carries the page attrs plus `data-wcl-slot`.

*Rejected:* page attrs on the document root (tells you the page, not the hole — Design mode's inserts
need to know *which* hole); page attrs on every block anchor (invasive, and cannot represent an
**empty** slot, so you could never drop content into an unfilled hero).

**This fixes a live bug:** `build.rs:2079` wraps only `content`, while regions render at
`build.rs:2050` — *before* the wrapper — so a block inside a region has **no page-provenance ancestor
today**, and the editor cannot find its `comments.wcl` sidecar or the file to edit.

Two consequences taken deliberately:

- **Unfilled slots get a wrapper too, in edit mode.** Wil: *"show all the slots and allow editing
  them."* An invisible hole cannot be filled by direct manipulation — same reasoning as the wireframe
  empty-container placeholder.
- **Fallback content is layout-owned**, so its wrapper must say so and point provenance at the
  *layout's* file/span. Otherwise clicking a default footer tries to edit a page that never wrote one.

---

## 3.7 Template selection — no page-type axis

Selection stays exactly `page.template ?? site.default_template`. **The type is the site, or the
repeater**, both of which already share a template by construction.

**Measured:** exactly **one** `template = :` override exists in the whole repo
(`examples/wdoc_template.wcl:57`, a demo of the feature). Every real site selects at site level.

A blog is `site blog { default_template = :post }` — which also supplies the `/blog/` URL prefix, the
theme and the menu as one declaration — and its list page is the one page carrying `template = :list`.

*Rejected:* a type axis — indirection over a field already on the page; it needs a precedence chain
(site → type → page), which is the shape already ruled out with `baseof` and themes; and it makes
§3.6.6's accepted consequence bite harder, since a rule that reassigns layouts implicitly is a rule
that breaks a page without the page changing.

### 3.7.1 Collections are data, not pages

A blog post is a `post` block with a `body`. Pages come from a root repeater; the list page is a
repeater over the same gather. **This needs nothing built** — it is exactly what every wskill and WAD
already do at scale, including for prose (`project { from = <unit>.body }`), and the list builtins are
all present in `collections.rs` (`sort_by`, `group_by`, `take`, `slice`, `unique`, `reverse`, `head`,
`tail`).

**Consequence:** a post is a data record, not a page, so `page.template` never applies to one — the
repeater's single `page` block owns it. You declare a small `post` schema before writing your first
post.

*Rejected:* extending the page-metadata builtin into page *content* so
`wdoc_repeater { each = pages_in(:blog) }` works. That moves §3.3's memoisation constraint from the
template surface (four templates) into the authoring surface, where it gets used casually, and it
forces `Page` to grow a metadata bag.

### 3.7.2 Slot checking on repeater-generated pages: **possibly-fills**

The pairing is evaluated **per authored `page` block, not per rendered page**, so the check is
O(authored) and still covers everything (WAD: 32 declarations, 161 pages).

> A repeater or conditional fill site counts as **possibly filling** the slot it targets. A required
> slot with **no fill site anywhere in the page** — static or dynamic — is a hard error. A required
> slot with only conditional fill sites passes. **Silence is scoped to the individual slot, never the
> page.**

Precedent: `validate_connection_stmts` (`schema_check.rs:218-235`) suppresses an unresolved-operand
error only when a `@dynamic` connection plausibly admits the statement, rather than switching
validation off for the whole diagram.

**Accepted cost:** a required slot whose only filler is a repeater that turns out empty renders as an
unfilled slot with no warning — under §3.6.10 that is a `display:contents` wrapper with nothing in it,
visible in the output, not a crash.

### 3.7.3 `Page.sites` stops defaulting to "every site" in multi-site documents

Today absent/empty ⇒ every site (`core.wcl:172`). That was harmless when a site was just an output
folder; under §3.7 the site **chooses the page's layout**, so adding a `site blog` to an existing
document would silently re-render every existing page into `/blog/` under the blog's template, without
any page changing.

- **One site** (every wskill projection, WAD, most user documents): unchanged, no `sites` anywhere.
- **More than one site**: `sites` is **required**; a page carrying none is a build error.

**Migration cost measured at approximately zero**: in both multi-site documents in the repo
(`docs/main.wcl`, `examples/wdoc/main.wcl`) `sites =` declarations already outnumber `page` blocks.

---

## 3.8 Collection templates

*Recommended against and overruled* (Wil: *"yeah add a collection template too"*). Two things reframe
it once general, both of which make it a better decision than the one recommended:

- It **generalises something that already exists**. Deck-ness is a hardcoded string comparison —
  `build.rs:1483` is `default_template.as_deref() == Some("presentation")`. Making collections general
  **deletes that comparison**, so collection-ness becomes a property of the layout's declared slots and
  a **user-declared** collection template works. Same stringly-typed dispatch this map exists to kill,
  in a place nobody had listed.
- The performance objection is **backwards**. The 36% is an O(n²) pathology; a collection template
  renders **once** for n pages — O(n), and n bodies is the irreducible cost of one file containing n
  bodies.

**3.8.1 Slot arity is the declaration.** `slot content: content` = one page's content (a page
template); `slot content: content*` = many (a collection template). Chosen over a `collection = true`
flag or a distinct block kind, because those are two declarations of one fact whose failure mode is
divergence.

**3.8.2 Site-level invocation.** A site whose template is a collection template renders to **one file**
— the site directory's `index.html`; its pages are the members and get no individual HTML output.
Members arrive as **typed page handles plus metadata**, *not* pre-rendered HTML, and the renderer
resolves each placed handle **after template eval**, exactly as §3.2 settled for block handles.

- **Forcing is demand-driven**: a template placing 3 of 40 handles forces 3 bodies. Today
  `DeckSlide.content` (`presentation.wcl:60`) forces every slide unconditionally.
- **No template holds another page's HTML as a string**, so the seam this map exists to kill is not
  re-established in the one place with a legitimate excuse for it.
- `presentation`'s deck rendering collapses into this, and `site.deck` demotes from *the mechanism* to
  *one way of ordering members* that the presentation template reads.

*Rejected:* page-level invocation (an ordinary page with a member query, members still rendering their
own pages) — under §3.7.1 its use cases aren't page-shaped, and it would introduce a second,
query-shaped membership rule that could disagree with `Page.sites`.

**Accepted cost:** a collection **replaces** per-page output for its site. Wanting both — per-post
pages *and* an all-posts file — means two sites over the same pages: `sites = [:blog, :blog_all]`.

**3.8.3 The filler follows the arity.**

- **`*` slots are per-member**, filled by each member page using §3.6's ordinary bare-name fills.
- **Non-`*` slots on a collection template are site-level**, filled by the **`site` block**, which
  gains slot fills as child blocks.

Evidence that per-member slots beyond `content` are required: `DeckSlide { content, notes, title }`
(`presentation.wcl:59`) — `notes` is already a second per-member hole. So `*` applies to **any** slot,
not just the reserved one.

*Rejected:* routing site-level fills through the `start` page — makes one member secretly also the
chrome, an implicit role exactly like the one §3.7.3 deletes, and it breaks when a collection site has
no start page.

**Two consequences:** a **`site` block can now carry content**, where every `SiteConfig` field today is
data; and §3.6.7's required-slot check gains a second target — site-level required slots checked
against the **site**, per-member required slots against **every member page**, with §3.7.2's
possibly-fills rule applying to the latter unchanged.

**3.8.4 HTML-only.** No filename control. PDF, Markdown and skill render member pages individually; the
template never runs and `default_template` continues to serve only as a visibility axis there. Both
rules generalise existing deck behaviour unchanged.

**In-repo proof:** the collection template does not rest on the out-of-repo blog. Its in-repo consumers
are the presentation projections — every wskill's deck and `examples/wdoc`'s `talk` site — which are
today's special case and become the general mechanism's first users.

**Deliberately not bought: feeds.** See [08](08-open.md).

---

## Checklist for this part

- [ ] Block handles as typed nodes; both U+FFF9 sentinels gone (with [02](02-blocks.md) §2.8)
- [ ] `TemplateCtx` split; `render/headings.rs` and `first_h1_text` deleted
- [ ] Page-metadata builtin — memoised, metadata-only, never forces a body
- [ ] `slot` grammar incl. `?`, `= …`, `content<T>`, `*`; `content` reserved
- [ ] `region` / `wdoc_region` / `Region` / `TemplateCtx.regions` deleted; `website_layout`'s hardcoded names gone
- [ ] Six-row severity table at `wcl wdoc build`; possibly-fills; static-unresolvable ⇒ silence
- [ ] Per-slot `display:contents` wrapper incl. unfilled slots in edit mode; layout-owned fallback provenance
- [ ] Slot references resolve at render
- [ ] `Page.sites` required in multi-site documents
- [ ] Collection templates; `build.rs:1483` and `build_presentation_page` deleted; `presentation` rewritten as an ordinary collection template
- [ ] All four templates ported; `book` rewritten around the metadata builtin
