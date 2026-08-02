# Template selection — does a page *type* axis exist, or is it per-page?

Type: grilling
Status: resolved
Blocked by: —

## Question

Wil's ask on ticket 02, in his words: **"more just want to be able to set templates for page types
and the like and then render them."**

Part of that already exists. `page.template: symbol?` overrides the site's `default_template`
(`crates/wcl_wdoc/lib/core.wcl:167`), so a page can pick its layout today. What does **not** exist is
a *type* axis: every page names its own template individually, and there is no notion of "every page
in this section renders with `post`", the way Hugo picks `single.html` vs `list.html` by page kind
and section.

Decide whether wdoc grows one, and what it keys on.

The tension:

- **Per-page selection is honest and already works.** A type axis is another layer of indirection
  over a field that is right there on the page. For a 30-page book it buys nothing.
- **But the target use cases are landing pages and a blog** (ticket 02), and those are exactly where
  it bites: every post rendering with `post`, index pages with `list`, without restating it per page.
  A blog also wants the *list* side — a template that renders a collection of pages, which today only
  `presentation` does (via `site.deck`) and only as a special case.

Things to settle, not a menu:

- What is a "page type"? A symbol on the page, folder/section membership, the gather it came from,
  or the schema type of the block that declared it? wdoc has no section concept today —
  `include` nests *sites*, not page groups within a site.
- Does a **list/index template** need a first-class notion of "the pages I am rendering", or is
  `site.pages` plus a filter enough? Note ticket 01 confined cross-page access to a memoised
  interface, and ticket 02 resolved that as a **page-metadata builtin** — a list template is the
  main consumer of it.
- Is this **selection** only, or does it reach the slot contract? A layout that declares required
  slots (ticket 03) can only be checked against a page once you know which pages get it. If
  selection is per-type, the check becomes per-type too — which is *better*, not worse.
- Does anything here change if the blog is out of repo? (See the map's Out of scope: out-of-repo
  migration is a separate effort — but a blog is a *new build*, not a migration.)

## Inherited from ticket 02 (resolved)

- The template model is **Model B — a terse WCL element DSL**, not external HTML files. A "template"
  is a WCL `template` block with a `render` fn, so selection is selecting a symbol, not resolving a
  file path by convention. Hugo's whole `layouts/_default/single.html` lookup-by-filename mechanism
  is therefore **not** available as a design to copy — that was Model A's world.
- **`baseof` inheritance and themes are out of scope** (Wil, explicitly). Don't reintroduce them
  through a type system that wants a fallback chain.
- The **page-metadata builtin** (memoised, metadata-only, never forces a page body) is the sanctioned
  way any template reaches other pages. A list template must go through it.

## Relationship to ticket 03

03 (the slot contract) asks how a layout declares what it needs and how a page is checked against it.
This ticket asks which pages a layout gets in the first place. They interact but neither blocks the
other: 03 can be answered per-page and generalise, and this one can be answered without knowing the
slot syntax. Resolve either first; whichever goes second inherits from the first.

## Inherited from ticket 03 (resolved — this ticket went second)

**A page does not name its layout, and must not.** 03 made slot fills **bare names, layout-agnostic**,
because one wskill unit body is projected into four template sets (book, skill, training, deck) via
`project { from = <unit>.body }`. Wil: *"make sure we can do that projection or wskill breaks."* Any
selection rule you design must preserve that — the page supplies content, the site/section supplies
the layout.

**Your rule must degrade to silence when it can't be resolved statically.** 03 put slot checking at
`wcl wdoc build`, and the pairing it checks is `page.template ?? site.default_template` plus whatever
you add. Where that can't be determined — a computed `template =`, a repeater-generated page — the
slot check must go quiet rather than emit a false positive. Precedent:
`crates/wcl_lang/src/doc/schema_check.rs:218-235`.

**Accepted consequence you inherit:** two layouts may declare `hero` differently, so a page can be
valid under one and invalid under another. Changing a page's section or template can therefore break
it without the page changing. That is the contract working — but your selection rule determines *how
often* it happens, and a rule that reassigns layouts implicitly makes it happen more.

**A layout may legitimately declare no `content` slot** — a blog list page rendering a repeater over
child posts and no prose. So "every page has a body" is not a safe assumption for the selection rule.

## Resolution

**There is no page-type axis. Selection stays two-level — and the ticket's second half turned out to
be the real one: wdoc grows *collection templates*, which deletes a hardcoded string comparison in
the renderer.**

### Measured before deciding (don't re-derive)

- **Per-page template selection is essentially unused.** Across `docs/`, `.wad/`, `examples/` and
  `crates/` there is exactly **one** `template = :` override in the repo — `examples/wdoc_template.wcl:57`,
  a demo of the feature itself. Every real site selects at site level via `default_template`.
- **Two grouping concepts already exist and already share a template by construction.** A `site` block
  is a named page group with one `default_template` and its own output subdirectory
  (`examples/wdoc/main.wcl` declares four — including, already, a `site blog`); and a root
  `wdoc_repeater` with `page` children turns **one** authored `page` block into N rendered pages
  (`core.wcl:31`, `components.wcl:93`), all sharing that block's `template`. WAD's 161 pages come from
  32 declarations this way.
- **List operations exist.** `collections.rs` carries `sort`, `sort_by`, `unique`, `reverse`, `take`,
  `slice`, `group_by`, `head`, `tail`, `len`, `list_contains` — newest-first ordering, tag grouping and
  pagination are all expressible today.
- **Date handling does not exist at all.** No `now`, no date type, no parse/format builtin anywhere in
  `wcl_lang`. ISO-8601 strings sort correctly under `sort_by`, so ordering works; *display* formatting
  means authoring the string or doing string surgery. Recorded as a gap, not resolved here.
- **`TemplateCtx.pages` is `{ name, href }` only** (`templates.wcl:18`), and `Page` carries `title` but
  no date/summary/tags. `frontmatter` is `@schemaless` and **Markdown-target-only** (`core.wcl:190`).
  So a list page has nothing to render *from* if pages are the collection.
- **`DeckSlide.content` is every slide's rendered body HTML** (`presentation.wcl:60`) — the presentation
  template already receives N page bodies and forces all of them, unconditionally.
- **Deck-ness is a hardcoded string comparison.** `build.rs:1483` is
  `default_template.as_deref() == Some("presentation")` — the renderer decides "this site renders as one
  file" by comparing against a built-in template's *name*.
- **PDF and Markdown ignore deck-ness entirely.** Both read `default_template` only to set
  `@only`/`@except` visibility context (`pdf/mod.rs:292`, `markdown/mod.rs:240`) and then emit one
  physical output per page block. Consistent with 05's finding that templates are HTML-only.

### 1. No page-type axis

Selection stays exactly `page.template ?? site.default_template`. **The type is the site, or the
repeater.** A blog is `site blog { default_template = :post }` — which also supplies the `/blog/` URL
prefix, the theme and the menu as one declaration — and its list page is the one page carrying
`template = :list`.

Rejected because a type axis is indirection over a field already on the page; it needs a
precedence chain specified (site → type → page), which is the shape 02 already ruled out with
`baseof` and themes; and it makes 03's inherited consequence bite harder, since a rule that reassigns
layouts implicitly is a rule that breaks a page without the page changing.

Note the saving is *not* line count: `Page.sites` also costs one line per page in a multi-site
document. The saving is that the site carries layout, URL prefix, theme and nav as one declaration.

### 2. Collections are data, not pages

A blog post is a `post` block with a `body`. Pages come from a root repeater; the list page is a
repeater over the same gather. **This needs nothing built** — it is exactly what every wskill and WAD
already do at scale, including for prose (`project { from = <unit>.body }`), and the list builtins are
all present.

Rejected: extending the page-metadata builtin into page *content* so `wdoc_repeater { each = pages_in(:blog) }`
works. That moves 01's "cross-page must be memoised" constraint from the template surface — four
templates — into the authoring surface, where it gets used casually, and it forces `Page` to grow a
metadata bag (or `frontmatter` to stop being Markdown-only).

Consequence: a post is a data record, not a page, so `page.template` never applies to one — the
repeater's single `page` block owns it. You declare a small `post` schema before writing your first
post.

### 3. Slot checking on repeater-generated pages: **possibly-fills**

The pairing 03 checks is evaluated **per authored `page` block, not per rendered page**, so the check
is O(authored) and still covers everything (WAD: 32 declarations for 161 pages).

03 said "degrade to silence when the template can't be resolved statically". This ticket adds the case
03 didn't have: the template resolves fine and the **fill set** is dynamic. Rule:

> A repeater or conditional fill site counts as **possibly filling** the slot it targets. A required
> slot with **no fill site anywhere in the page** — static or dynamic — is a hard error. A required slot
> with only conditional fill sites passes. Silence is scoped to the individual slot, never the page.

Precedent: `validate_connection_stmts` (`schema_check.rs:218-235`) suppresses an unresolved-operand
error only when a `@dynamic` connection *plausibly admits* the statement, rather than switching
validation off for the whole diagram.

Rejected: skipping the page (switches the check off for essentially every wskill and WAD page — the
majority of pages in the repo) and strict static checking (forces authors to hoist fills out of
repeaters, making the common data-driven page the awkward one).

Accepted cost: a required slot whose only filler is a repeater that turns out empty renders as an
unfilled slot with no warning. Under 03 that is a `display:contents` wrapper with nothing in it —
visible in the output, not a crash.

### 4. `Page.sites` stops defaulting to "every site" in multi-site documents

Today absent/empty ⇒ every site (`core.wcl:172`). That was harmless when a site was just an output
folder; under decision 1 the site *chooses the page's layout*, so adding a `site blog` to an existing
document silently re-renders every existing page into `/blog/` under the blog's template, without any
page changing. That is exactly 03's implicit-reassignment risk, and decision 1 is what makes the site
the thing doing the reassigning.

- **One site** (every wskill projection, WAD, most user documents): unchanged, no `sites` anywhere.
- **More than one site**: `sites` is **required**; a page carrying none is a build error. A genuinely
  shared page writes `sites = [:web, :blog]`, visible rather than inferred.

Migration cost measured at approximately zero: in both multi-site documents in the repo
(`docs/main.wcl`, `examples/wdoc/main.wcl`) `sites =` declarations already outnumber `page` blocks.

### 5–9. Collection templates

**Recommended against and overruled** (Wil: *"yeah add a collection template too"*). Two things
reframed it once general, both of which make it a better decision than the one recommended:

- It **generalises something that already exists**. The deck is the mechanism, special-cased; making it
  general deletes `build.rs:1483`'s `default_template == "presentation"` string comparison. Collection-ness
  becomes a property of the layout's declared slots, so a **user-declared** collection template works and
  the built-in stops being privileged. That is the same stringly-typed dispatch this map exists to kill,
  in a place nobody had listed.
- The performance objection is **backwards**. Ticket 11's 36% is an **O(n²)** pathology — `book_pageflow`
  called once per page, re-walking the whole tree, 26082 calls for 161 pages. A collection template
  renders **once** for n pages: O(n), and n bodies is the irreducible cost of one file containing n
  bodies. It is not the shape 01 was guarding against.

**5. They exist as a general capability.**

**6. Site-level invocation.** A site whose template is a collection template renders to **one file**;
its pages are the members and get no individual HTML output. Members arrive as **typed page handles
plus metadata** — *not* pre-rendered HTML — and the renderer resolves each placed handle **after
template eval**, exactly as 01 settled for block handles. Consequences:

- Forcing is **demand-driven**: a template placing 3 of 40 handles forces 3 bodies. Today
  `DeckSlide.content` forces every slide unconditionally.
- No template holds another page's HTML as a string, so the seam this map exists to kill is not
  re-established in the one place with a legitimate excuse for it.
- `presentation`'s deck rendering collapses into this, and `site.deck` demotes from *the mechanism* to
  *one way of ordering members* that the presentation template reads.

Rejected: page-level invocation (an ordinary page with a member query, members still rendering their
own pages). Under decision 2 its use cases aren't page-shaped — "homepage with full post text inline"
is `project { from = p.body }` over the `post` gather, no page involvement. The case that genuinely
needs *pages* as members is "this whole site collapses into one file", which is site-level by
definition. It also keeps membership governed by `Page.sites` rather than introducing a second,
query-shaped membership rule that could disagree with it.

Accepted cost: a collection **replaces** per-page output for its site. Wanting both — per-post pages
*and* an all-posts file — means two sites over the same pages: `sites = [:blog, :blog_all]`.

**7. Slot arity is the declaration.**

- `slot content: content` — one page's content. A page template.
- `slot content: content*` — many. A collection template.

Chosen over a `collection = true` flag or a distinct block kind because those are two declarations of
one fact whose failure mode is divergence; arity is a single source of truth on the surface 03 already
made checkable, so `wcl wdoc build` validates it with machinery 03 already specified.

**This extends 03's slot grammar with `*`** — 03 defined optional (`?`), defaulted (`= …`) and typed
(`content<SvgBlock>`), not repetition. Write it in the spec as an explicit addition so nobody reads it
as already-decided.

**8. The filler follows the arity.**

- **`*` slots are per-member**, filled by each member page using 03's ordinary bare-name fills.
- **Non-`*` slots on a collection template are site-level**, filled by the **`site` block**, which
  gains slot fills as child blocks.

Evidence that per-member slots beyond `content` are required: `DeckSlide { content, notes, title }`
(`presentation.wcl:59`) — `notes` is already a second per-member hole. So `*` applies to any slot, not
just the reserved one.

Rejected: routing site-level fills through the `start` page — that makes one member secretly also the
chrome, an implicit role exactly like the one decision 4 deletes, and it breaks when a collection site
has no start page.

03's non-negotiable survives on both sides: fills stay **bare names, layout-agnostic**, so a member
page writes `notes` without knowing whether it is being projected into a deck or a book — the property
the wskill four-way projection depends on.

Two consequences: a **`site` block can now carry content**, where every `SiteConfig` field today is
data; and 03's required-slot check gains a second target — site-level required slots checked against
the **site**, per-member required slots against **every member page**, with decision 3's possibly-fills
rule applying to the latter unchanged.

**9. HTML-only; output is the site directory's `index.html`.** No filename control. PDF, Markdown and
skill render member pages individually — the template never runs and `default_template` continues to
serve only as a visibility axis there. Both rules generalise existing deck behaviour unchanged.

**Deliberately not bought: feeds.** An RSS/Atom file is one output over a collection and looks like it
should fall out of this, but it needs a non-HTML output file with a chosen name, and wdoc has **no
mechanism to emit a generated text file at all** — the `file` block only copies from disk
(`file.wcl:23`). Sharpened into the map's "blog as a consumer" fog rather than resolved here.

**In-repo proof.** The collection template does not rest on the out-of-repo blog: its in-repo consumers
are the presentation projections — every wskill's deck and `examples/wdoc`'s `talk` site — which are
today's special case and become the general mechanism's first users.

### What this hands downstream

- The **spec** gains: `*` on 03's slot grammar; site-level slot fills on `SiteConfig`; the
  possibly-fills rule; the multi-site `sites`-required rule; typed page handles as the collection member
  representation (reusing 01's handle machinery, not a second mechanism).
- The **migration** gains one deletion (`build.rs:1483`'s presentation string comparison plus
  `build_presentation_page`'s dedicated path) and one rewrite (the `presentation` template as an
  ordinary collection template).
- Two gaps recorded, neither resolved: **no date builtins** anywhere in `wcl_lang`, and **no mechanism
  to emit a generated non-HTML output file**.
