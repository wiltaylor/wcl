# wdoc: `body`/`project` can't address a body on a record nested inside another record's `@children`

**Status:** ✅ Fixed in wcl **0.24.1-alpha** (2026-06-20). Nested-record body projection now resolves — both a single `@child("body")` and a `@children("body")` list project correctly from a nested `wdoc_repeater`. The wskill model migrated its `tutorial`/`procedure` steps off `partial`/`collect` onto `body`/`project` as a result; no workaround remains.

**Reported by:** wskill body/project migration (2026-06-20)
**Component:** `wcl_wdoc` `project` resolution — `crates/wcl_wdoc/src/render/expand.rs`; the `@by_ref` reference threading in `crates/wcl_lang/src/doc/views.rs` (`to_record_value_at`, `reify_block_at`, `dataref_to_value_at`); `crates/wcl_wdoc/lib/project.wcl`. Introduced by `40054ff2 feat(wdoc): addressable content fragments via body + project`.
**Severity:** enhancement / limitation (workaround exists: keep nested bodies on `partial`/`collect`)

## Summary

`body` + `project` (the new addressable-content-fragment feature) works beautifully
for a record that sits **directly in a document-root `@children` slot** — a
`project { from = s.overview }` in a `wdoc_repeater { each = servers }` resolves each
server's own fragment. But it does **not** work for a record that is nested one level
deeper — i.e. a record in the `@children` list of *another* record. The `@by_ref`
body slot reifies, but the emitted reference doesn't re-resolve, and `project` errors.

This is exactly the shape of an ordered-steps model: a `tutorial` / `procedure` block
with `@children("step") steps`, where each step carries its own body. The top-level
units (`concept` / `entity` / `fact`) migrate to `body`/`project` cleanly because they
are root-gathered; the nested `step` bodies cannot, so they have to stay on the older
`partial` / `collect` mechanism. The two mechanisms now coexist for no reason the
author can see — the natural expectation is that a body rides on *any* record.

## Reproduction

### 1. Body on a nested record — `project` fails to resolve

```wcl
import <wdoc.wcl>

@block("tstep")
type TStep { @inline(0) n: u32  @child("body") body: WdocAddressableBody? }
@block("tut")
type Tut  { @inline(0) id: identifier  @children("tstep") steps: list<TStep> }
@document
type Doc  { @children("tut") tuts: list<Tut> }

tut t1 {
  tstep 1 { body { p "STEP ONE body" } }
  tstep 2 { body { p "STEP TWO body" } }
}

site s { title = "t" }
page pg { sites = [:s]
  wdoc_repeater { each = tuts as = :t            // root-gathered record — OK so far
    wdoc_repeater { each = t.steps as = :st      // nested record — the problem
      h3 $"Step ${st.n}"
      project { from = st.body }
    }
  }
}
```

`wcl wdoc build` fails:

```
× error: `project` target `body` did not resolve to a `body`; it must
  name an addressable `body` (e.g. a `@by_ref` property of the data being
  generated from)
   ╭─[…]
   │     project { from = st.body }
```

`wcl check` **passes** — the failure is render-time only. It happens **with or without
an intervening `filter`** on the outer list (`let tuts = filter(tuts, …)`), so the
filter/reification step is not the cause; plain nesting is. The equivalent top-level
case (`wdoc_repeater { each = servers as = :s  project { from = s.overview } }` over a
root `@children("server")`) renders correctly — this is the documented happy path and
it works.

### 2. Numeric `@inline(0)` labels can't be path-addressed either

The cycle test in the feature commit shows a body can be reached by a full static path
from a root field (`from = loops.only.b`). That relies on the nested record having a
*named* `@inline(0)` label. A step's label is its number (`@inline(0) n: u32`), and a
numeric segment can't be written in a dotted path:

```wcl
page pg { sites = [:s]
  project { from = tuts.t1.steps.1.body }
}
```

```
× expected identifier after '.', found number
   ╭─[…]
   │   project { from = tuts.t1.steps.1.body }
   ·                                  ┬
   ·                                  ╰── expected identifier
```

So neither the repeater-binding form (`st.body`) nor the full-static-path form works
for a nested, numerically-labelled record. There is no remaining way to project it.

## Requested

Make a `@by_ref` body resolvable when its owning record is reached through a **nested**
`@children` (or `@child`) access, not only when the record is a direct child of the
document root. Concretely, one or more of:

1. **Thread the document path through nested record access.** When a repeater binds
   `st` from `t.steps` (where `t` was itself bound from a root `@children`), `st.body`
   should carry a `Value::DataPath` whose `base` is the full root path
   (`[tuts, <t-label>, steps, <st-label>]`), so `Document::get` re-resolves it. The
   `to_record_value_at` / `reify_block_at` threading already extends `base` by field
   name for child slots when a record is reified top-down — the gap appears to be that
   a record reached via a *value access* on an already-reified parent (`t.steps`) has
   lost / never carried the base needed to address its own `@by_ref` slots.

2. **Allow numeric path segments** (or index segments) in the `from` expression /
   `DataPath`, so `tuts.t1.steps.1.body` (or an indexed form) parses and resolves —
   covers the static-path escape hatch for numerically-labelled nested records.

Either alone would let an ordered-steps model use `body`/`project` end-to-end. Both
together would make the feature uniformly "a body rides on any record, projected from
wherever you iterate to it".

## Code touch-points

- `crates/wcl_lang/src/doc/views.rs`: `to_record_value_at` / `reify_block_at` /
  `dataref_to_value_at` — confirm the `base` path is threaded when a nested
  `@children` element is reified, and survives a value-level field access on an
  already-reified parent (the `t.steps` case). The doc comment already promises
  "a `@child`/`@children` slot whose kind is `@by_ref` reifies to a
  `Value::DataPath { segments: base + [field, …] }`" — verify that holds at depth > 1.
- `crates/wcl_wdoc/src/render/expand.rs`: the `project` arm that turns `from` into a
  body and calls `Document::get` — the "did not resolve to a `body`" error is raised
  here; check what path the nested `DataPath` actually carries.
- WCL path parser: allow numeric / indexed segments after `.` in a path expression
  (for the static-address escape hatch).
- Tests in `crates/wcl_wdoc/tests/build.rs`: a body on a record in a nested
  `@children` list, projected from a nested `wdoc_repeater` (mirroring tutorial →
  steps → step.body); and a numeric-label static path.

## Workaround in use

In the wskill schema, the four root-level units (`concept` / `entity` / `fact` /
`index`) use `@child("body") body: WdocAddressableBody?` + `project { from = c.body }`.
The nested **step** records (`procedure`'s `step` screen/steps fragments, and
`tutorial_step`) stay on the old `@children("partial")` + `collect "<kind>_<id>"`
pattern, because their bodies can't be projected. Functional, but it means the same
codebase carries both content-fragment mechanisms and new authors have to learn the
"steps are different" exception.
