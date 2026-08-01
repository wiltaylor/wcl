# The content seam — what does a template actually receive?

Type: grilling
Status: resolved
Blocked by: —

## Question

`TemplateCtx.content` is a `utf8` of already-rendered HTML. `Region { name: utf8, content: utf8 }`
is the same. A template today receives **opaque HTML strings and pastes them into holes**.

Decide what a template receives instead.

The tension to resolve:

- **Strings are why nothing can be checked.** A template can't inspect content, can't reorder it,
  can't ask "is there a hero here", can't be validated against the page. Every downstream problem in
  this map (slot contracts, layout validation, the editor knowing what it's editing) traces back to
  this field being a `utf8`.
- **But strings are why it's cheap.** The renderer lowers a page body once and hands over the result.
  If a template receives *structured* content it must be able to render it — which means the
  lowering pipeline runs inside template evaluation, and WCL templates gain the power to
  re-enter the renderer.

Options to grill against, not a menu to pick from:

- Structured blocks the template places and the renderer lowers on demand
- A middle form — opaque *handles* a template can place and count but not inspect (checkable
  identity without exposing the block tree)
- Rendered strings kept, but the *addressing* becomes typed (see `03-slot-contract`) — the minimum
  change that buys validation
- Something else the grilling surfaces

**This is the most upstream decision in the map.** The template authoring model
(`02-template-authoring`) and the slot contract (`03-slot-contract`) are both downstream of it, and
the block type system (`05-block-type-system`) shares its currency — the `HtmlFundamental` /
`SvgFundamental` / `TermFundamental` families.

Constraints any answer must satisfy:

- Four backends consume this pipeline: HTML, PDF, Markdown, and the Claude-skill folder. See
  `04-backend-survey` for what each actually needs. HTML is privileged today; the answer should say
  whether it stays privileged deliberately or by accident.
- The browser editor stamps `data-wcl-span` / `data-wcl-file` anchors during rendering
  (`anchor_block`, gated on `InlinePatterns::edit_mode()`). Design mode's entire click-to-edit
  surface depends on rendered output tracing back to source spans. Whatever the seam becomes must
  still carry that provenance.
- Breaking changes are fine. Compat shims are not.

## Answer

**A template receives the authored block tree for its page, plus prepared site context.**

### The three decisions

**1. Content representation — the authored block tree.** A template walks the WCL blocks as
authored: a `callout` with its fields, a `code` with its language, a heading as a heading. Enables
semantic queries ("the h1", "every callout", "first paragraph as a summary").

Rejected: the **lowered fundamentals tree** — querying it is querying HTML by another name ("the
Paragraph whose class is `heading-1`" is the same string-matching as `first_h1_text`, just typed).
Rejected: a **purpose-built third model** — more design work and a third thing to keep in sync with
both the block schemas and the fundamentals.

**2. Placement — typed block handles, resolved after template eval.** The template emits handles; the
renderer resolves them once evaluation is done. **No phase inversion and no re-entrancy** — template
eval never calls back into the renderer.

This was the ticket's stated risk and it turned out to be avoidable, because **the pattern already
exists twice in the codebase** — as magic Unicode sentinels substituted into rendered HTML:
- `WF_CHILDREN_SLOT = "\u{FFF9}wdoc:children\u{FFF9}"` (`render/lower.rs:120`) — wireframe container
  widgets, via `HtmlFundamental::Children { }`
- `WF_CONTENT_SLOT = "\u{FFF9}wdoc:content\u{FFF9}"` (`render/lower.rs:127`) — `wdoc_component`'s
  content slot

U+FFF9 was chosen because it "can't appear in document content, so it can't collide". **The pattern
is proven; the implementation is a hack.** The refactor's job is to make it a typed node, not to
invent a mechanism.

Query access is likewise cheap because it is read-only, and WCL already passes block instances as
values — that is what `project { from = c.body }` and repeater `each = <gather>` do.

**3. Query scope — page-local free, site-level memoised.** A template queries its own page's block
tree freely (O(page)). Cross-page queries are permitted but **must** go through a memoised interface.

This is a direct response to ticket 11's measurement: `wdoc_book_layout` is **20.22s of a 55.4s
build (36%)** because it is called once per page and recursively re-walks the whole toc each time —
`book_pageflow` **26082 calls** for 161 pages. Unrestricted whole-document query access would make
that pathology the default rather than an accident. Forbidding cross-page queries entirely was
rejected as too strict: a template could then never invent a new kind of navigation.

### Consequences (derived, confirmed with Wil)

- **The 14 `TemplateCtx` fields split by the same rule.** Page-local ones become *derivable* and stop
  being pre-extracted in Rust: `on_this_page` loses its dedicated module (`render/headings.rs`), and
  `first_h1_text` (`build.rs:2267`) dies outright — it currently does `html.find("heading-1")`, then
  finds `>`, then `</p>`, then converts HTML back to text, to recover a heading the renderer itself
  just produced. Site-level fields — `toc`, `pages`, `menu`, `home_href`, `deck` — stay **supplied**,
  computed once per site.
- **The seam is read-only.** A template reorders content by *placing* handles in the order it wants,
  not by mutating the tree.
- **A template sees authored blocks, not lowered output.** A user's `my_widget` appears as
  `my_widget`, not as whatever it lowers to.

### Evidence that reframed the ticket

The ticket's premise — "strings are why nothing can be checked" — was **half wrong**. The checking
problem is about *slot names*; content opacity hurts the **renderer**, not the template.

- **No template ever inspects content.** Across all four templates `c.content` appears exactly
  **three** times — `templates.wcl:570`, `templates.wcl:840`, `website.wcl:213` — and every one is
  `HtmlFundamental::Raw { html: c.content }`. Pure paste. The only inspection anywhere is *emptiness*
  tests on regions (`website.wcl:193, 208, 215, 232`).
- **The renderer, however, needs to read content back and can't**, so it works around it two ways:
  pre-extraction into `TemplateCtx` fields (`render/headings.rs`, `html.rs:437`), and string-searching
  its own output (`first_h1_text`).
- **The phase order is strictly one-way today.** `build_normal_page` (`build.rs:2044-2068`) renders
  every block to a string first, partitions into `content` + `regions`, and only then calls
  `render_template` (`html.rs:368`). By the time a template evaluates, the blocks are gone.
- `HtmlFundamental::Element` is **already recursive** (`children: list<HtmlFundamental>`) and
  `Children { }` is already a positional splice marker (`lib/diagram-core.wcl`). The fundamentals
  already carry slot machinery.

### Constraints passed downstream

- **Ticket 02 inherits an expression-language requirement.** A plain `.html` file with dumb slot
  markers cannot query a block tree. The template layer must be Jinja-shaped — HTML with logic — not
  paste-and-mark. This narrows the option space Wil originally described ("build html templates"),
  though it does not contradict it.
- **Ticket 03 inherits a provenance wrinkle.** The edit-mode page wrapper (`build.rs:2079`) wraps the
  single `content` string in a `display:contents` div carrying `data-wcl-page-file` /
  `-name` / `-span`. With multiple typed slots, where that wrapper goes needs deciding. Anchors
  themselves survive — `anchor_block` stamps at render time, which becomes handle-resolve time.

### Deliberately not decided here

Whether `region` survives as the slot mechanism or a queryable page tree makes it redundant. That is
ticket 03's question.

Status: resolved
