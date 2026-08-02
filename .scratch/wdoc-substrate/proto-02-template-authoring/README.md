# PROTOTYPE — ticket 02: how is an HTML template authored?

**Throwaway.** Answers one question, then gets archived to a branch.

```bash
python3 run.py          # no deps; renders to out/ and prints the check report
```

The same **real** layouts, authored three ways:

| | Model A — external HTML + expressions | Model B — terse WCL element DSL | Model C — heredoc with checked slots |
|---|---|---|---|
| docs website | [`model-a-html/website.html`](model-a-html/website.html) | [`model-b-wcl-dsl/website.wcl`](model-b-wcl-dsl/website.wcl) | [`model-c-heredoc/website.wcl`](model-c-heredoc/website.wcl) |
| book (hard case) | [`model-a-html/book.html`](model-a-html/book.html) | [`model-b-wcl-dsl/book.wcl`](model-b-wcl-dsl/book.wcl) | *(see the note in website.wcl — it doesn't reach the book)* |
| deck (hard case) | [`model-a-html/presentation.html`](model-a-html/presentation.html) | — | — |
| a typo'd copy | [`model-a-html/website-typo.html`](model-a-html/website-typo.html) | — | — |

Only model A is executable — it's the one with a new mechanism to prove.
B and C are near-neighbours of today's WCL and are judged as source.

Read [`SIDE-BY-SIDE.md`](SIDE-BY-SIDE.md) for the same fragment in all three.

---

## What the run proves

```
── website.html      check passed   rendered 1494 bytes
── book.html         check passed   rendered 3250 bytes
── presentation.html check passed   rendered  938 bytes
── website-typo.html CHECK FAILED (6)
    website-typo.html:18: `site` has no field `titel` — has: deck, footer, home_href, …
    website-typo.html:22: `p` has no field `label` — has: href, name
    website-typo.html:27: unknown slot `heor` — declared: content, footer, hero
    website-typo.html:31: unknown filter `frist_of` — known: count, default, first_of, …
    website-typo.html:35: macro `credit` takes 1 argument(s), given 2
```

Every one of those is silent today. `wdoc_region(c, "heor")` returns `""` and
the hero simply is not there.

The book output has a working recursive TOC tree with `current` highlighting
and mdbook-style active-path expansion (`<li class="book-branch open">`), so
the hard case isn't hand-waved.

---

## Findings

### 1. The three models are not three models. B and C collapse.

**Model C collapses into A.** A heredoc cannot loop, so the first `<ul>` with
a dynamic list chops the HTML into three fragments — `<ul class="menu">`,
loop, `</ul>` — and **no fragment is well-formed HTML**. The one property
that makes C better than today's string concatenation (parse the heredoc,
check it) is exactly what you lose at every loop. Add `{% for %}` to fix it
and C *is* A, with the HTML in a WCL string literal instead of its own file.

**Model B is orthogonal, not alternative.** It shortens element construction
(the 25-line `<header>` becomes 8) but does nothing for the conditional
attributes where half the real noise lives — `sel_if(true, ".book-chapter",
e.current, ".current")` reads worse than `class="book-chapter {% if e.current
%}current{% endif %}"`. B is a fine cleanup of the *fundamentals*
constructors, and those don't go away under A: `HtmlFundamental::Element` is
still what blocks lower to. **B belongs to ticket 05, not this one.**

So the real question is narrower than the ticket assumed: **is the template
an external file, or a WCL heredoc?** And C answers it by collapsing.

### 2. Line counts, layout code only (CSS heredocs excluded)

| template | today's WCL | model A | |
|---|---|---|---|
| `website` | 92 | **45** | |
| `book` | 181 | **57** | but see finding 3 — computation moved out |
| `presentation` | 45 | **20** | |

### 3. ⚠ The book template's cost is COMPUTATION, and the model A constraint deletes it

This is the finding I did not expect, and it is the one that matters most.

Three of the book template's load-bearing helpers are not layout:

- `toc_active(e)` — recursive predicate, "is the current page in this subtree"
- `book_pageflow(toc)` — recursive flatten of the tree into reading order
- `book_pagenav(toc)` — fold to find the current index, then index ±1

Model A **cannot express any of them**: a macro returns markup, not a `bool`
and not a list. So writing the book template under model A forced them out of
the template and into supplied context — `e.active` on the `TocEntry`,
`page.prev` / `page.next` on the page.

That is exactly ticket 11's 36%. `book_pageflow` runs **26082 times for 161
pages** because it is called once per page and re-walks the whole tree each
time. Computed once per site, it runs once.

**Model B keeps the pathology available** — "just write a fn" is the model, so
the natural thing to write is the thing that is slow. Model A's *limitation*
produced the faster design by accident. A template language that is
deliberately not a computation language is a performance feature, not just an
ergonomic one.

The corollary, which ticket 03 will need: the split isn't "HTML in the file,
data in WCL". It is **layout in the template, computation in WCL** — and
"computation" turns out to mean anything recursive or index-arithmetic.

### 4. The deck was not a hard case after all

"Presentation renders a whole deck at once" sounded like it would break any
per-page model. It's a two-level `{% for %}` (20 lines). Deck-ness lives in
the *data* — `site.deck` is resolved site-level context and each slide's
content is a block handle exactly like a page's. Nothing in the template
language has to know decks exist.

### 5. CSS moves to a sibling file, and a documented footgun dies

Today CSS is a WCL heredoc emitted verbatim into a body `<style>`. Both
`templates.wcl:710` and `presentation.wcl:106` carry the same shouted warning:

> IMPORTANT: emitted verbatim into the page `<style>`, so it must be pure CSS
> — never a `//` line (CSS has no `//` comment; a stray one swallows every
> rule after it).

Under model A a template is a small **folder** — `book.html` + `book.css` —
and the CSS is CSS in a `.css` file. The warning becomes unnecessary rather
than better-documented.

Site-level assets don't move: `site.stylesheets` / `scripts` / `fonts`, the
`assets` folder copy and `HtmlFundamental::Head` all stay as they are. They
are *site* declarations, not template ones. (`Head` may not even need to
survive as a fundamental — a template folder can just declare its head
assets — but that's ticket 03's contract question.)

### 6. What happens to the four existing templates

All four port, and they get smaller. `webpage` is trivial, `website` and
`presentation` are near-mechanical, `book` is the only real rewrite — and it
is a rewrite because of finding 3, not because of syntax.

The other 4→N question: template folders being *files* rather than WCL `let`s
means a user's own template is a folder they can copy, and the "copy the
stdlib template and swap parts" story stops needing `wdoc_part_*` exports.
The whole `wdoc_part_*` family — 12 exported functions whose only purpose is
to be composable pieces of a WCL layout fn — has no obvious successor and
probably shouldn't get one.

---

## Open, for Wil

1. **Is model A right, given B and C collapse?** The prototype argues yes, but
   it argues it by elimination as much as on merits.
2. **How far does the expression language go?** I stopped at
   `if` / `for` / `macro` / filters / field access, and finding 3 says that
   limit is a *feature*. But it means every "the template needs to compute X"
   becomes a change to the supplied context — i.e. a Rust change. Is that
   acceptable friction, or does it recreate `render/headings.rs`?
3. **Whose filters?** `first_of` / `where` / `text` / `count` are the query
   surface ticket 01 promised. That set is the block-tree query language and
   it needs a real design; here it's four filters chosen to make the rail work.
4. **Does the editor's Design mode want the template as a file?** External
   HTML files are much easier to show and edit in the browser editor than WCL
   `let` bindings that build variant records. Possibly a bigger win than the
   authoring ergonomics — and ticket 10's territory.
