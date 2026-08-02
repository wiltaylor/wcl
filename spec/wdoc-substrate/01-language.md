# 01 — Language (`wcl_lang`)

Source: ticket [14](issues/14-wdoc-lang-extraction.md), with items from
[15](issues/15-constructor-dsl.md) and [05](issues/05-block-type-system.md).

`wcl_lang` is the *language* crate and it hardcodes its consumer's vocabulary. `wdoc_component`,
`wdoc_slot`, `wdoc_body`, `wdoc_repeater`, `wdoc_instance` and `wdoc_content` appear **by name** in the
document model, the view layer and the schema checker. This part removes them and adds the four
language capabilities the rest of the spec needs.

**Measured seam** (don't re-derive): 109 raw `wdoc` hits in `crates/wcl_lang/src/`, of which **12 are
live code outside tests**. The rest are doc comments, one lexer test string (`lexer.rs:1146`) and a
`reflect.rs:63` docstring example. `crates/wcl` carries **8 more** (`editor/blocks.rs:1686,1699`,
`editor/nav.rs` ×3, `editor/mod.rs`) which are *legitimate* — the editor is a wdoc consumer — but which
break on 02/03's renames, so they are migration surface (part 07).

---

## 1.1 Two language-owned decorators, consumer-applied

```wcl
@declares_kind(name = 0, params = "slots", body = "body")
@contextual
```

The decorator **names** belong to the language; their **use** belongs to the consumer. wdoc applies
them to its own types in `lib/components.wcl`.

**This is new ground and must be documented as such.** Every consumer-declared decorator WCL has today
— `@only`, `@except`, `@wdoc.file`, `@wdoc.editable`, `@answerable` — is **inert**: the language parses
it and the consumer reads it back. These are the first decorators where the language changes *its own*
behaviour on one.

*Rejected:* a registration API on `Environment` (splits the property from the `@block` declaration it
describes, and a document opened without the right Environment silently mis-validates); a
consumer-implemented trait (heaviest seam — `Document` gains a `dyn` parameter every caller pays for).
Note §1.4 brings back a narrow trait-shaped route for *behaviour*, which a decorator structurally
cannot carry.

**Naming, settled** (14 decision 7): `@declares_kind` says the unusual thing exactly — a kind comes
from an *instance*, not a type. `params` is the neutral word for what a declarer takes, leaving `slot`
free as wdoc's own spelling. `contextual` says placement is decided by context rather than by kind.
These go in the `wcl` wskill as **language features**.

*Rejected names:* `defines_kind`/`fields`/`generator` (`fields` collides with `Block::fields`;
`generator` reads as "produces values"); `kind_template`/`params`/`emits_anywhere` (`template` is
already taken, hard, by wdoc's layout block).

## 1.2 Derived schemas — `validate_component_instance` is deleted, not moved

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

Component instances become **ordinary registered kinds**. The two checks
`validate_component_instance` (`schema_check.rs:353-412`) hand-rolls — every instance field names a
declared slot, every defaultless slot is supplied — are *exactly* `UnknownField` and `MissingRequired`,
which the generic validator already emits.

This is what protects the spec's non-negotiable #2: `wcl check` keeps catching
`field 'labell' is not a slot of component 'metric_card'` — for free, and now for **any** consumer
using `@declares_kind`, not just wdoc.

**Routes through the derivation instead of hardcoding:** `component_def` (`doc.rs:2043` — note it is
**public and used 9× in `wcl_wdoc`**: `render/expand.rs:40,87,417`, `build.rs:1299`, `tree.rs:83`,
`node_table.rs:82`, `html.rs:620`, `pdf/collect.rs:189`, `markdown/emit.rs:237`), `is_component_kind`,
the `schema_check.rs:440` fallback, and the `error.rs:301` collision path.

**Implementation cost, stated honestly:** `synthetic_types` is fixed at construction and these are not,
so derived `TypeDecl`s need lazily-built owned storage on `Document` — a `OnceLock` arena alongside
`component_index`, which the derivation subsumes (`doc.rs:86`). **This is the real work in the part.**

*Rejected:* a de-hardcoded `validate_component_instance` reading its param-field name from the
decorator (smaller, but leaves the language with a bespoke second validator whose only user is wdoc —
and it is honestly a *slot* checker, so ticket 03's one-concept-two-checkers problem would stand);
moving the check to wdoc entirely (regresses `wcl check`, which is forbidden).

**Constraint from 14 decision 9, onto §1.5:** a component kind's schema will **not** be in
`type_decls()`, so anything introspecting by walking declarations will not find it — including the
editor's palette, which special-cases `wdoc_component` at `blocks.rs:1686` and must route through the
derivation instead.

## 1.3 `TypeRef` grows generics — syntactically only

```rust
TypeRef::Named { path: Vec<String>, args: Vec<TypeRef> }   // args defaults empty
```

Parser accepts `Path<A, B>`; printer round-trips it; serde carries it. **No arity check, no
substitution, no `type Foo<T>` declaration form.** Consumers read `args` as metadata. The 36 existing
`TypeRef::Named` sites (all inside `wcl_lang` — no consumer crate constructs or matches it) become a
one-line pattern change.

This is enough because the derivation emits **both** a typed field and a decorator, and the decorator
does the checking:

```
slot shapes: content<SvgBlock>
  ⇓ derives
@children(SvgBlock) shapes: content<SvgBlock>
//  ↑ does the checking      ↑ carries the intent
```

`is_descendant_of` on the interface already handles the accepts-check
(`schema_check.rs:803-808`), and **child-kind checking reads the `@children(X)` decorator argument, not
the field type** (`schema_check.rs:742-770`) — the field type is decorative there. So 03's readable
surface and 02's "the accepts-type is a set of block types in a field-type position" both land with
**zero type-system semantics**.

**Cost accepted:** `content<Nonsense>` parses and fails later, at `@children` resolution or at build.

*Note this reversed the session's recommendation.* The recommendation was to desugar `content` →
`list<WdocBlock>` for zero language change. Wil took generics, then scoped them to syntax-only.

*Rejected:* declared arity via `type Content<T>` (catches the above, but generic params become names
that must resolve inside a declaration body — the thin end of full generics); full generics (a
language-design effort in its own right — see [08](08-open.md), it is a **fresh effort**, not a
fold-in).

## 1.4 `@contextual` and the expander callback

Cluster C splits in two:

- **Exemption — metadata.** `@contextual` on the type. `schema_check.rs:787`'s hardcoded
  `"wdoc_repeater" | "wdoc_instance" | "wdoc_content"` becomes a decorator lookup, carrying both halves
  of what that site does: the block is legal wherever children are allowed at all, and its body is not
  recursed into. (`wdoc_content` drops off the list regardless — [02](02-blocks.md) kills it.)
- **Expansion — an expander callback registered on `Environment`**, consulted when the language
  projects `@children`. A decorator can declare *that* a block expands; it cannot carry *how*
  ("iterate `each`, bind each element to the symbol named by `as`"; "bind slot names to instance
  fields, falling back to each slot's `default`"). That is behaviour.

**This deletes the 87-line mirror** at `views.rs:2824-2902` (`generator_children`,
`expand_component_body`) — a deliberate simplified copy of `wcl_wdoc/src/render/expand.rs` (431 lines)
whose own doc comment says so: *"kept minimal for the projection path: an erroring `each` or slot value
contributes nothing here."* Two implementations of one semantics, drifting **by design**. Deleting it
makes projection *more* correct than today, because wdoc's real expander records errors where the
mirror silently contributes nothing.

**Control case worth knowing:** `partial` / `collect` are wdoc generators declared in the same file as
the others and appear in `wcl_lang` **zero** times — they get by as ordinary `@children(WdocBlock)`
blocks whose expansion is purely renderer-side.

*Rejected:* a richer `@expands(over = "each", bind = "as", body = "children")` decorator (fully
declarative and consistent with §1.1, but it is a mini-language for expansion semantics and,
decisively, **the duplicate implementation survives it**, parameterised but still able to drift);
dropping expansion from the language entirely (re-breaks the bug `views.rs:2749` was written to fix —
data-driven children inside a custom shape vanishing from `@children` projections).

## 1.5 A missing expander is a hard error, fired on demand

Projecting a `@contextual` block's generated children with **no registered expander** is a hard error
naming the missing expander. It fires **when the generated children are demanded**, not when the
document is opened.

- Safe: `wcl parse`, `wcl fmt`, any generic AST reader.
- Must supply the wdoc Environment: `wcl get` / `wcl eval`, the LSP, the editor's open paths.

**This forces CLI work in the same change.** `wcl check` opens with a plain `Environment::new()`
(`main.rs:1383`) while the wdoc registry is threaded separately as a *loader* (`main.rs:33`). See part
[07](07-migration.md) sweep 7 — and note it **must land with this part**, because between the two those
commands fail loudly on every wdoc document.

*Rejected:* silent degradation (the language would carry a mode where the right answer is quietly
unavailable — this map's own failure mode); binding the expander to `wcl_wdoc::schema_registry()` (it
is threaded as a loader today, so this would mean reshaping the CLI's wdoc-awareness inside the
language change).

## 1.6 Typed schema introspection — `FieldShape`

```rust
TypeField::shape() -> FieldShape::{ Scalar(prim), List(prim), Fn, Block, … }
```

Replaces the printed-string comparisons the editor's WAD Systems view rests on:

| today | site |
|---|---|
| `bare_type(f) == "identifier"` | `blocks.rs:1542` |
| `bare_type(f) == "list<identifier>"` | `blocks.rs:1558` |
| `to_string().starts_with("fn")` | `blocks.rs` |
| `full_name().starts_with("wdoc.")` | `blocks.rs:1460` |
| `is_descendant_of("wdoc.SvgBlock")` | `blocks.rs:1615` |

**Several fail silently** — a reclassified field just stops being a parent link, with no error. That
is this map's theme in miniature, and it is the concrete breakage risk the whole refactor carries.

Two constraints on the API:

- **It cannot land after 02/03's renames** (see [README](README.md) dependency order). 14 decision 9
  checked and corrected 05's claim that *this* extraction reshuffles the `wdoc` namespace — it does
  not; wdoc's types stay in `namespace wdoc` in `crates/wcl_wdoc/lib/*.wcl` and only the Rust
  hardcoding moves. The strings that actually break are broken by the **renames**.
- **Derived schemas (§1.2) must be reachable through it.** See the constraint at the end of §1.2.

## 1.7 Else-less `if` yields `none`

```wcl
class: ["book-chapter", if e.current { "current" }]
```

**Does not parse today** — verified: `unexpected token` at the closing brace. Without this change,
[02](02-blocks.md)'s none-dropping `class` costs `else { none }` at each of the 11 conditional-class
sites, and 05's headline example is aspirational rather than true.

Pure language feature; independent of every other item here and of every migration sweep.

**Related fact, no language change needed:** `["a", none]` already type-checks in a `list<utf8>` and
evaluates to `["a", none]`. Dropping the `none`s is therefore a **consumer-side change** in `wcl_wdoc`
— and an all-none list must emit **no attribute**, not `class=""`. (15 §4)

## 1.8 Riding out with the extraction

- `error.rs:301`'s component-name collision **survives, generically reworded**: a kind declared by a
  `@declares_kind` instance colliding with a registered `@block`/`@table` kind is a real generic error.
  Only the wording changes.
- `reflect.rs:386,435` docstrings and `lexer.rs:1146`'s test string get neutral examples.
- `doc.rs:86`'s `component_index` becomes the `@declares_kind` index.
- Comments rewritten in the new vocabulary: `doc.rs:1116`, `schema_check.rs:223-224`,
  `views.rs:810,970,2051,2077,2457,2749`, `eval.rs:1218-1219`, `cells.rs:84,192`, `scope.rs:23`,
  `parser/decls.rs:1135`.
- **No code change needed:** `scope.rs`, `cells.rs`, `eval.rs:1218` — already generic, name nothing.

## 1.9 What stays wdoc's

**The template slot check does not become a language feature.** `wcl_lang` exposes the declarer →
schema derivation as public API; components ride the ordinary block path automatically (§1.2);
**templates do not**. wdoc resolves the `page.template ?? site.default_template` pair, derives the
layout's slot schema through that same API, and applies all six severity rows itself
([03](03-templates.md) §3.6).

Why two call sites is honest rather than duplicative: the two filler relationships are **structurally
different** — a component instance *is* a block of the declared kind, so ordinary machinery applies; a
page is *not* a block of the template's kind, it is paired by a wdoc rule. And **four of the six
severity rows are irreducibly wdoc's**: `?`-conditional fills, the site-wide typo check (needs the
union of every layout in the site), double-fill, and the accepts-type check. Only "unknown fill name"
and "required slot unfilled" are the generic pair.

*Rejected:* a `check_fills(declarer, filler_fields)` API in `wcl_lang` — marginally more shared code,
but the language would gain an API whose only caller is wdoc, and the generic/wdoc split is arbitrary
enough to invite drift.

---

## Checklist for this part

- [ ] `@declares_kind` / `@contextual` parsed, indexed, documented as language features in the `wcl` wskill
- [ ] `block_schema` derivation + `OnceLock` arena on `Document`; `component_index` subsumed
- [ ] `validate_component_instance` **deleted**; `wcl check` still catches the `labell` case (regression test)
- [ ] `TypeRef::Named { path, args }` — parse, print, serde, round-trip; 36 sites updated
- [ ] `@contextual` replaces the `schema_check.rs:787` hardcode
- [ ] Expander callback on `Environment`; `views.rs:2824-2902` deleted
- [ ] Missing-expander hard error on demand **+ the CLI/LSP/editor Environment plumbing in the same change**
- [ ] `TypeField::shape() -> FieldShape`, reaching derived schemas
- [ ] Else-less `if` yielding `none`
- [ ] §1.8's renames, rewordings and neutral examples
