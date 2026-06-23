# wdoc: `:layered` / `:force` layout reserves a flat 40px for `node_table`, ignoring its derived height → overlapping boxes

**Reported by:** WAD C4 code-diagram work (2026-06-21)
**Component:** `wcl_wdoc` diagram layout — `effective_dims` in `crates/wcl_wdoc/src/render/svg/shapes.rs:257`, fed to the solver via `flow_nodes_of` (`crates/wcl_wdoc/src/render/svg/diagram.rs:285`, `size: effective_dims(...)` at :298). True size lives in `node_table_bbox` (`crates/wcl_wdoc/src/node_table.rs:104`).
**Severity:** bug (renders overlapping, unreadable diagrams; no clean workaround)

## Summary

When a `diagram` uses an auto-layout (`layout = :layered`, also `:force`/`:radial`),
each child's footprint is reported to the solver by `effective_dims`. That function
special-cases `container`, `circle`, and wireframe widgets, but has **no arm for
`node_table`**. A `node_table` carries no `height` field — its height is *derived*
(`node_table_bbox` = `header_offset + rows × row_height`) — so `effective_dims` falls
through to:

```rust
let declared_h = field_f64(block, "height").unwrap_or(40.0);  // node_table has no `height`
```

and reports **40px tall for every node_table**, regardless of how many `node_row`s it
holds. The solver then spaces the next rank ~40px below the box's top, while the box
actually renders 150–250px tall, so the following rank's shapes are drawn *on top of*
the preceding rank. (Width is fine — `node_table` declares `width`, which the fall-through
path does read.)

## Repro

A `:layered` diagram with `node_table` shapes that have several rows and at least two
ranks (e.g. two structs that both `implements` an interface — the interface ranks below
and overlaps them). Concretely, WAD's per-component C4 code diagram:

```
diagram { layout = :layered  routing = :elbow  width = 760  height = 440
  node_table { id = "fleet_view"  title = "Ⓢ FleetView"  width = 220.0
    node_row { p "- rows: Vec<RegionRow>" }
    node_row { p "+ render(&self, f: &mut Frame, area: Rect)" }
    node_row { p "+ on_key(&mut self, key: KeyEvent) -> Action" }
  }
  node_table { id = "check_tail" ... }     // sibling on the same rank
  node_table { id = "view" ... }           // interface, ranked below
  edges = { fleet_view -> view; check_tail -> view }   // :implements
}
```

`fleet_view`/`check_tail` (rank 0) render ~200px tall, but the solver places `view`
(rank 1) only ~40 + layer_gap px down → `view` overlaps both. Screenshot in the WAD
session shows the interface box sitting across the bottom of the two struct boxes.

## Expected

`effective_dims` should report a `node_table`'s true derived size, so the solver reserves
its real extent — exactly as it already does for `container` (content bbox + padding),
`circle` (2r), and wireframe widgets (measured). The data is one call away.

## Suggested fix

Add a `node_table` arm to `effective_dims` (`shapes.rs:257`), before the declared/default
fall-through:

```rust
if block.kind() == "node_table" {
    let (_, _, w, h) = crate::node_table::node_table_bbox(block, 0.0, 0.0);
    return (w, h);
}
```

`node_table_bbox` already returns `(x, y, width, derived_height)`; only `w`/`h` are needed.
Passing `parent_w/parent_h = 0.0` is fine for the common absolute-`width` case (used by
DB/class/code diagrams). If percentage widths against the parent are ever needed here, the
real `parent_w/parent_h` are available at the `flow_nodes_of` call site and can be threaded
through.

## Impact / notes

- Affects every auto-laid-out diagram built from `node_table` shapes — DB schema diagrams
  and C4 code/class diagrams are the primary users.
- Today the only workarounds are hacks: hand-tuning `layer_gap` per diagram to a value
  larger than the tallest box (fragile — box height is content-dependent), or abandoning
  auto-layout and hand-placing every box with `rect`/`x`/`y`. Neither is acceptable for
  data-driven diagrams whose box contents vary per entity.
- Row-content wrapping (a row wider than the box `width` wraps to N lines but
  `row_height` reserves one) is a *separate, smaller* derived-height inaccuracy; fixing
  the missing-arm bug above is the blocker. Measuring wrapped row height would be a
  follow-up refinement.
