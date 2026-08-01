# How is an HTML template authored?

Type: prototype
Status: open
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
