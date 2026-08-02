# Break wdoc out of `wcl_lang` — what replaces the hardcoded block names?

Type: grilling
Status: resolved
Blocked by: 05

## Question

`wcl_lang` is the *language* crate. It hardcodes its consumer's vocabulary: `wdoc_component`,
`wdoc_slot`, `wdoc_body`, `wdoc_repeater`, `wdoc_instance`, `wdoc_content` appear **by name** in the
language's document model, view layer and schema checker. Wil's call on ticket 03: this ends.

That decision is already load-bearing rather than aesthetic. Ticket 03 put the slot check at
`wcl wdoc build` rather than `wcl check` **because** wdoc concepts are leaving the language crate —
once they do, `wcl_lang` cannot check a wdoc slot contract even if it wanted to.

### The measured seam (established on ticket 03 — don't re-derive)

**109** raw `wdoc` hits in `crates/wcl_lang/src/`, but only **12 are live code outside tests**. The
rest are doc comments, one lexer test string (`lexer.rs:1146`) and a docstring example
(`reflect.rs:63`). Two clusters:

**Components / slots**
- `doc.rs:2043` — `component_def`: find the `wdoc_component` whose name matches an instance kind
- `doc.rs:2157` — component-name-vs-registered-block-kind collision check
- `views.rs:2884,2895` — bind each `wdoc_slot` to the instance field / default, expand `wdoc_body`
- `schema_check.rs:374` — `validate_component_instance`'s slot list

**Repeaters / instances**
- `views.rs:2813` — `wdoc_repeater` / `wdoc_instance` are context-polymorphic
- `views.rs:2834,2854` — their expansion
- `schema_check.rs:787` — `wdoc_repeater` / `wdoc_instance` / `wdoc_content` emit-anywhere exemption

Supporting machinery that is *already* generic and names nothing: `scope.rs` (renderer-injected
bindings), `cells.rs` (body expansions), `eval.rs:1218`.

## Decide

- **What is the generic facility?** Candidates: a decorator vocabulary a consumer's schema uses to
  mark "this block type declares slots" / "this block type is context-polymorphic" / "this block type
  expands a body under bindings"; a registration API on `Document`; a trait the consumer implements.
  The decorator route is the most WCL-shaped — the language already has `@block` / `@children` /
  `@schemaless` — and would let wdoc declare these properties in `lib/*.wcl` rather than in Rust.
- **Does the slot contract become a language feature?** Ticket 03 unified layout slots and component
  slots into one typed `slot` concept. If slot declaration and the both-directions check generalise,
  `validate_component_instance` stops being wdoc-specific and the biggest cluster dissolves. If they
  don't, wdoc reimplements a checker it currently gets for free — cost to state honestly.
- **Where does the check that ticket 03 moved to build time actually live?** In `wcl_wdoc`, as part of
  the build; or in `wcl_lang` as a generic facility wdoc *invokes* with resolved (declarer, filler)
  pairs. The page→layout pairing rule (`page.template ?? site.default_template`, and whatever
  ticket 12 adds) is irreducibly wdoc's.
- **What about the context-polymorphic exemption?** `schema_check.rs:787` exempts repeaters and
  component instances from placement checks because they "emit anywhere". That is a genuine language
  concept wearing wdoc names — decide what it is called when it's generic.
- **Does anything else ride out with it?** `error.rs:301` (component-name collision), the
  `reflect.rs` docstring, the lexer test string.

## Constraint

`wcl wdoc build` must still catch everything ticket 03's severity table says it catches, and the
component slot check must not regress — it works today and is verified
(`field 'labell' is not a slot of component 'metric_card'`).

Blocked by `05-block-type-system`: the extraction seam has to express whatever the block type system
becomes, so designing it first would be designing against a moving target.

## Inherited from ticket 03 (resolved)

Layout slots and component slots are **one typed `slot` concept** — `slot hero: content`,
`slot shapes: content<SvgBlock>`, optional via `?`, defaulted via `= …`. `wdoc_content` is subsumed.
Slots are `@children("slot")` on their declaring block (`template` or `wdoc_component`), so they scope
to their declarer and nothing resolves a slot name globally. Slot **references** (`slot(c, :hero)`)
resolve at render, not statically — 02's "symbol sets check it for free" claim does not survive
per-declarer scoping.

## Inherited from ticket 05 (resolved)

**You are now unblocked.** The type system 05 landed, in the terms your extraction seam has to express:

- **Three block interfaces, one per output IR** — `ContentBlock` → a closed semantic **content IR**,
  `SvgBlock` → `SvgFundamental`, `TermPrimitive` → `TermFundamental`. Placement is no longer in the
  interface; it rides on the slot's accepts-type (`slot shapes: content<SvgBlock>`), which is *your*
  question 2 — the accepts-type is a set of block types in a **field-type position**, so whatever
  generic slot facility you land has to name block types there, not merely in an `extends` clause.
- **`lower` is no longer a universal interface field.** A block declares either a `lower` or `@native`,
  and **wdoc build-checks exactly one**. That check is one more thing living outside `wcl_lang` — add
  it to whatever the answer to "where does the build-time check live" turns out to be.
- **`@native(html, markdown, …)` declares backend coverage**, cross-checked against the Rust dispatch
  registry; use on an uncovered target is a build error waived by `@except(backends = […])`. All of
  that is wdoc vocabulary and all of it is build-time — more weight on the seam you are designing.

**05 took decision 10 — the typed introspection API is now in scope somewhere, and it is your
dependency.** `TypeField::shape() -> FieldShape::{Scalar(prim), List(prim), Fn, Block, …}` replaces the
editor's string comparisons on printed type names. The one that matters to *you* specifically is
`full_name().starts_with("wdoc.")` (`blocks.rs`): **your extraction reshuffles exactly the namespace
that string reads**, so if the introspection is still Display-based when wdoc leaves `wcl_lang`, the
WAD Systems view degrades silently. Whether the API lands in this ticket or in 05's implementation is
a sequencing call — but it cannot land *after* the extraction.

**One cluster may have grown.** 05 decision 8 made splicing **structural** — the renderer fills slot
markers as tree nodes, deleting `HtmlFundamental::Children` (0 users), both U+FFF9 sentinels, and the
three hand-written component-content-slot implementations (`html.rs:820`, `pdf/collect.rs:165-217`,
`markdown/emit.rs:224-262`). `wdoc_content` is gone as a block kind, which removes it from your
`schema_check.rs:787` exemption list; the slot-filling itself becomes renderer-side and generic, which
may make your "does the slot contract become a language feature?" question easier to answer yes to.

**Not a constraint, but useful:** the content IR's Rust enum is **generated from its WCL union** in
`wcl_wdoc/build.rs` (05 decision 11). If your generic facility needs a Rust-side view of a
consumer-declared vocabulary, that codegen is the precedent — WCL is the source of truth and Rust is
derived from it.

---

## Resolution

**The 12 sites are three clusters, and they get three different answers: cluster A+B are *deleted*
(they dissolve into the generic validator), cluster C splits into metadata + a callback.**

### Measured before deciding

- **`component_def` is public and used 9× in `wcl_wdoc`** — `render/expand.rs:40,87,417`,
  `build.rs:1299`, `tree.rs:83`, `node_table.rs:82`, `html.rs:620`, `pdf/collect.rs:189`,
  `markdown/emit.rs:237`. Not an internal detail; an API wdoc leans on.
- **The language's expander is a deliberate simplified mirror of wdoc's.** `views.rs:2824-2902`
  (87 lines) vs `wcl_wdoc/src/render/expand.rs` (431 lines). Its own doc comment: *"Mirrors the
  renderer-side expansion … kept minimal for the projection path: an erroring `each` or slot value
  contributes nothing here."* Two implementations of one semantics, drifting **by design**.
- **`partial` / `collect` are the control case.** wdoc generators declared in `components.wcl`
  beside the others, appearing in `wcl_lang` **zero** times — they get by as ordinary
  `@children(WdocBlock)` blocks whose expansion is purely renderer-side.
- **Child-kind checking reads the `@children(X)` decorator argument, not the field type**
  (`schema_check.rs:742-770` — `allowed_child_kinds`, `union_slots`, `interface_slots` doing
  `is_descendant_of`). The field type `list<WdocBlock>` is decorative for that purpose. This is what
  makes decision 3 cheap.
- **`TypeRef` has no generics** (`value.rs:534` — `Builtin | Named | Reference | List | Tensor |
  Function`), so 03's `slot shapes: content<SvgBlock>` was **not expressible**. **36**
  `TypeRef::Named` sites, **all inside `wcl_lang`** — no consumer crate constructs or matches it.
- **`wcl check` opens with a plain `Environment::new()`** (`main.rs:1383`); the wdoc registry is
  supplied separately as a loader (`main.rs:33`). This is what makes decision 5 a live hazard rather
  than a theoretical one.
- **`crates/wcl` carries 8 hardcodes** of these block names (`editor/blocks.rs:1686,1699`,
  `editor/nav.rs` ×3, `editor/mod.rs`) — uncounted by this ticket's charting. Legitimate: the editor
  is a wdoc *consumer* and is allowed to know wdoc.

### Decisions

**1. The generic facility is a language-owned decorator vocabulary, consumer-applied.**
Two decorators, `@declares_kind(name = 0, params = "slots", body = "body")` and `@contextual`, whose
*names* belong to the language and whose *use* belongs to the consumer. wdoc puts them on its own
types in `lib/components.wcl`.

This is new ground and should be documented as such: WCL's existing consumer-declared decorators
(`@only`, `@except`, `@file`, `@editable`, `@answerable`) are **inert** — the language parses them and
the consumer reads them back. These are the first where the language changes *its own* behaviour on a
decorator. Rejected: a registration API on `Environment` (splits the property from the `@block`
declaration it describes, and a document opened without the right Environment silently mis-validates)
and a consumer-implemented trait (heaviest seam, `Document` gains a `dyn` parameter every caller pays
for). Note decision 4 brings back a *narrow* form of the trait route for behaviour, which a decorator
structurally cannot carry.

**2. Clusters A and B dissolve into the generic validator — `validate_component_instance` is deleted,
not moved.**
`@declares_kind` makes `block_schema(kind)` fall back to a **lazily derived** schema built from the
declarer's params:

```
block_schema("metric_card")
  → no @block type found
  → consult the @declares_kind index
  → found `wdoc_component metric_card`
  → derive TypeDecl { label: utf8, value: utf8, status: utf8? }
  → generic UnknownField / MissingRequired apply
```

Component instances become ordinary registered kinds. The two checks `validate_component_instance`
(`schema_check.rs:358`) hand-rolls — every instance field names a declared slot; every defaultless
slot is supplied — are *exactly* `UnknownField` and `MissingRequired`, so the generic validator
already emits them. `component_def` / `is_component_kind` / the `schema_check.rs:440` fallback / the
`error.rs:301` collision path all route through the derivation instead.

**This is what protects the ticket's stated constraint.** `wcl check` keeps catching
`field 'labell' is not a slot of component 'metric_card'` — for free, and now for any consumer that
uses `@declares_kind`, not just wdoc.

The cost, stated honestly: `synthetic_types` is fixed at construction and these are not, so the
derived `TypeDecl`s need lazily-built owned storage on `Document` (a `OnceLock` arena alongside
`component_index`, which the derivation subsumes). This is the real implementation work in the ticket.

Rejected: keeping a de-hardcoded `validate_component_instance` reading its param-field name from the
decorator (smaller, but leaves the language with a bespoke second validator whose only user is wdoc —
and it is honestly a *slot* checker, so ticket 03's one-concept-two-checkers problem would stand), and
moving the check to wdoc entirely (would regress `wcl check`, which the ticket forbids).

**3. `TypeRef` grows generics — syntactically only.**
`TypeRef::Named { path: Vec<String>, args: Vec<TypeRef> }`, args defaulting empty so the 36 existing
sites become a one-line pattern change. The parser accepts `Path<A, B>`, the printer round-trips it,
serde carries it. **No arity check, no substitution, no `type Foo<T>` declaration form.** Consumers
read `args` as metadata.

This is enough because the derivation emits *both* a typed field and a decorator, and **the decorator
does the checking**:

```
slot shapes: content<SvgBlock>
  ⇓ derives
@children(SvgBlock) shapes: content<SvgBlock>
//  ↑ does the checking      ↑ carries the intent
```

`is_descendant_of` on the interface already handles the accepts-check (`schema_check.rs:803-808`).
So 03's readable surface and 05's "the accepts-type is a set of block types in a **field-type
position**" both land with zero type-system semantics.

Cost accepted: `content<Nonsense>` parses and only fails later, at the `@children` resolution or at
build. Rejected: declared arity via `type Content<T>` (catches that, but generic params become names
that must resolve inside a declaration body — the thin end of full generics), and full generics
(a language-design effort in its own right; if it is ever wanted it is **its own map ticket**, not a
fold-in here).

*Note this reverses the recommendation on the table.* The recommended answer was to desugar `content`
→ `list<WdocBlock>` and `content<SvgBlock>` → `list<SvgBlock>` for zero language change. Wil took
generics instead, then scoped them to syntax-only — so the language grows a `TypeRef` variant it
would not otherwise need, in exchange for `content` remaining a first-class spelling rather than
sugar over `list`.

**4. Cluster C splits: the exemption is metadata, the expansion is a callback.**
- **Exemption** — `@contextual` on the type. `schema_check.rs:787`'s hardcoded
  `"wdoc_repeater" | "wdoc_instance" | "wdoc_content"` becomes a decorator lookup, carrying both
  halves of what that site does today: the block is legal wherever children are allowed at all, and
  its body is not recursed into. (`wdoc_content` drops off the list regardless — 05 killed it.)
- **Expansion** — an **expander callback registered on `Environment`**, consulted when the language
  projects `@children`. A decorator can declare *that* a block expands; it cannot carry *how*
  ("iterate `each`, bind each element to the symbol named by `as`"; "bind slot names to instance
  fields, falling back to each slot's `default`") — that is behaviour.

This **deletes the 87-line mirror** at `views.rs:2824-2902` (`generator_children`,
`expand_component_body`) and makes projection *more* correct than today: wdoc's real expander records
errors where the mirror silently contributes nothing.

Rejected: a richer `@expands(over = "each", bind = "as", body = "children")` decorator — fully
declarative and consistent with decision 1, but it is a mini-language for expansion semantics and,
decisively, **the duplicate implementation survives it**, parameterised but still able to drift. Also
rejected: dropping expansion from the language entirely, which re-breaks the bug `views.rs:2749` was
written to fix (data-driven children inside a custom shape vanishing from `@children` projections).

**5. A missing expander is an error, not a silent degradation — and it fires on demand.**
Projecting a `@contextual` block's generated children with no registered expander is a hard error
naming the missing expander. **The error fires when the generated children are actually demanded, not
when the document is opened** — so `wcl parse`, `wcl fmt` and any generic AST reader stay safe, while
`wcl get` / `eval` and the LSP on a wdoc document fail loudly until they are given the wdoc
Environment.

Cost accepted deliberately: the CLI's generic paths must learn to supply the wdoc Environment when a
document imports `<wdoc.wcl>` — `main.rs:1383` and the LSP/editor open paths. That is CLI work this
decision *forces*; see the map's migration note. Rejected: silent degradation (the language would
carry a mode where the right answer is quietly unavailable) and binding the expander to
`wcl_wdoc::schema_registry()` (that is threaded as a *loader* today, so it would mean reshaping the
CLI's wdoc-awareness inside this ticket).

**6. The template slot check is wdoc's; only the derivation is shared.**
`wcl_lang` exposes the declarer → schema derivation as public API. **Components ride the ordinary
block path automatically** (decision 2). **Templates do not**: wdoc resolves the
`page.template ?? site.default_template` pair, derives the layout's slot schema through that same API,
and applies all six of ticket 03's severity rows itself.

The two filler relationships are **structurally different**, which is why two call sites is honest
rather than duplicative: a component instance *is* a block of the declared kind, so ordinary machinery
applies; a page is *not* a block of the template's kind — it is paired by a wdoc rule. And **four of
03's six rows are irreducibly wdoc's**: `?`-conditional fills, the site-wide typo check ("a
conditional fill naming a slot no layout in the site declares is an error" — needs the union of every
layout in the site), double-fill, and the accepts-type check. Only "unknown fill name" and "required
slot unfilled" are the generic pair.

Rejected: a `check_fills(declarer, filler_fields)` API in `wcl_lang` — marginally more shared code,
but the language would gain an API whose only caller is wdoc, and the split between "generic rows" and
"wdoc rows" is arbitrary enough to invite drift.

**7. Naming: `@declares_kind` / `params` / `@contextual`.**
These go in the `wcl` wskill as **language features**, so they read as language concepts rather than
renamed wdoc ones. `@declares_kind` says exactly the unusual thing — a kind comes from an *instance*,
not a type. `params` is the neutral word for what a declarer takes, which leaves `slot` free as
wdoc's own spelling (03's concept survives at wdoc's layer). `contextual` says placement is decided by
context rather than by kind.

Rejected: `defines_kind`/`fields`/`generator` (reuses words the codebase already says, but `fields`
collides with the ordinary `Block::fields` sense and `generator` reads as "produces values" rather
than "placement is contextual"), and `kind_template`/`params`/`emits_anywhere` (most literal, but
`template` is already taken — hard — by wdoc's layout block).

**8. Riding out with it.**
- `error.rs:301`'s component-name collision **survives, generically reworded**: a kind declared by a
  `@declares_kind` instance colliding with a registered `@block`/`@table` kind is a real generic
  error. Only the wording changes.
- `reflect.rs:386,435` docstrings and `lexer.rs:1146`'s test string get neutral examples.
- `doc.rs:86`'s `component_index` becomes the `@declares_kind` index; the `doc.rs:1116`,
  `schema_check.rs:223-224`, `views.rs:810,970,2051,2077,2457,2749`, `eval.rs:1218-1219`,
  `cells.rs:84,192`, `scope.rs:23`, `parser/decls.rs:1135` comments get rewritten in the new
  vocabulary. `scope.rs` / `cells.rs` / `eval.rs:1218` need **no code change** — they were already
  generic and name nothing.

**9. No ordering constraint from 05's introspection API — and one new constraint *onto* it.**
Ticket 05 warned that this extraction "reshuffles exactly the namespace" `blocks.rs:1460`'s
`full_name().starts_with("wdoc.")` reads, and therefore that `TypeField::shape() -> FieldShape` could
not land after it. **Checked: that premise does not hold.** This seam leaves wdoc's types in
`namespace wdoc` in `crates/wcl_wdoc/lib/*.wcl` — only the Rust hardcoding moves. The strings that
actually break are broken by **03 and 05's renames** (`wdoc_slot` → `slot`, `wdoc_content` and
`Region` dying), so the API belongs with the work that breaks them.

**This ticket hands 05's implementation a constraint instead: derived schemas (decision 2) must be
reachable through whatever `FieldShape` API 05 lands.** A component kind's schema will not be in
`type_decls()`, so anything introspecting by walking declarations will not find it — including the
editor's palette, which already special-cases `wdoc_component` at `blocks.rs:1686` and will need to
route through the derivation instead.

## Corrections to the map

- **Ticket 05's claim that this extraction reshuffles the `wdoc` namespace is wrong** (decision 9).
  The Display-based-introspection risk is real but rename-shaped, and belongs to 03/05.
- **The entanglement is 12 sites in `wcl_lang` *plus* 8 in `crates/wcl`** — `editor/blocks.rs:1686,1699`,
  `editor/nav.rs` ×3, `editor/mod.rs`. Ticket 03's survey counted only the language crate. The editor's
  are legitimate (it is a wdoc consumer) but they will need updating by 03/05's renames, so they are
  migration surface.
