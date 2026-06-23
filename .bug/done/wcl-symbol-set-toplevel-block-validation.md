# wcl: symbol_set-typed field rejected on top-level (@document-child) blocks

**Reported by:** WAD skill implementation (2026-06-13)
**Component:** `wcl_lang` schema validation (symbol_set membership)
**Severity:** moderate (forces enum fields to weaken to plain `symbol`)

## Summary

A field typed by a `symbol_set` validates correctly only on **nested** blocks. On
a block authored at the document top level (a direct `@document` child),
assigning even a valid member fails:

```
schema_violation: field 'status' declared as <StatusSet> but value is symbol
```

This happens for a literal member assignment and via `@default(:member)` alike.

## Repro

```wcl
symbol_set NodeStatus { existing tba broken }
@block("infra_node") type InfraNode { @inline(0) id: identifier  status: NodeStatus }
@document type M { @children("infra_node") nodes: list<InfraNode> }
infra_node "box" { status = :existing }      // schema_violation (top-level)
```

Move the same block to be a child of another block and it validates. (The same
type/field works fine nested.)

## Expected

A `symbol_set`-typed field should validate symbol-set membership identically
whether the block is a document child or nested.

## Workaround in use

WAD types every enum-like field on a top-level entity as plain `symbol`
(`provenance`, `status`, lint-rule `kind`, …) and documents the allowed values in
a comment, comparing/`match`ing them in render code. Nested blocks still use
`symbol_set`. This loses membership validation on those fields (the WAD
mechanical lint compensates where it matters).
