# An honest block type system — what replaces WdocBlock / SvgBlock and the 24 stub lowers?

Type: grilling
Status: open
Blocked by: 01, 04

## Question

The declared type system and the real one have diverged. Measured:

- **134** `@block` types. 43 `extends WdocBlock`, 30 `extends SvgBlock`.
- **24** declare a stub `lower` returning `[]` **while Rust intercepts them entirely** — terminal,
  card, node_table, tree, timeline, tilemap, dopesheet, map, wireframe widgets, icons, math, code,
  table, list. The declaration is a lie told to satisfy an interface.
- **5** `lower_svg` fns exist purely because a page-level block that draws SVG must still satisfy
  `WdocBlock` (whose `lower` returns HTML fundamentals), so its geometry has to hide in a second fn.

The mechanism itself is good and worth preserving: an unrecognised block kind dispatches to a WCL
`<kind>_lower` returning fundamentals, which the renderer recurses until only fundamentals remain.
That's what makes user-declared `@block(...) extends SvgBlock` shapes plug in — the wireframe `wf_*`
family, and the WAD Systems view's schema-derived editing, both depend on it.

Decide:

- **How does a Rust-implemented block declare itself truthfully?** A `@native` marker? A declared
  capability the renderer checks? Something that makes "Rust owns this one" a fact in the schema
  rather than a stub plus a match arm. The 24 aren't going away — calendar math, ANSI grids, LaTeX,
  syntax highlighting, measured widget layout and valid nested-list HTML genuinely aren't expressible
  in WCL — so the goal is honesty, not elimination.
- **How does a block declare which targets it can lower to?** One `lower` per target? A target-keyed
  set? Today it's one HTML-shaped `lower` plus ad-hoc second fns, which is why `lower_svg` exists.
- **Does the `WdocBlock` / `SvgBlock` split survive?** Its real content is "page-level vs
  diagram-child", which is about *placement*, not about what the block renders to. Those may be two
  different axes wrongly collapsed into one.
- **What happens to blocks a backend can't render?** Degradation is scattered per-block today
  (`demo`, `edit_object`, `markdown_source` each handle it differently). Does the type system carry
  it?

Constraints:

- **`TypeDecl::is_descendant_of` is already load-bearing.** `/api/palette` derives `diagram_kinds` from
  every `@block` descending from `wdoc.SvgBlock`; the WAD Systems view derives its whole model from
  schema introspection (`kind_links`), and the wireframe palette reads a schema-derived
  `accepts_children`. The editor's schema-driven UI is built on this hierarchy — changing it changes
  the editor.
- `04-backend-survey` establishes what each backend needs. Read it first.
- `01-content-seam` fixes what a template receives; the fundamentals are the shared currency.

## Inherited from tickets 01 + 04 (both resolved)

**From 04 — the root cause is narrower than this ticket assumed.** `lower_recurse` exists **only in
HTML** (`render/lower.rs:317`). That single asymmetry explains the per-backend divergence: `callout` has
a real WCL `lower`, so HTML follows it and needs no special case, while PDF and Markdown hand-reimplement
it in Rust. **Consequence: the WCL extension mechanism is HTML-only in practice** — a user block whose
`lower` returns another custom variant works in the book and silently renders nothing elsewhere. Fixing
recursive lowering across backends may resolve more of this ticket than redesigning the hierarchy does.

Corrected counts: **57** stub `lower`s (24 `HtmlFundamental` + 33 `SvgFundamental`), not 24; **2**
`lower_svg` fns, not 5.

**From 11 — the concrete breakage risk.** The editor's schema introspection (`kind_links`,
`blocks.rs:1526-1579`) tests types by **string equality on printed names**: `bare_type(f) == "identifier"`
/ `== "list<identifier>"` (`:1542,1558`), `to_string().starts_with("fn")`,
`full_name().starts_with("wdoc.")`. **Several fail silently** — reclassify a field and it just stops being
a parent link, no error. Any hierarchy change must account for this or the WAD Systems view degrades
quietly.

**From 01 — a template sees authored blocks, not lowered output.** So the block tree and the fundamentals
are now two distinct consumer-facing surfaces: templates walk the authored tree, backends consume
fundamentals. That may relieve pressure on the fundamentals to be expressive.
