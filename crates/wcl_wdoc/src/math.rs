//! LaTeX math → inline SVG, via the pure-Rust RaTeX pipeline.
//!
//! Like `terminal` and the `Highlighted` code body, the SVG comes from a
//! crate that isn't expressible in WCL, so it's produced in Rust. Two
//! leaf payloads feed in: the block `math` lower emits an
//! `Html::Math` (rendered by [`render_math_fundamental`]), and
//! the inline `$…$` / `$$…$$` patterns emit an `InlineSpan::Math`
//! (rendered by [`render_inline_math`]). Both carry `{ latex, display,
//! class }` and share the [`math_svg`] core.
//!
//! The pipeline is `parse → layout → to_display_list → render_to_svg`
//! with `embed_glyphs = true` and the `embed-fonts` feature, so each SVG
//! is fully self-contained (glyph outlines as `<path>`s from the embedded
//! KaTeX fonts — no external CSS/fonts, works on a `file://` open).
//!
//! **Colour follows the text.** RaTeX paints every glyph/rule with the
//! layout colour (black by default), emitted as `fill="rgba(0,0,0,1)"`.
//! [`force_current_color`] rewrites that default black to `currentColor`
//! so an equation inherits whatever `color` its surroundings have (theme,
//! `class`, light/dark toggle); an explicit `\textcolor{…}` keeps its own
//! (non-black) colour. The `wdoc-math` structured rules reinforce this in CSS.

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::OnceLock;

use regex::Regex;
use wcl_lang::Value;

use ratex_layout::{LayoutOptions, layout, to_display_list};
use ratex_parser::parse;
use ratex_svg::{SvgOptions, render_to_svg};
use ratex_types::math_style::MathStyle;

use crate::render::{classes_attr_from_names, escape_html, map_utf8, map_utf8_list};

/// User units per em handed to RaTeX. Coordinates come back in these
/// units; the rendered `<svg>` is then re-sized in `em` (from the
/// display list's own em metrics) so it scales with the surrounding
/// font. A non-trivial value keeps the embedded glyph outlines clear of
/// `f32` rounding; the exact number is irrelevant since the viewBox is
/// scaled to a CSS box.
const EM: f64 = 40.0;

/// Render an `Html::Math` (the block `math` lower) — a
/// centred display equation wrapped in `<div class="wdoc-math …">`.
pub(crate) fn render_math_fundamental(map: &BTreeMap<String, Value>) -> String {
    render_math_block(
        &map_utf8(map, "latex").unwrap_or_default(),
        map_bool(map, "display").unwrap_or(true),
        &map_utf8_list(map, "class"),
        None,
    )
}

/// [`render_math_fundamental`] from typed parts — the content IR's `Math`
/// node, which carries its LaTeX, classes and anchor id rather than a
/// payload map.
pub(crate) fn render_math_block(
    latex: &str,
    display: bool,
    class: &[String],
    id: Option<&str>,
) -> String {
    let mut names = vec!["wdoc-math".to_string()];
    names.extend_from_slice(class);
    let class_attr = classes_attr_from_names(&names);
    let mut out = format!("<div{class_attr}");
    crate::render::append_attr(&mut out, "id", id);
    format!("{out}>{}</div>", math_svg(latex, display, false))
}

/// Render an `InlineSpan::Math` (the `$…$` / `$$…$$` inline patterns) —
/// a baseline-aligned equation wrapped in `<span class="wdoc-math-inline …">`.
pub(crate) fn render_inline_math(map: &BTreeMap<String, Value>) -> String {
    let latex = map_utf8(map, "latex").unwrap_or_default();
    let display = map_bool(map, "display").unwrap_or(false);
    let class_attr = wrapper_class(map, "wdoc-math-inline");
    let svg = math_svg(&latex, display, true);
    format!("<span{class_attr}>{svg}</span>")
}

/// Build the wrapper's ` class="base user…"` attribute.
fn wrapper_class(map: &BTreeMap<String, Value>, base: &str) -> String {
    let mut names = vec![base.to_string()];
    names.extend(map_utf8_list(map, "class"));
    classes_attr_from_names(&names)
}

fn map_bool(map: &BTreeMap<String, Value>, name: &str) -> Option<bool> {
    match map.get(name)? {
        Value::Bool(b) => Some(*b),
        _ => None,
    }
}

/// Lay out `latex` and emit a self-contained, theme-coloured `<svg>`.
/// `display` selects display vs text math style; `inline` controls the
/// sizing (baseline `vertical-align` for inline, plain box for block).
/// A parse/layout failure yields an inline error marker rather than a
/// build failure.
fn math_svg(latex: &str, display: bool, inline: bool) -> String {
    let parts = match render_pipeline(latex, display) {
        Ok(p) => p,
        Err(msg) => {
            return format!(
                "<span class=\"wdoc-math-error\" title=\"{}\">{}</span>",
                escape_html(&msg),
                escape_html(latex),
            );
        }
    };
    let total = parts.height_em + parts.depth_em;
    // Size in `em` so the equation tracks the surrounding font size.
    // Inline equations additionally drop their baseline by `depth` so
    // they sit on the text baseline (MathJax's convention).
    let style = if inline {
        format!(
            "height:{}em;width:{}em;vertical-align:-{}em",
            fmt(total),
            fmt(parts.width_em),
            fmt(parts.depth_em),
        )
    } else {
        format!("height:{}em;width:{}em", fmt(total), fmt(parts.width_em))
    };
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" style=\"{}\" \
         role=\"math\" aria-label=\"{}\">{}</svg>",
        fmt(parts.vb_w),
        fmt(parts.vb_h),
        style,
        escape_html(latex),
        parts.body,
    )
}

struct MathParts {
    /// SVG element contents (everything between the `<svg>` tags), with
    /// default-black paints rewritten to `currentColor`.
    body: String,
    vb_w: f64,
    vb_h: f64,
    width_em: f64,
    height_em: f64,
    depth_em: f64,
}

/// Run `parse → layout → to_display_list → render_to_svg`. Wrapped in
/// `catch_unwind` so a panic deep in the layout engine on pathological
/// input becomes an error marker, never a build crash.
fn render_pipeline(latex: &str, display: bool) -> Result<MathParts, String> {
    let raw = catch_unwind(AssertUnwindSafe(|| {
        let nodes = parse(latex).map_err(|e| e.to_string())?;
        let style = if display {
            MathStyle::Display
        } else {
            MathStyle::Text
        };
        let opts = LayoutOptions::default().with_style(style);
        let lbox = layout(&nodes, &opts);
        let dl = to_display_list(&lbox);
        let svg_opts = SvgOptions {
            font_size: EM,
            padding: 0.0,
            embed_glyphs: true,
            ..SvgOptions::default()
        };
        let svg = render_to_svg(&dl, &svg_opts);
        Ok::<_, String>((svg, dl.width, dl.height, dl.depth))
    }))
    .map_err(|_| "math layout panicked".to_string())??;

    let (svg, width_em, height_em, depth_em) = raw;
    let body = force_current_color(strip_svg_wrapper(&svg));
    Ok(MathParts {
        body,
        vb_w: width_em * EM,
        vb_h: (height_em + depth_em) * EM,
        width_em,
        height_em,
        depth_em,
    })
}

/// Drop RaTeX's outer `<svg …>` / `</svg>` so we can re-wrap with our own
/// viewBox + `em` sizing. The output has no nested `<svg>`, so the first
/// `>` ends the opening tag.
fn strip_svg_wrapper(svg: &str) -> &str {
    let start = svg.find('>').map(|i| i + 1).unwrap_or(0);
    let end = svg.rfind("</svg>").unwrap_or(svg.len());
    if start <= end { &svg[start..end] } else { svg }
}

/// Rewrite RaTeX's default black fill (`rgba(0,0,0,…)`) to
/// `currentColor` so equations follow the surrounding text colour. Any
/// non-black colour (an explicit `\textcolor{…}`) is left untouched.
fn force_current_color(body: &str) -> String {
    static BLACK: OnceLock<Regex> = OnceLock::new();
    let re = BLACK.get_or_init(|| Regex::new(r"rgba\(0,0,0,[^)]*\)").expect("valid regex"));
    re.replace_all(body, "currentColor").into_owned()
}

/// Format an `f64` for an attribute: a few decimals, trailing zeros
/// trimmed, never scientific notation.
fn fmt(n: f64) -> String {
    let s = format!("{n:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_self_contained_svg_with_current_color() {
        let svg = math_svg("E = mc^2", true, false);
        assert!(svg.contains("<svg"), "should emit an svg: {svg}");
        assert!(svg.contains("<path"), "glyphs should be embedded as paths");
        assert!(svg.contains("currentColor"), "default fill → currentColor");
        assert!(
            !svg.contains("rgba(0,0,0"),
            "no baked-in black fill should remain: {svg}"
        );
    }

    #[test]
    fn inline_math_gets_baseline_alignment() {
        let svg = math_svg("x^2", false, true);
        assert!(
            svg.contains("vertical-align:-"),
            "inline math aligns to baseline: {svg}"
        );
        assert!(svg.contains("em;width:"), "sized in em: {svg}");
    }

    #[test]
    fn explicit_color_is_preserved() {
        // `\textcolor{red}` → rgba(255,0,0,1); must survive the
        // black→currentColor rewrite.
        let svg = math_svg("\\textcolor{red}{x}", false, true);
        assert!(svg.contains("currentColor") || svg.contains("rgba(255"));
        assert!(svg.contains("rgba(255,0,0"), "explicit red kept: {svg}");
    }

    #[test]
    fn bad_latex_yields_error_marker_not_panic() {
        let svg = math_svg("\\frac{", false, true);
        assert!(
            svg.contains("wdoc-math-error"),
            "malformed input → inline error marker: {svg}"
        );
    }

    #[test]
    fn force_current_color_keeps_non_black() {
        let input = r#"<path fill="rgba(0,0,0,1)"/><path fill="rgba(255,0,0,1)"/>"#;
        let out = force_current_color(input);
        assert_eq!(
            out,
            r#"<path fill="currentColor"/><path fill="rgba(255,0,0,1)"/>"#
        );
    }
}
