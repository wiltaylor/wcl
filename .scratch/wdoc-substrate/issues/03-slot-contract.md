# The slot contract — how does a layout declare what it needs, and how is a page checked against it?

Type: grilling
Status: open
Blocked by: 02

## Question

Regions today are a one-way, unchecked, flat string namespace:

- A page declares `region "hero" { … }`; a layout pulls `wdoc_region(c, "hero")`.
- A layout **cannot declare** which regions it needs, so nothing knows a page is missing one.
- `wdoc_region(c, "heor")` returns `""`. Silently. Forever.
- All region names share one flat namespace with no ownership.

The embryo of the right idea is already there — *"render wdoc content into blocks on pages"* is
exactly what regions do. It just has no contract.

Decide the contract:

- **How a layout declares its slots.** Required vs optional vs defaulted. Does a slot declare a
  *fallback* (the current `website_footer` falls back to `c.title` when no footer region exists), and
  is that the layout's business or the page's?
- **Are slots typed by what they accept?** A `hero` slot that only accepts an `h1` + `p`; a
  `sidebar` that accepts any `WdocBlock`. Or are slots untyped holes and typing is over-engineering?
- **What does `wcl check` report?** An unfilled required slot, a page filling a slot the layout
  doesn't declare, a duplicate slot — which are errors, which warnings, which silent?
- **Where does the default body go?** Everything outside a region is `c.content` today — an implicit
  unnamed slot. Does it become an explicit named one, and what breaks if it does?
- **Scoping.** Do slot names belong to the layout that declares them, and what happens when a site
  has several layouts? Bear in mind the merged-`@document` corollary in `CLAUDE.md`: gather-field
  names already share one namespace across imported schemas and silently collide. Don't repeat it.

Constraint worth surfacing early: the editor's Design mode and the wskill projections both need to
know which slot a given block lives in, to place edits and visibility toggles. A slot contract that
only exists at render time doesn't help them.

Blocked by `02-template-authoring`: the contract's expression depends on where templates are authored.

## Inherited from ticket 01 (resolved)

**The open question 01 deliberately left you:** does `region` survive as the slot mechanism at all, or
does a queryable page block tree make it redundant? A template that can ask the page tree for "the
blocks tagged hero" may not need an author-declared `region "hero" { … }` wrapper. Settle this before
designing the declaration syntax.

**A provenance wrinkle you own.** The edit-mode page wrapper (`build.rs:2079`) wraps the single
`content` string in a `display:contents` div carrying `data-wcl-page-file` / `-name` / `-span`, which
the editor client uses to locate the owning `comments.wcl` sidecar and the file to edit. With multiple
typed slots there is no longer one content string to wrap — decide where that provenance goes. The
per-block anchors themselves are fine: `anchor_block` stamps at render time, which becomes
handle-resolve time.

**Already settled by 01** — page-local queries are free, cross-page queries must be memoised (this is
what keeps the `wdoc_book_layout` 26082-walk pathology from returning), and site-level facts (`toc`,
`pages`, `menu`, `home_href`, `deck`) stay *supplied* rather than derived.
