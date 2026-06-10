# Bug report: reflection (`type_table` / `block_reference` / `child_types`) returns empty for namespaced types

Found 2026-06-10 while continuing the `wad` example project's namespace migration, immediately
after `d45f0eaa` (fix(lang): namespace-aware block-kind resolution and connection dispatch) —
that fix works; this is the next layer down. Repro files live in `/tmp/wcl-blockref/`.

## Symptom

`type_table { type = T }`, `block_reference { type = T }` and the `child_types(T)` builtin all
silently emit/return **nothing** when `T` is declared inside a `namespace`. The same constructs
work for root-namespace types in the same document. No diagnostic — the blocks just vanish from
the rendered page.

## Repro

`schema.wcl` — namespaced:

```wcl
namespace lib

import <wdoc.wcl>

@block("gizmo")
type Gizmo {
  @inline(0) @doc("Stable id.") id: utf8
  @doc("Display name.") name: utf8
}

@document
type LibModel {
  @children("gizmo") gizmos: list<Gizmo>
}
```

`schema_root.wcl` — identical but no `namespace` (`RootGizmo` / `RootModel`).

`main.wcl`:

```wcl
import <wdoc.wcl>
import "./schema.wcl"
import "./schema_root.wcl"

site t {
  default_template = :book
  title = "T"
  toc { chapter "R" { page = ref } }
}

page ref {
  sites = [:t]
  h1 "Ref"
  h2 "namespaced"
  block_reference { type = LibModel }      // emits NOTHING
  h2 "root"
  block_reference { type = RootModel }     // works: heading + property table
  h2 "type_table probe"
  type_table { type = Gizmo }              // emits NOTHING
  h2 "qualified probe"
  type_table { type = lib.Gizmo }          // emits NOTHING (dotted path resolves, still empty)
  h2 "child_types probe"
  wdoc_repeater { each = child_types(LibModel)  as = :b   // empty list — repeater body never runs
    type_table { type = b }
  }
}
```

```
$ wcl wdoc build main.wcl --out _out   # exit 0, "wrote 1 page"
```

Rendered text of `ref.html`: the `rootgizmo` table appears under "root"; every namespaced probe
section is empty.

## Expected

Reflection should see a namespaced type's fields and child slots exactly as it sees a root type's
— `type_fields` / `child_types` presumably key their lookup by bare type name and miss the
qualified registration (`lib.Gizmo`), or look in a per-namespace table without falling back.

Note the failure is **silent** — even an unresolved `type =` reference produces no diagnostic.
A warning when `type_table` / `block_reference` resolves to a type with zero reflected fields
would have surfaced this immediately.

## Impact on `wad`

`wad`'s new self-documenting "Schema Reference" appendix (`wdoc/reference.wcl`, built on
`block_reference` over `wad.c4.C4Model`, `wad.adr.AdrModel`, …) renders as headings with no
tables. Every schema field in `wad` carries `@doc` specifically to feed these tables. The page
ships disabled-in-effect until this is fixed; everything else about the namespace migration works
after `d45f0eaa`.

## Suggested regression tests

1. `type_fields(lib.Gizmo)` / `child_types(lib.LibModel)` return the same shape as for identical
   root-namespace declarations.
2. `block_reference { type = LibModel }` (bare, resolved via import search path) and
   `{ type = lib.LibModel }` (qualified) both emit one heading + table per child block.
3. A `type_table` whose `type` resolves to nothing emits a diagnostic instead of silence.
