# The slot contract — how does a layout declare what it needs, and how is a page checked against it?

Type: grilling
Status: resolved
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

## Inherited from ticket 02 (resolved)

**Templates are WCL, not HTML files** — Model B, a terse element DSL (`div(".ws-header", [ … ])`). The
"external `.html` file with a Jinja-shaped expression language" option was ruled out: the paste-a-design
story that paid for it is dead (Wil: *"exact html is not a problem, can get AI to migrate to WCL or a
script"*), leaving only syntax preference against the cost of hand-writing a second language.

**This decides most of your declaration syntax for you, and it is the good case.** A slot is a
**symbol**, not a string — `slot(c, :hero)`, declared `slots = [:banner?, :hero?, :content]`. So:

- The typo case that motivated this whole map (`wdoc_region(c, "heor")` → `""`) is already an error, via
  the existing symbol-set check. You are not designing *that*; you are designing what else is checked.
- **Required vs optional is already expressible** — a symbol set plus an optional marker. Don't invent
  a parallel mechanism.
- **Your scoping question has a sharper form.** Symbol sets are namespaced WCL types, so "do slot names
  belong to the layout" becomes "is each layout's slot set its own `symbol_set`, or is there one shared
  vocabulary" — a question about WCL declarations, not about inventing a namespace. The merged-`@document`
  collision warning in `CLAUDE.md` still applies.

**A new constraint from 02.** Any cross-page reach — a layout wanting neighbours, reading order, or
active-path — goes through a **page-metadata builtin**: memoised, computed once, **metadata only**, so it
never forces a page body through lazy evaluation. This is 01's "cross-page must be memoised" given a
concrete mechanism. If the slot contract wants to be checked across pages, it consumes that builtin.

**Also inherited:** `wdoc_part_*` survives (composable WCL fns are what Model B *is*); site-level asset
handling is unchanged; template-owned CSS moves to sibling `.css` files.

**Interacts with ticket 12** (template selection by page type) — neither blocks the other, but a layout's
slots can only be checked against a page once you know which pages get that layout. Whichever resolves
second inherits from the first.

## Answer

**One `slot` mechanism, declared by whatever holds content, typed, checked at build.** `region` /
`wdoc_region` / `Region` / `TemplateCtx.regions` all die.

### What the grilling found before it decided anything

Three measured facts reframed the ticket:

- **Regions are all but unused.** **7** `region "…"` blocks exist in the entire repo — 4 names
  (`hero`×2, `footer`×3, `banner`, `sidebar`) across `docs/pages/wcl/index.wcl`,
  `docs/pages/reference/wdoc/websites.wcl`, `examples/wdoc_website.wcl`. Only the `website` template
  consumes them; book, presentation and webpage use **zero**, as do WAD and every wskill projection.
  The mechanism this ticket exists to fix has almost no users to break.
- **A checked, bidirectional slot contract already exists in wdoc — for components.** Verified by
  running `wcl check`: `field 'labell' is not a slot of component 'metric_card'` /
  `component 'metric_card' is missing required slot 'label'`. `validate_component_instance`
  (`crates/wcl_lang/src/doc/schema_check.rs:353-412`) checks **both directions**, and `default` makes a
  slot optional. The contract this ticket was asked to invent is already shipping, one abstraction over.
- **A `template` is already a block** — `@block("template") type Template { @inline(0) name: identifier,
  render: fn(TemplateCtx) -> list<HtmlFundamental> }` (`templates.wcl:327`). So slots have an obvious
  home (`@children("slot")`) and the declaration site needed no decision at all.

### The decisions

**1. Author-declared containers survive; queries do not replace them.** A **slot is an obligation**
("this layout does not work without a hero") — checkable, and a query cannot express it, because a
query finding nothing returns nothing rather than saying the page is wrong. A **query is an
opportunity**. Ticket 01's evidence agrees: every query it justified was a *derivation*
(`on_this_page`, `first_h1_text`), never placement. Query-only would also make the fill site invisible —
you could never point at a block and say which hole it lands in. **What dies is the hardcoded
vocabulary**: `website_layout` naming `banner`/`hero`/`sidebar`/`footer` in the stdlib
(`website.wcl:245-246`). A layout declares its own.

**2. One mechanism for layouts and components — and it is `wdoc_content` grown up.** wdoc had three
fill mechanisms with three different levels of rigour:

| | declared by | filled by | checked |
|---|---|---|---|
| `wdoc_slot label` | component | instance **field** | ✅ both directions |
| `wdoc_content` | component body | nested blocks — unnamed, singular | n/a (positional) |
| `region "hero"` | *nobody* | page's `region` block | ❌ neither |

The layout slot is the middle row made **named and plural**. Components can't do that today either —
a card cannot have a `header` slot and a `body` slot — so unifying closes a real component gap as a
side effect rather than as separate work. Inventing a third declaration syntax for "declare named
holes, fill them, check both ways" is the exact failure mode this map exists to kill.

**3. One keyword, and the parameter/content split falls out of the *type*.**

```wcl
slot label:   utf8              // parameter, required
slot status:  utf8 = "ok"       // parameter, optional via default
slot hero:    content           // content slot, required
slot sidebar: content?          // content slot, optional
slot shapes:  content<SvgBlock> // content slot, restricted by what it accepts
```

WCL is a typed language and this ticket's whole complaint is a seam with no type. Making `content` a
slot **type** rather than a second keyword means the ticket's "are slots typed by what they accept?"
question stops being a separate decision and becomes a consequence. Required / optional / defaulted
reuse `?` and `= …` rather than duplicating optionality machinery, and there is one uniform
declaration for the editor palette and Design mode's slot forms to introspect.

**4. The default body is declared, filled implicitly, and `content` is reserved.** Every book, wskill
and WAD page in the repo is 100% loose blocks, so requiring pages to wrap them (`content { … }`) was
off the table — hundreds of rewrites for zero information. But leaving the body hole *magic* was
rejected too, and the deciding case is the **blog list page** Wil raised: a layout that renders a
repeater over child posts and no prose legitimately has **no** body hole. Magic can't say that, so
prose put there vanishes silently. Declared-but-implicitly-filled gets both: zero ceremony at the fill
site, and `wcl wdoc build` telling you the layout declares no `content` slot. It also removes the
special case from the checker — one rule over declared slots, none of them magic. Reserving the name
`content` beats a `@default` marker: it's already the name in `TemplateCtx.content`, in `wdoc_content`
and in every doc comment.

**5. Checked at `wcl wdoc build`, not `wcl check` — and wdoc gets broken out of `wcl_lang`.**
*(Wil overruled the recommendation here, and took the layering with it.)* The recommendation was
`wcl check`, on the grounds that the editor's save path validates through `schema_errors`. That
argument was weaker than it looked: Design mode's commit path already targeted-rebuilds the current
page, so a build-time slot error **does** surface in the preview pane, just on rebuild rather than on
keystroke. And the two decisions are consistent — once wdoc concepts leave the language crate,
`wcl_lang` *cannot* check wdoc slots.

Measured for the extraction: 109 raw `wdoc` hits in `crates/wcl_lang/src/`, but only **12 are live
code outside tests**, in two clusters — components/slots (`doc.rs:2043,2157`, `views.rs:2884,2895`,
`schema_check.rs:374`) and repeaters/instances (`views.rs:2813,2834,2854`, `schema_check.rs:787`). The
rest are doc comments, a lexer test string and a `reflect.rs` docstring example. Tractable seam, not a
rewrite. **Split out as [ticket 14](14-wdoc-lang-extraction.md).**

Where the layout can't be resolved statically (computed `template =`, a repeater-generated page) the
check degrades to silence rather than a false positive — precedent at `schema_check.rs:218-235`, which
already suppresses connection-operand errors a `@dynamic` connection could plausibly generate.

**6. Fills are bare names, layout-agnostic — this one is load-bearing.** Wil: *"make sure we can do
that projection or wskill breaks."* One unit body is projected into **four** template sets (book,
skill, training, deck) via `project { from = <unit>.body }`. Binding a fill to a named layout would
make cross-projection content impossible, which is the entire wskill model. Ticket 12 pushes the same
way — the point of page-type selection is that a page *doesn't* pick its layout.

Scoping is then solved by construction: slots are `@children("slot")` on their declaring `template`,
nothing resolves a slot name globally, and the `CLAUDE.md` merged-`@document` collision trap does not
reproduce. Accepted deliberately: two layouts may declare `hero` differently, so a page can be **valid
under one layout and invalid under another** — a slot error can appear because a page's section or
template changed, without the page changing. That's the contract working, and build time is the right
moment for it to surface.

**7. Strict errors, plus an explicit opt-out.** *(Wil overruled here too — and improved it.)* The
recommendation was to **degrade** an unknown-to-this-layout fill into `content`, on the grounds that
projection makes it the common case. Wil's counter: error, but give the author a way to say "fill this
only if it exists". That's better, because it moves the conditionality from a rule the *system* infers
to a marker the *author* writes at a specific site.

| violation | verdict |
|---|---|
| unconditional fill names no slot on the resolved layout | **error** |
| conditional fill (`?`) and the layout has no such slot | content **dropped**, no diagnostic |
| conditional fill naming a slot **no** layout in the site declares | **error** — typo, still caught |
| required slot (no `?`, no default) unfilled | **error** |
| same slot filled twice on one page | **error** — ambiguous; concatenation must be explicit |
| fill content violates the slot's accepts-type | **error** |
| template's `render` references a slot the template doesn't declare | **error** at render (see 9) |

The site-wide union of declared slot names still does real work: it is what keeps `?` from becoming a
blanket typo amnesty.

**8. `?` marks a conditional fill; the page owns conditionality, the layout owns the fallback.**

```wcl
page intro {
  hero   { h1 "…" }   // must exist on this layout — error if not
  aside? { p  "…" }   // fill if the layout has one, else dropped
}
```

`?` on the **declaration** means "this slot need not be filled"; `?` on the **fill** means "this
layout need not have the slot" — same sigil, same meaning (*this may be absent*), read from whichever
side it's written on. Dropping is normally the sin this map kills, but here the author wrote the
marker: the silence is requested, at a visible site. Falling inline instead was rejected as worse —
hero chrome rendered as mid-page prose is a wrong-looking page, and a wrong-looking page is harder to
notice than an absent one.

This answers the ticket's fallback question by **ownership**: the page says "fill this if you can"
(`?` at the fill), the layout says "render this when nobody filled me" (`= …` on the declaration) —
which is exactly the `website_footer`-falls-back-to-`c.title` case, and it stops being an `if` buried
in template code.

**9. Provenance becomes one `display:contents` wrapper per slot, and the editor shows every slot.**
The wrapper carries the page attrs plus `data-wcl-slot`. Rejected: page attrs on the document root
(tells you the page, not the hole — Design mode's inserts need to know *which* hole), and page attrs
on every block anchor (invasive, and cannot represent an **empty** slot, so you could never drop
content into an unfilled hero).

This also **fixes a live bug**: `build.rs:2079` wraps only `content`, while regions are rendered at
`build.rs:2050` — *before* the wrapper — so a block inside a region has no page-provenance ancestor
today, and the editor cannot find its `comments.wcl` sidecar or the file to edit.

Two consequences taken deliberately:
- **Unfilled slots get a wrapper too** (edit mode). Wil: *"show all the slots and allow editing them."*
  An invisible hole can't be filled by direct manipulation — same reasoning as the wireframe
  empty-container placeholder.
- **Fallback content is layout-owned**, so its wrapper must say so and point provenance at the
  layout's file/span. Otherwise clicking a default footer tries to edit a page that never wrote one.

**10. Slot references resolve at render — correcting ticket 02.** 02 claimed the reference check
"comes free — a slot is a symbol, so `:heor` is already a symbol-set violation." **That only holds if
slot names live in one global symbol set**, which decision 6 killed by scoping slots to their
declaring template. The free check was illusory.

`slot(c, :hero)` returns a handle (ticket 01's mechanism); the reference errors at render if the
rendering template declares no `hero`. Render is the first moment the pairing of *this reference* with
*this template's slot set* is known, so it's the first moment the check can be correct. This matters
because of `wdoc_part_*` — twelve exported fns taking a `TemplateCtx`, **shared across templates**, so
a part cannot be checked against any one slot set at declaration time. Under this rule a part is
checked once per calling template, which is the right granularity.

Rejected: parts declaring their own slot requirements, with a calling template inheriting the
obligation. Real contract composition, catches it earlier — but a second declaration layer over twelve
stdlib fns to buy a diagnostic build already gives you.

### Constraints passed downstream

- **Ticket 10 (editor review)** inherits *show and allow editing of every slot a layout declares,
  filled or not* — the per-slot wrapper is the editing surface, not just provenance.
- **Ticket 12 (template selection)** inherits that a layout's slots are only checkable once page→layout
  selection is known, and that the resolution must degrade to silence when it can't be determined
  statically.
- **Ticket 14 (wdoc out of `wcl_lang`)** is created by decision 5 and carries its measurements.
- **Ticket 05 (block type system)** inherits that `wdoc_content` is subsumed by the new `slot`, and
  that `content<SvgBlock>` needs the accepts-type to be expressible over whatever replaces
  `WdocBlock` / `SvgBlock`.

### Deliberately not decided here

The **surface syntax** of a content-slot fill beyond `name { … }` / `name? { … }`, and whether
`slot` needs a namespace prefix in the stdlib (`wdoc_slot` vs bare `slot`) — naming, settled when the
stdlib is written.

Status: resolved
