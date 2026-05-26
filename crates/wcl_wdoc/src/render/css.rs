//! The `class`-block lowering (`render_class` / `class_props`).
//!
//! The bare-string CSS that the `class` system can't express — non-bare
//! selectors, `@font-face` / `@keyframes`, `var()`/custom properties — no
//! longer lives here. Block-level + base CSS moved into co-located
//! `@block("stylesheet")` blocks in the stdlib (`lib/{core,table,terminal,
//! icons,tilemap,diagram-core,callout}.wcl`), which `build.rs` collects
//! into `<head>`; template-region CSS (`webpage` / `book`) moved into a
//! `<style>` the template emits into the body (`lib/templates.wcl`).
//! Styling that *is* a bare single-class rule (chart palette, headings, …)
//! lives in `lib/css-classes.wcl` as `class` blocks. The lone Rust-side
//! CSS that remains is `highlight::theme_css()` (the syntax-highlight
//! theme, an external `assets/code-theme.css`).

use std::fmt::Write as _;

use wcl_lang::Block;

use super::*;

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
