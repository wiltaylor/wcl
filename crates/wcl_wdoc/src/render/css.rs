//! CSS: the injected style-string constants plus the `class`-block
//! lowering (`render_class` / `class_props`).

use std::fmt::Write as _;

use wcl_lang::Block;

use super::*;

/// Base page styling. Resets the body margin and makes it fill the
/// viewport so a themed `class "wdoc-body" { … }` background reaches
/// every edge (no white gutter / short-page gap). Colour-neutral, so
/// un-themed pages look the same as before.
pub(crate) const BASE_CSS: &str = "\
.wdoc-body { margin: 0; min-height: 100vh; }";

// Note: the heading hierarchy (`.heading-1`..`6`) is no longer a Rust
// constant — it migrated to bundled `class` blocks in css-classes.wcl now
// that the class system carries `font_weight` / `line_height` /
// `text_transform` / `letter_spacing` (see the chart-palette note below).

/// Default styling for `table` blocks, injected into every page's
/// `<style>` (before user `class` rules, so those still override it).
/// Keeps tables legible out of the box without forcing every author
/// to declare a border class.
pub(crate) const TABLE_CSS: &str = "\
table.wdoc-table { border-collapse: collapse; }
.wdoc-table th, .wdoc-table td { border: 1px solid #ccc; padding: 0.3rem 0.6rem; text-align: left; }
.wdoc-table th { background: #f4f4f4; }";

/// Default styling for the bundled `webpage` template's regions.
/// Injected like `TABLE_CSS`; user `class` rules can override it.
// `.site-header`'s colour/weight lives in the bundled `class "site-header"`
// (wdoc.wcl); only its flex / descendant / display rules stay here.
pub(crate) const SITE_CSS: &str = "\
.site-nav { display: flex; gap: 1rem; padding: 0.5rem 0; border-bottom: 1px solid #ccc; margin-bottom: 1rem; }
.site-nav a { text-decoration: none; }
.site-main { display: block; }";

/// Default styling for the bundled `book` template — a fixed left
/// chapter sidebar and a centered reading column. Injected like
/// `SITE_CSS`; user `class` rules can override it.
// Only the layout that needs compound / descendant / `:hover` selectors
// lives here; the chapter / section colours and the active-chapter weight
// moved to bundled `class "book-chapter" / "book-section" / "current"`
// blocks (wdoc.wcl), which a user `class` of the same name overrides.
pub(crate) const BOOK_CSS: &str = "\
.book-sidebar { position: fixed; top: 0; left: 0; width: 16rem; height: 100vh; overflow-y: auto; box-sizing: border-box; padding: 1rem; border-right: 1px solid #ccc; background: #fafafa; }
.book-title { font-weight: bold; font-size: 1.1rem; margin-bottom: 0.75rem; }
.book-sidebar ul.book-toc { list-style: none; margin: 0; padding-left: 0; }
.book-sidebar ul.book-toc ul.book-toc { padding-left: 0.85rem; }
.book-sidebar a.book-chapter { display: block; padding: 0.2rem 0; }
.book-chapter { text-decoration: none; }
.book-chapter:hover { text-decoration: underline; }
.book-sidebar .book-section { display: block; padding: 0.35rem 0 0.1rem; font-weight: 600; }
.book-content { margin-left: 16rem; padding: 1rem 2.5rem; max-width: 46rem; }
.theme-toggle { display: block; margin: 0 0 0.75rem; padding: 0.2rem 0.6rem; cursor: pointer; background: transparent; border: 1px solid currentColor; border-radius: 4px; color: inherit; font: inherit; }";

// Note: `bar_chart` / `line_chart` / `pie_chart` styling (the eight-hue
// series palette + axes / gridlines / labels / title / legend / line) is
// no longer a Rust constant — it migrated wholesale to bundled `class`
// blocks in wdoc.wcl (`wdoc-series-1`..`8`, `wdoc-axis`, `wdoc-grid`,
// `wdoc-axis-label`, `wdoc-chart-title`, `wdoc-legend`, `wdoc-line`).
// Every rule was a bare single-class selector over allowlisted paint
// properties, so the class system expresses it directly; recolour a
// series by redeclaring `class "wdoc-series-N" { fill = … }`.
//
// Why the OTHER style constants below can't follow suit — the class
// system emits only a single bare `.name { … }` rule from a fixed
// property allowlist (color/background/bold/italic/underline/font_weight/
// font_size/line_height/font_family/text_align/text_transform/
// letter_spacing/padding/margin/border + the SVG paints
// fill/stroke/stroke_width/stroke_linejoin/stroke_linecap/opacity), so
// none of these are expressible:
//   - BASE_CSS:     `min-height` is not allowlisted.
//   - TABLE_CSS:    `table.wdoc-table` (element+class) and `.wdoc-table
//                   th/td` (descendant) selectors; `border-collapse`.
//   - SITE/BOOK:    `display:flex`/`gap`, `position:fixed`, descendant
//                   and `:hover` selectors (the bare-class colours of
//                   each already moved out — see above).
//   - TERMINAL_CSS: `@font-face`, `@keyframes`, `:hover`, `[hidden]`,
//                   descendant `.wdoc-terminal-svg text`, layout props.
//   - ICON_CSS:     `svg.wdoc-icon` (element+class); display/width/height.
//   - TILEMAP_CSS:  `.wdoc-tilemap image` (descendant) + `.smooth image`
//                   (compound); `image-rendering`.
//   - DIAGRAM_CSS:  position/flex, descendant svg, `.panning`, `:hover`.
//   - CALLOUT_CSS:  built on a `--callout-accent` custom property +
//                   `var()`; compound `.callout.note/…`; `:first/last-
//                   child`.
//   - code-theme.css (highlight::theme_css): `pre.code-block` compound,
//                   descendant `pre.code-block code`, compound token
//                   classes (`.tok-storage.tok-type`).
// In short: non-bare selectors, pseudo-classes/elements, attribute
// selectors, @font-face/@keyframes, and var()/custom properties can never
// be a `class`, nor can any property outside the allowlist.

/// Default styling for `terminal` blocks: the embedded JetBrains Mono
/// Nerd Font faces (served from `_wdoc/`), the cell-grid font binding,
/// window chrome, blink/cursor animation, and replay controls. Injected
/// like `TABLE_CSS` (before user `class` rules, so those override it).
/// Only emitted on pages that actually contain a terminal.
///
/// The terminal's colours come from the `class` system: `.wdoc-terminal`
/// is the default theme (dark bg + light fg) on bare-class selectors, so
/// a user `class \"x\" { background = … color = … }` on the terminal
/// overrides it. Default-fg glyphs paint with `currentColor` (so the
/// class `color` themes them, dark/light included) and the terminal
/// background is the `<div>`'s `background`; only explicit ANSI colours
/// carry inline fills.
pub(crate) const TERMINAL_CSS: &str = "\
@font-face { font-family: 'JetBrainsMono Nerd Font'; font-weight: normal; font-style: normal; font-display: swap; src: url('_wdoc/JetBrainsMonoNerdFontMono-Regular.woff2') format('woff2'); }
@font-face { font-family: 'JetBrainsMono Nerd Font'; font-weight: bold; font-style: normal; font-display: swap; src: url('_wdoc/JetBrainsMonoNerdFontMono-Bold.woff2') format('woff2'); }
@font-face { font-family: 'JetBrainsMono Nerd Font'; font-weight: normal; font-style: italic; font-display: swap; src: url('_wdoc/JetBrainsMonoNerdFontMono-Italic.woff2') format('woff2'); }
.wdoc-terminal { display: inline-block; max-width: 100%; border-radius: 6px; overflow: hidden; }
.wdoc-terminal { background: #1c1c1c; color: #d0d0d0; }
.wdoc-terminal-svg { display: block; max-width: 100%; height: auto; }
.wdoc-terminal-svg text { font-family: 'JetBrainsMono Nerd Font', ui-monospace, 'Cascadia Code', 'Fira Code', Menlo, Consolas, monospace; }
.term-chrome-bar { fill: currentColor; opacity: 0.08; }
.term-title { fill: currentColor; opacity: 0.6; font-family: ui-sans-serif, system-ui, sans-serif; font-size: 0.75em; }
.term-close { stroke: currentColor; opacity: 0.55; stroke-width: 1.5; stroke-linecap: round; }
.term-close:hover { opacity: 1; }
.term-chrome-btn { fill: currentColor; opacity: 0.55; font-size: 0.8em; cursor: pointer; }
.term-chrome-btn:hover { opacity: 1; }
.term-cursor { fill: currentColor; opacity: 0.65; }
.term-blink { animation: wdoc-term-blink 1s steps(1) infinite; }
@keyframes wdoc-term-blink { 50% { opacity: 0; } }
.wdoc-terminal-error { color: #bf616a; font-family: monospace; }
.wdoc-terminal-player { position: relative; }
.term-overlay-play { position: absolute; top: 50%; left: 50%; transform: translate(-50%, -50%); width: 4rem; height: 4rem; display: flex; align-items: center; justify-content: center; padding: 0; border: none; border-radius: 50%; cursor: pointer; background: rgba(20,20,20,0.55); color: #fff; font-size: 1.9rem; line-height: 1; }
.term-overlay-play:hover { background: rgba(20,20,20,0.75); }
.term-overlay-play[hidden] { display: none; }";

/// Default styling for icons. The inline `:name:` handler emits an
/// `<svg class=\"wdoc-icon\">` sized to the surrounding text (1em); a
/// per-icon `size` overrides it via inline style. Diagram icons are
/// `<use>` elements sized by their `width` / `height`, so the size rule
/// uses the `svg.wdoc-icon` selector and leaves them alone. Icons paint
/// with `currentColor`, so a user `class`/`color` recolours them.
pub(crate) const ICON_CSS: &str = "\
svg.wdoc-icon { display: inline-block; width: 1em; height: 1em; vertical-align: -0.125em; }";

/// Default styling for `tilemap` blocks. Pixel-art tilesets read best
/// with nearest-neighbour scaling, so tile images render `pixelated` by
/// default; a `smooth = true` tilemap opts back into the browser's
/// default smoothing via the extra `smooth` class. Injected like the
/// other constants (before user `class` rules, which therefore override
/// it).
pub(crate) const TILEMAP_CSS: &str = "\
.wdoc-tilemap image { image-rendering: pixelated; }
.wdoc-tilemap.smooth image { image-rendering: auto; }";

/// Styling for interactive (pan + zoom) diagrams. Only `pan_zoom`
/// diagrams get the `.wdoc-diagram-viewport` wrapper, so plain diagrams
/// are untouched. The controls paint with `currentColor` (like the
/// terminal chrome), so they adopt the page theme. Injected before user
/// `class` rules, which therefore override it. The bundled
/// `diagram-pan-zoom.js` drives the SVG `viewBox`.
pub(crate) const DIAGRAM_CSS: &str = "\
.wdoc-diagram-viewport { position: relative; display: inline-block; max-width: 100%; }
.wdoc-diagram-viewport svg { display: block; max-width: 100%; height: auto; touch-action: none; cursor: grab; }
.wdoc-diagram-viewport.panning svg { cursor: grabbing; }
.wdoc-diagram-controls { position: absolute; top: 6px; right: 6px; display: flex; flex-direction: column; gap: 4px; }
.wdoc-diagram-controls button { width: 1.6rem; height: 1.6rem; padding: 0; display: flex; align-items: center; justify-content: center; font: inherit; font-size: 1rem; line-height: 1; cursor: pointer; color: inherit; background: rgba(127,127,127,0.12); border: 1px solid currentColor; border-radius: 4px; opacity: 0.6; }
.wdoc-diagram-controls button:hover { opacity: 1; }";

/// Default styling for `callout` blocks. The per-type accent rides a CSS
/// custom property so it colours only the heading, the left border, and
/// the (currentColor) icon — never the body text. Injected like the
/// other constants (before user `class` rules, which therefore override
/// it); the background is a theme-neutral translucent grey so callouts
/// read on both light and dark pages.
pub(crate) const CALLOUT_CSS: &str = "\
.callout { --callout-accent: #888; margin: 1rem 0; padding: 0.6rem 0.9rem; border-left: 4px solid var(--callout-accent); border-radius: 4px; background: rgba(127,127,127,0.08); }
.callout-heading { display: flex; align-items: center; gap: 0.45rem; font-weight: 600; color: var(--callout-accent); }
.callout-title { margin: 0; }
.callout-heading svg.wdoc-icon { width: 1.15em; height: 1.15em; }
.callout-body > :first-child { margin-top: 0; }
.callout-body > :last-child { margin-bottom: 0; }
.callout.note, .callout.info { --callout-accent: #5e81ac; }
.callout.tip { --callout-accent: #88c0d0; }
.callout.warning { --callout-accent: #d08770; }
.callout.error { --callout-accent: #bf616a; }
.callout.success { --callout-accent: #a3be8c; }";

/// Emit a CSS rule body for a `@block("class")` instance.
/// Returns `None` if the block doesn't have an inline name.
/// Build the CSS declaration string for one styling block (a `class`
/// or one of its `light {}` / `dark {}` mode blocks — they share field
/// names). Empty when no styling fields are set.
pub(crate) fn class_props(block: &Block<'_>) -> String {
    let mut props = String::new();
    push_css(&mut props, "color", field_utf8(block, "color").as_deref());
    push_css(
        &mut props,
        "background",
        field_utf8(block, "background").as_deref(),
    );
    if field_bool(block, "bold") == Some(true) {
        props.push_str("font-weight:bold;");
    }
    if field_bool(block, "italic") == Some(true) {
        props.push_str("font-style:italic;");
    }
    if field_bool(block, "underline") == Some(true) {
        props.push_str("text-decoration:underline;");
    }
    // Numeric/named weight (e.g. "600"/"700"); distinct from the `bold`
    // flag above, which a later `font_weight` overrides by cascade.
    push_css(
        &mut props,
        "font-weight",
        field_utf8(block, "font_weight").as_deref(),
    );
    push_css(
        &mut props,
        "font-size",
        field_utf8(block, "font_size").as_deref(),
    );
    push_css(
        &mut props,
        "line-height",
        field_utf8(block, "line_height").as_deref(),
    );
    push_css(
        &mut props,
        "font-family",
        field_utf8(block, "font_family").as_deref(),
    );
    push_css(
        &mut props,
        "text-align",
        field_utf8(block, "text_align").as_deref(),
    );
    push_css(
        &mut props,
        "text-transform",
        field_utf8(block, "text_transform").as_deref(),
    );
    push_css(
        &mut props,
        "letter-spacing",
        field_utf8(block, "letter_spacing").as_deref(),
    );
    push_css(
        &mut props,
        "padding",
        field_utf8(block, "padding").as_deref(),
    );
    push_css(&mut props, "margin", field_utf8(block, "margin").as_deref());
    push_css(&mut props, "border", field_utf8(block, "border").as_deref());
    // SVG painting — themes diagram shapes and chart series (their
    // `class`es reach the `<rect>` / `<line>` / `<polygon>` elements).
    push_css(&mut props, "fill", field_utf8(block, "fill").as_deref());
    push_css(&mut props, "stroke", field_utf8(block, "stroke").as_deref());
    push_css(
        &mut props,
        "stroke-width",
        field_utf8(block, "stroke_width").as_deref(),
    );
    push_css(
        &mut props,
        "stroke-linejoin",
        field_utf8(block, "stroke_linejoin").as_deref(),
    );
    push_css(
        &mut props,
        "stroke-linecap",
        field_utf8(block, "stroke_linecap").as_deref(),
    );
    push_css(
        &mut props,
        "opacity",
        field_utf8(block, "opacity").as_deref(),
    );
    props
}

/// Emit the CSS rule(s) for a `@block("class")`. The class's own
/// fields are shared defaults; optional `dark {}` / `light {}` mode
/// blocks add per-mode overrides. `dark` is the default mode; `light`
/// applies under `prefers-color-scheme: light`; an explicit
/// `:root[data-theme=…]` (set by the theme toggle) overrides both.
pub(crate) fn render_class(block: &Block<'_>) -> Option<String> {
    let name = label_string(block)?;
    let base = class_props(block);
    let dark = block.block("dark").map(|b| class_props(&b));
    let light = block.block("light").map(|b| class_props(&b));

    // Default-mode rule: shared fields, with the dark mode merged in
    // (dark is the default) so a later same-specificity declaration
    // wins for overlapping properties.
    let mut default_props = base.clone();
    if let Some(d) = &dark {
        default_props.push_str(d);
    }
    let mut out = format!(".{name} {{ {default_props} }}");

    if let Some(l) = &light {
        write!(
            out,
            "\n@media (prefers-color-scheme: light) {{ .{name} {{ {l} }} }}"
        )
        .expect("write to String");
    }
    // Explicit toggle overrides the system preference (higher specificity).
    if let Some(d) = &dark {
        write!(out, "\n:root[data-theme=\"dark\"] .{name} {{ {base}{d} }}")
            .expect("write to String");
    }
    if let Some(l) = &light {
        write!(out, "\n:root[data-theme=\"light\"] .{name} {{ {base}{l} }}")
            .expect("write to String");
    }
    Some(out)
}

pub(crate) fn push_css(out: &mut String, prop: &str, value: Option<&str>) {
    if let Some(v) = value {
        write!(out, "{prop}:{v};").expect("write to String");
    }
}
