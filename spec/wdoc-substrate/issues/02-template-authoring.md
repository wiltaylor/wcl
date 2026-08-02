# How is an HTML template authored?

Type: prototype
Status: resolved
Blocked by: 01

## Question

There is no middle gear today. A template is a WCL fn returning `list<HtmlFundamental>`, so you
either:

- construct typed variant records — `website_header` in `lib/website.wcl` spends **25 lines of WCL**
  to emit one `<header>` with a brand link and a menu; or
- drop to `HtmlFundamental::Raw { html: <<HEREDOC }` and throw away every guarantee.

The stdlib's own doc comment admits the intended style is the second one: *"paste the design, mark
`${c.content}` / `${wdoc_region(c, "name")}` / `${c.title}`"*. So the escape hatch is the design, and
the typed path is what the stdlib is stuck writing.

Wil's ask, in his words: **"more of a html template system that lets us build html templates and
then render wdoc content into blocks on pages using this html template."**

Decide the authoring model. Prototype the *same real layout* — take the docs website: sticky header
with brand + menu + search + theme toggle, a hero region, content, footer — under at least three
models and react to them side by side:

- **External `.html` files** with a slot syntax, referenced from WCL. Designers/Figma exports/Claude
  artifacts paste in unmodified. WCL keeps the data, HTML keeps the layout.
- **A terse WCL element DSL** — `div(class: "ws-header", [ a(href: home, brand), menu(c) ])` — so
  templates stay in the language and stay typed, but stop being 25 lines per element.
- **Heredoc-with-slots promoted to first-class** — what people already do, but with the slot
  references checked rather than string-interpolated.

What the prototype has to expose, not just look nice:

- How a slot reference is **checked** — a typo in a slot name must not silently render nothing
- Whether the model survives the **book** and **presentation** templates, not just `website`. Book
  needs a sidebar tree, an on-this-page rail, prev/next, and a sidebar footer; presentation renders
  a whole deck at once. A model that only works for marketing pages hasn't answered the question.
- What happens to the **4 existing templates** — do they port, or get rewritten
- Whether **CSS/asset handling** stays as it is (`site.stylesheets` / `scripts` / `fonts`, the
  `assets` folder copy, `HtmlFundamental::Head`) or moves with the template

Blocked by `01-content-seam`: you can't design the syntax that places content without knowing what
content *is* at that point.

Link the prototype from this ticket as an asset.

## Asset

[`proto-02-template-authoring/`](../proto-02-template-authoring/) — the docs-website layout, the
book layout and the deck authored under all three models, plus a runnable Model-A engine
(`python3 run.py`) that renders them and demonstrates the check pass catching five typos that are
silent today. Findings in its [`README.md`](../proto-02-template-authoring/README.md), the
source comparison in [`SIDE-BY-SIDE.md`](../proto-02-template-authoring/SIDE-BY-SIDE.md).

## Inherited from ticket 01 (resolved)

**The template layer needs an expression language.** Ticket 01 decided a template receives the
**authored block tree** and may query it (page-local free; cross-page memoised). A plain `.html` file
with dumb slot markers **cannot query a block tree** — so the "paste the design, mark the holes" model
is ruled out on its own. The target is **Jinja/Handlebars-shaped**: HTML with logic.

This narrows the three prototype models in the question above, but does not settle it — "HTML file with
an expression syntax", "terse WCL element DSL with query builtins" and "heredoc with checked expression
slots" all still satisfy it. Prototype accordingly, and note that Wil's original framing ("build html
templates and then render wdoc content into blocks") is still satisfiable — Jinja templates *are* HTML.

Also settled by 01, so don't re-litigate: placement is by **typed block handles resolved after template
eval** (no re-entrancy; precedent is `WF_CHILDREN_SLOT` / `WF_CONTENT_SLOT` at `render/lower.rs:120,127`),
and the seam is **read-only** — a template reorders by placing handles, not by mutating.

## Answer

**Model B — a terse WCL element DSL.** Templates stay in WCL and stay typed; what changes is that
constructing an element stops costing eight lines. `header(".ws-header", [ a(".ws-brand", { href: … },
[txt(c.title)]), … ])` against today's nested `HtmlFundamental::Element` records.

### The three models were two, then one

**Model C (heredoc with checked slots) collapses into A.** A heredoc cannot loop, so the first
dynamic `<ul>` chops the markup into three fragments — `<ul class="menu">`, the loop, `</ul>` — and
**none is well-formed HTML**. The parse-the-heredoc-and-check-it property that distinguishes C from
today's string concatenation is exactly what is lost at every loop, and at every level of a recursive
one. Add `{% for %}` to fix it and C *is* A with the HTML in a WCL string literal.

**Model A (external `.html` + expression language) was ruled out on cost and need, not on merit.** It
works — the prototype runs, renders all three real layouts, and catches five typos that are silent
today. But it means hand-writing a second language into a repo whose stated conventions are "no parser
generators" and "keep the dependency list minimal": lexer, parser, checker, span-carrying miette
diagnostics, and a `{% %}` dialect that grows forever.

What paid for that cost was the paste-a-design story — Figma exports, Claude artifacts and designer
HTML dropping in unmodified, which is what the `website` template exists for. **Wil ruled it out
explicitly**: *"exact html is not a problem. Can get AI to migrate to WCL or a script."* Transcription
is cheap now. With that gone, A's remaining advantage over B is syntax preference, and syntax
preference does not pay for a second language.

### Checking comes free — the argument the ticket didn't have

**A slot is a symbol, not a string.** `slot(c, :heor)` is a symbol-set violation that WCL already
rejects, with a span, through the existing miette path. No new checker, no new error-reporting plumbing,
no new line-number machinery.

Compare the failure that motivated this ticket: `wdoc_region(c, "heor")` returns `""` and the hero
silently is not there. Model A buys the same guarantee by building a checker (the prototype's
`Checker` class); Model B gets it from the type system that already exists.

### Hugo parity, scoped

Wil's target: *"feature parity with other html generators like hugo but for wcl based docs"* — for
landing pages and eventually a blog.

The tension resolved along the way: **Hugo's template layer *is* Model A.** `layouts/_default/single.html`
with `{{ range }}` / `{{ if }}` / `{{ partial }}` is `{% for %}` / `{% if %}` / macros with different
sigils. So parity is with Hugo's **capabilities**, not its authoring surface — defensible, since Go
templates are widely reckoned Hugo's worst feature and the reason people leave for Astro and Eleventy.

Ruled out of parity by Wil: **i18n**, and **`baseof` template inheritance** — which takes the **theme
system** with it, since a drop-in file-by-file-overridable theme folder leans on templates being files.
Both recorded in the map's Out of scope.

Already present, so not gaps: shortcodes (`wdoc_component` / `wdoc_slot`), menus, output formats,
sections and nesting (`include`).

### The constraint B must carry, and its resolution

Measured in the prototype: three of the book template's load-bearing helpers are **computation, not
layout** — `toc_active` (recursive predicate), `book_pageflow` (recursive flatten), `book_pagenav`
(fold + index arithmetic).

Model A **cannot express any of them** — a macro returns markup, not a `bool` and not a list — so
authoring the book template under A forced them out into context computed once per site. That is
ticket 11's 36%: `book_pageflow` runs **26082 times for 161 pages** because it is called once per page
and re-walks the whole tree each time. **Model A was faster by accident; B removes the guardrail**,
because "just write a fn" is the model and the slow thing stays the natural thing to write.

**Wil's resolution: a builtin that reads page metadata off the TOC without evaluating the whole page.**
Memoised and computed once, Rust-side; **metadata only**, so reading a neighbour's title never forces
its body through lazy evaluation. Templates get prev/next, active-path and reading-order answers from
it instead of hand-walking `c.toc`.

This is enforcement **by construction** — the fast path is the ergonomic path — rather than by rule,
which is what this map's own "prose guidance is not a mechanism" standing preference demands.

### Also decided

- **The `wdoc_part_*` family survives.** Twelve exported composable layout pieces are WCL fns composing
  WCL fns, which is precisely what B is. Under A they had no successor; that was a hidden migration cost
  of A and it disappears.
- **The four existing templates all port.** `webpage`, `website` and `presentation` are mechanical
  shortening of code that already exists. `book` is the one real rewrite, and it is a rewrite because of
  the computation split above, not because of syntax.
- **Site-level asset handling is unchanged** — `site.stylesheets` / `scripts` / `fonts`, the `assets`
  folder copy, `HtmlFundamental::Head`. Those are *site* declarations, not template ones.
- ~~**Template-owned CSS should move to a sibling `.css` file.**~~ **RETRACTED — split out as
  [ticket 13](13-css-authoring.md).** Wil's challenge: why doesn't CSS get the same DSL treatment HTML
  just got? It partly already has — the `class` block (`core.wcl:212`) *is* a CSS-in-WCL DSL, and
  `css-classes.wcl` documents where it stopped ("only rules expressible as a single bare `.name`
  selector with allowlisted properties"). Measured: **13 of `book_css`'s 41 rules are bare `.name`, 28
  are not.** More importantly, a sibling `.css` file forecloses the option that carries *this ticket's
  own winning argument* over to CSS — typed selectors, so a class-name typo is a symbol-set violation
  and an unreferenced rule is dead code. That is not a call to make in passing.

  **RESOLVED by ticket 13 — sibling `.css` files are now ruled out, and typed selectors won, but *not*
  on this argument.** WCL symbols cannot contain a hyphen (`is_ident_cont`, `lexer.rs:592`) and all 237
  class names are hyphenated, so "a class-name typo is a symbol-set violation" is impossible; the check
  became an **output-scan lint at build**. The DSL grows to typed selectors with **raw declarations** —
  the property census (94 distinct, 74 outside the allowlist) plus WCL's missing map type made
  modelling declarations untenable. Note this is the second failure of the *a slot is a symbol* argument
  that won this ticket, after ticket 03 scoped slots per-declarer.

### Known weakness of the chosen model

**The DSL must solve conditional attributes, not just element construction.** B shortens the latter;
roughly half of real template noise is the former, and the prototype's own attempt reads worse than
Model A and arguably worse than today:

```wcl
a(sel_if(true, ".book-chapter", e.current, ".current"), { href: e.href }, [txt(e.title)])
```
against Model A's `class="book-chapter {% if e.current %}current{% endif %}"`. A DSL design that only
addresses element construction buys less than this ticket's line counts suggest.

### Deliberately not decided here

**Which template a page gets.** Wil asked to *"set templates for page types and the like"*. The
mechanism partly exists — `page.template: symbol?` overrides the site's `default_template`
(`lib/core.wcl:167`) — but selection is per *page*, not per type or section the way Hugo picks
`single.html` vs `list.html`. Split out as **ticket 12**.

Status: resolved
