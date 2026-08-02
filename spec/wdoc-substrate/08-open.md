# 08 — Open, deferred and out of scope

Three different things, kept apart on purpose:

- **Out of scope** — ruled beyond this effort's destination. Never graduates; returns only as a fresh
  effort.
- **Deliberately not decided** — inside scope, but left to the implementing ticket because it is
  naming, packaging or a call that wants a real artifact in front of it.
- **Open questions** — real questions the route surfaced and could not yet state sharply. These are the
  candidates for a *next* map.

Plus the incidental defects found along the way, which are real bugs and belong in the tracker.

---

## Out of scope

| | why |
|---|---|
| **Migrating out-of-repo wskills and pages** | Already broken and need migrating regardless; a separate effort after this lands |
| **Backwards compatibility for anyone outside this repo** | A compat shim preserves the exact seam this effort exists to kill |
| **Hand-editing `.wcl` ergonomics** | Not Wil's loop; the browser editor is |
| **Redesigning the PDF and Markdown backends** | They must keep rendering, so they are *constraints on* the new type system — not subjects of it |
| **i18n** | Ruled out by Wil when scoping what "Hugo parity" means. Not a gap to close |
| **`baseof`-style template inheritance** | Same call. A layout does not extend another layout |
| **A theme system** | Follows from the two above — Hugo themes lean on templates being *files* overridable one at a time, and [03](03-templates.md) §3.5 chose WCL templates. Reopening themes means reopening that decision |
| **Sibling `.css` files** | Ruled out on [04](04-css.md), not merely unchosen — note ticket 02 had originally decided *for* them before that was retracted. They buy editor support for free but at 23 files of ~11 lines each they fragment the co-location commit `5ee5d88f` deliberately created, and they check nothing. `code-theme.css` folds in |
| **Full generics in WCL** | [01](01-language.md) §1.3 took **syntax-only** generics because that is all the slot accepts-type needs — the `@children(X)` decorator does the checking. Real generics are a language-design effort in their own right and nothing on the route wants them. A **fresh effort**, not a resumption |
| **Model A, an external `.html` template language** | Ruled out on [03](03-templates.md) §3.5, not merely unchosen. It works — the prototype runs — but it costs a hand-written second language in a repo whose conventions forbid parser generators, and the paste-a-design story that justified it is dead |

---

## Deliberately not decided

Each is owned by the implementing ticket, and each is naming or packaging rather than design.

- **Which of the 34 page-content blocks become content-IR variants vs `@native`.** Follows mechanically
  from the payload-shaped / subtree-shaped test ([02](02-blocks.md) §2.4). Spec work, not a decision.
- **The short union names.** `Html` / `Svg` / `Content` are the obvious picks, but the namespace
  question belongs with [01](01-language.md)'s extraction.
- **The surface syntax of a content-slot fill** beyond `name { … }` / `name? { … }`, and whether `slot`
  needs a namespace prefix in the stdlib (`wdoc_slot` vs bare `slot`). Naming, settled when the stdlib
  is written.
- **What the CSS lint's waiver looks like** — a field on the class block, a build-config list, or a
  naming convention for hook-only classes. Small surface, and it wants designing against a real lint run
  rather than against the 23 hooks found by grep.
- **Where the four new CSS `@block` types sequence** against [02](02-blocks.md)'s implementation. An
  implementation call.
- **Whether the `el` family lives in wdoc's prelude or a separate importable part.** Stdlib packaging.
- **How "is this document wdoc's?" is decided** for migration sweep 6 — the import list, or something
  explicit ([07](07-migration.md) constraint A).
- **CI assembly after consolidation.** All eight recipes' disposition is settled
  ([05](05-wskill.md) §5.3.6); what is left is whether `just ci` calls `check` per wskill or once over
  `docs/wskills/`, where `install --check` sits relative to `just skills-install`, and whether the
  `--deny warn` escalation is on in this repo.

---

## Open questions

Real, in-scope-adjacent, and **not sharp enough to have been ticketed**. Several are explicitly handed
forward by a resolution.

### The blog as a consumer

A blog is a **new build**, not one of the migrations. Ticket 12 cleared most of it: selection needs
nothing (a blog is a `site`), dated collections need nothing (posts are **data**, and
`sort_by`/`group_by`/`take`/`slice`/`unique` all exist in `collections.rs`), taxonomies need nothing (a
repeater over a grouped gather). And [03](03-templates.md) §3.6.4 settled that a list layout may declare
no `content` slot.

**Two genuine gaps remain:**

- **No date handling anywhere in `wcl_lang`** — no `now`, no date type, no parse/format builtin.
  ISO-8601 strings sort correctly, so *ordering* works; *displaying* "1 August 2026" means authoring the
  string or doing string surgery. A small, self-contained builtin question.
- **No mechanism to emit a generated non-HTML output file**, which is what a feed is. The `file` block
  only copies from disk (`file.wcl:23`), and collection templates are HTML-only with no filename
  control. **This one could want its own ticket** — but only once someone actually builds the blog,
  since the shape of the answer (a generalised `file`? a non-HTML target? a second fundamentals
  vocabulary?) depends on how much more than feeds it has to carry.

### WAD beyond substrate fit

This map treats WAD as a **consumer that proves the substrate**. Whatever remains wrong with WAD after
it lands on the new substrate may need its own effort — Wil flagged it as "another area that needs some
work". One concrete residue from the survey: **140 of 252 repeaters (56%) are `if` statements in
disguise**, with discard loop variables. Whether that wants a different primitive is unanswered.

### Whether the four projections stay four

book / ai_skill / training / presentation are separate template sets over one model.
[03](03-templates.md) §3.6.6 removed one way this could have resolved — the slot contract does **not**
collapse them and in fact hard-codes their independence. What remains foggy is whether the projections
want restructuring for their own reasons. Two concrete asymmetries to carry into that question:

- The book gives a body-carrying index its own page while the skill has **no per-kind index pages at
  all** (`skill/main.wcl:9`), so [05](05-wskill.md) §5.1.3 forces the skill to grow pages the book
  already has.
- [03](03-templates.md) §3.8 removes one structural obstacle to a *fifth*: the deck stops being a
  privileged built-in and becomes an ordinary collection template, so a new whole-site-as-one-file
  projection is now declarable rather than a Rust change.

### What replaces `audience` scoping

`Audience` (`:book`/`:ai`/`:both`) plus the `@only`/`@except` visibility system are two overlapping
mechanisms for "which projection renders this". [02](02-blocks.md) §2.7 **loads the `@except` side
further**: its `backends` axis becomes the per-instance waiver for a block used on a target it cannot
render, and gains the missing `:skill` symbol. So `@except` is now doing three jobs — sites,
backends-as-intent, and backends-as-capability-waiver — which makes the overlap worth looking at sooner.

### What Design mode is missing for wskill work specifically

Ticket 10's fifth bullet, returned unresolved — that session spent its fidelity on the audit surface.
Ask again once the audit view, the search box and the curator trigger exist; the answer may be smaller
than it looks, or may be the fuller editor rework Wil flagged (*"we will have to rework editor later I
think"*).

### Whether comment pins scale at volume

[06](06-editor.md) §6.3.4 routes *diff-scoped* findings to row tags, which sidesteps it for the audit
view — but the standing surfaces (graph, content modal) still have to show findings on units nobody
just touched, and `comments.wcl` with `author = "curator"` is where they live. Sharp enough to ticket
only once the lint rule set is producing real volume on a real wskill.

---

## Incidental defects — file these separately

Real, actionable, and **not** part of this effort. Nothing here is a decision.

1. **`node_table` in a diagram loses all row text in PDF.** `card_rects` counts every `foreignObject`
   while `collect_card_blocks` counts only `card` blocks, so the counts disagree and every box renders
   empty. *(04)*
2. **No `wad-template-check`, and WAD's scaffold copy has drifted.** All 14 WAD projection files are
   duplicated into `crates/wcl/src/scaffold/templates/wad.wcl` with **no drift gate** — 11 of 14 are
   byte-identical; **3 have drifted**, including a `routing = :straight` CI-failure fix stranded in the
   live copy and absent from every newly-scaffolded WAD. **`wcl init wad` currently scaffolds a document
   with a known CI failure in it.** *(11)*
3. **`crates/wcl_wdoc/src/kinds.rs:1-8` is factually wrong** — it states the three backends
   "special-case the same block vocabulary"; three of its own constants are referenced by a strict
   subset. *(04)* — [02](02-blocks.md) dissolves most of `kinds.rs`; fix only if the refactor slips.
4. **CLAUDE.md's wdoc feature map misdescribes `code` and `math`.** It lists them among the blocks
   "special-cased in Rust with stub WCL `lower`s"; both have *real* lowers emitting `Highlighted` /
   `Math` leaf variants. *(05)*
5. **`crates/wcl_wdoc/lib/tui.wcl:33` states a language rule that isn't true** — *"Every field must be
   supplied at construction (set `none` to opt out)"*. Optional variant fields already default to
   `none`; the comment has cost the stdlib 204 dead arguments. *(05)*
6. **The `Index` schema doc-comment is stale.** `docs/wskills/*/schema/base.wcl` claims an index
   "renders as a link-collection page" in the skill; `skill/main.wcl:9` says *"There are NO per-kind
   index pages"*. *(06)*
7. **`entity wil_taylor` is triplicated** across the wcl, wad and wskill wskills with identical content,
   with nothing keeping the three copies in sync. *(06)*
8. **`crates/wcl_wdoc/src/render/css.rs:1-13` is factually wrong** — it states *"The lone Rust-side CSS
   that remains is `highlight::theme_css()`"*; `theme.rs`'s `APPLY` is ~84 hand-written rules. *(13)* —
   [04](04-css.md) dissolves this along with the CSS it misdescribes; fix only if the refactor slips.
9. **Fn signatures are second-class: argument types are never checked, and `?` is a parse error in a
   parameter list.** `elem("div", none, none, [])` evaluates clean against parameters declared
   `identifier` and `list<utf8>` — the annotations are documentation, not constraints (arity *is*
   checked). There are **zero** optional fn params in the stdlib. *(15)* — **If argument checking ever
   lands, revisit [02](02-blocks.md) §2.10.3: it is half of why SVG kept the named literal.**

**Already resolved by this spec — do not file:** the 6 dangling `related` ids
([05](05-wskill.md) §5.3.4) · reciprocal edges rendering twice ([05](05-wskill.md) §5.1.4) ·
`file`-in-PDF's undefined intent ([02](02-blocks.md) §2.7) · the duplicated `.tok-*` definitions
([04](04-css.md) §4.6) · the `//` CSS footgun ([04](04-css.md)).
