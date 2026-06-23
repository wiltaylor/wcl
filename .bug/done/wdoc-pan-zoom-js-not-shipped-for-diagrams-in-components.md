# wdoc: pan/zoom (and player) JS not shipped when the diagram is inside a wdoc_component

**Reported by:** WAD skill implementation (2026-06-13)
**Component:** `wcl_wdoc` asset/script injection (`build.rs` `uses_pan_zoom` / `render/svg/mod.rs`)
**Severity:** moderate (interactive diagrams silently render inert)

## Summary

`diagram-pan-zoom.js` is shipped + injected only when `uses_pan_zoom(page)` is
true, which walks the page's **raw block tree** (`render/svg/mod.rs`):

```rust
pub(crate) fn uses_pan_zoom(block: &Block<'_>) -> bool {
    (block.kind() == "diagram" && field_bool(block, "pan_zoom") == Some(true))
        || block.blocks().any(|b| uses_pan_zoom(&b))
}
```

A `pan_zoom = true` diagram that lives inside a `wdoc_component` (and is only
emitted when the page instantiates that component) is NOT in the page's raw block
tree — the page tree holds the component-instance block, whose `.blocks()` does
not include the component body. So the scan misses it: the renderer still emits
the correct `.wdoc-diagram-viewport` + `<svg data-pan-zoom data-zoom-min=…>`
markup, but **the JS is never shipped to `_wdoc/` and no `<script>` tag is added**.
The diagram looks interactive in the HTML but does nothing.

The same gap likely affects `uses_terminal`, `uses_map`, and the other
player-detection scans for content rendered via components/repeaters.

## Repro

```wcl
import <wdoc.wcl>
wdoc_component graph {
  wdoc_body {
    diagram { pan_zoom = true  width = 200  height = 120
      process "A" { id = a }  process "B" { id = b }
      a -> b
    }
  }
}
site s { default_template = :book  title = "x"  toc { chapter "C" { page = p } } }
page p { sites = [:s]  start = true  h1 "C"  graph {} }
```

`wcl wdoc build` → `p.html` contains `data-pan-zoom` markup, but
`_wdoc/diagram-pan-zoom.js` is absent and no `<script src=…diagram-pan-zoom.js>`
is injected. Move the same `diagram { pan_zoom = true … }` directly into the page
body and it works.

## Expected

Player-asset detection should see diagrams (terminals, maps, …) emitted through
`wdoc_component` / `wdoc_repeater`, so `pan_zoom` works regardless of whether the
diagram is authored inline or via a component. Either run detection on the
rendered output, or expand component/repeater bodies during the scan.

## Workaround in use

WAD authors at least one `pan_zoom` diagram **directly in a book page** (the
System Context graph in `wdoc/book.wcl`) rather than only inside section
components. Because the JS-injection flag is site-global, that one in-page diagram
ships the JS and adds the `<script>` to every page, which then activates the
viewports rendered by the section components too.
