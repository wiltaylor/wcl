//! Colour-theme CSS emission.
//!
//! A `site` names a `theme` block via its `theme` symbol (see
//! `lib/theme.wcl`); this module finds that block + its `dark` / `light`
//! `palette` children and emits the themed stylesheet: the `--wdoc-*`
//! custom properties on `:root` for the dark palette (the default), under
//! `@media (prefers-color-scheme: light)` for the light palette, and per
//! explicit `:root[data-theme=…]` (the book toggle), then appends the
//! structured rules in `lib/theme-rules.wcl`.
//! `build.rs::site_css` splices the result between the library `class`
//! rules and the user ones, so a theme overrides the built-in defaults
//! (chart palette, syntax tokens) while user `class` blocks still win. Rust
//! only generates selectors whose declarations come from palette/site data.
//!
//! The backend-neutral half — resolving a theme name to concrete role
//! colours for the PDF and wireframe painters — lives in
//! [`crate::render::theme`]; this module is the CSS reading of it.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use wcl_lang::{Block, Document};

use crate::render::{
    DEFAULT_THEME, HUES, chain_field, chain_metric, chain_role, field_symbol, theme_chain,
};

use super::{RenderedCss, render_styles};

/// The 18 `Palette` roles, paired with the CSS custom-property suffix
/// (`bg_alt` → `--wdoc-bg-alt`). Emission order is fixed so output is
/// deterministic.
const ROLES: &[(&str, &str)] = &[
    ("bg", "bg"),
    ("book_bg", "book-bg"),
    ("bg_alt", "bg-alt"),
    ("bg_inset", "bg-inset"),
    ("overlay", "overlay"),
    ("border", "border"),
    ("border_strong", "border-strong"),
    ("fg", "fg"),
    ("fg_muted", "fg-muted"),
    ("fg_subtle", "fg-subtle"),
    ("heading", "heading"),
    ("selection", "selection"),
    // The palette's own accent is emitted as `--wdoc-accent-pal`; the
    // active `--wdoc-accent` points at it (or at a `site.accent` hue) via
    // the generated accent rule, so the override still wins by source order.
    ("accent", "accent-pal"),
    ("accent_2", "accent2"),
    ("link", "link"),
    ("on_accent", "on-accent"),
    ("syn_kw", "syn-kw"),
    ("syn_str", "syn-str"),
    ("syn_num", "syn-num"),
    ("syn_fn", "syn-fn"),
    ("syn_type", "syn-type"),
    ("syn_comment", "syn-comment"),
    ("syn_punct", "syn-punct"),
    ("red", "red"),
    ("orange", "orange"),
    ("yellow", "yellow"),
    ("green", "green"),
    ("cyan", "cyan"),
    ("blue", "blue"),
    ("purple", "purple"),
    ("pink", "pink"),
];

/// Emit one mode's palette as CSS custom properties, sourcing each role
/// from the nearest link of the theme's `extends` chain that states it.
/// Returns whether anything was emitted, which is what tells a chain that
/// contributes no colour at all from a healthy one.
fn palette_vars(chain: &[Block<'_>], mode: &str, out: &mut String) -> bool {
    let mut any = false;
    for (field, var) in ROLES {
        if let Some(c) = chain_role(chain, mode, field) {
            write!(out, "--wdoc-{var}:{c};").expect("write to String");
            any = true;
        }
    }
    any
}

/// Emit `:root{ --wdoc-font-*: … }` for the font stacks a `theme` sets,
/// each taken from the nearest link of its `extends` chain that states it.
/// Mode-independent, so written once and placed after the `wdoc-fonts`
/// lib defaults — a theme (e.g. `paper`) overrides them by source order.
fn theme_font_vars(chain: &[Block<'_>], out: &mut String) {
    let mut decl = String::new();
    for (field, var) in [
        ("font_head", "font-head"),
        ("font_body", "font-body"),
        ("font_mono", "font-mono"),
    ] {
        if let Some(v) = chain_field(chain, field) {
            write!(decl, "--wdoc-{var}:{v};").expect("write to String");
        }
    }
    if !decl.is_empty() {
        writeln!(out, ":root{{{decl}}}").expect("write to String");
    }
}

/// The `metrics` fields, paired with their CSS custom-property suffix.
/// Emission order is fixed so output is deterministic.
const METRICS: &[(&str, &str)] = &[
    ("body_size", "body-size"),
    ("line_height", "line-height"),
    ("measure", "measure"),
    ("h1", "h1"),
    ("h2", "h2"),
    ("h3", "h3"),
    ("h4", "h4"),
    ("h5", "h5"),
    ("h6", "h6"),
];

/// Emit `:root{ --wdoc-body-size: … }` for the type metrics a `theme`
/// sets, each taken from the nearest link of its `extends` chain that
/// states it. Mode-independent, like the font stacks. Nothing is emitted
/// for a metric no link states: the bundled rules read every metric as
/// `var(--wdoc-…, <default>)`, so silence means the shipped constant
/// rather than a broken declaration.
fn theme_metric_vars(chain: &[Block<'_>], out: &mut String) {
    let mut decl = String::new();
    for (field, var) in METRICS {
        if let Some(v) = chain_metric(chain, field) {
            write!(decl, "--wdoc-{var}:{v};").expect("write to String");
        }
    }
    if !decl.is_empty() {
        writeln!(out, ":root{{{decl}}}").expect("write to String");
    }
}

/// The themed `<style>` content for one site, or `None` when there is no
/// `site` block (bare documents stay unthemed) or no `theme` block can be
/// resolved. A `site` without an explicit `theme` defaults to `forge`; an
/// unknown name also falls back to `forge`.
pub(crate) fn site_theme_css(
    doc: &Document,
    site_block: Option<&Block<'_>>,
) -> Option<RenderedCss> {
    let block = site_block?;

    // The `theme` symbol names a `theme` block; default to `forge`.
    let name = field_symbol(block, "theme").unwrap_or_else(|| DEFAULT_THEME.to_string());

    // The active accent: a `site.accent` hue wins (re-points `--wdoc-accent`
    // at that hue var); otherwise the theme's own `accent` role drives it
    // (via `--wdoc-accent-pal`). So a theme looks "designed" out of the box,
    // and `accent = :green` still overrides on demand.
    let accent_expr = match field_symbol(block, "accent") {
        Some(a) if HUES.contains(&a.as_str()) => format!("var(--wdoc-{a})"),
        _ => "var(--wdoc-accent-pal)".to_string(),
    };

    // The named `theme` block (built-in or user-declared) and everything it
    // inherits from, nearest first — falling back to the built-in `forge`
    // when the name doesn't resolve.
    let chain = theme_chain(doc, &name);
    if chain.is_empty() {
        return None;
    }

    // Pull the `--wdoc-*` vars for each mode, role by role along the chain.
    let mut dv = String::new();
    let mut lv = String::new();
    let dark = palette_vars(&chain, "dark", &mut dv);
    let light = palette_vars(&chain, "light", &mut lv);

    // A mode with no colour at all is not a subtle defect: the bundled
    // stylesheet reads most colour roles as a bare `var(--wdoc-…)` with no
    // fallback, so the page renders as browser defaults — white on black,
    // no accent — while the build reports success. Say so instead.
    for (mode, ok) in [("dark", dark), ("light", light)] {
        if !ok {
            crate::render::record_render_warning(format!(
                "theme \"{name}\" states no {mode} palette, directly or through \
                 `extends`, so every `--wdoc-*` colour is undeclared and the page \
                 renders unstyled — add a `palette {mode} {{ … }}`, or inherit one \
                 with `extends = :{DEFAULT_THEME}`"
            ));
        }
    }

    let styles = render_styles(doc);
    // The two subtree palettes below are written here rather than in WCL
    // (their declarations come from palette data), so this module declares
    // their class names for the lint itself.
    let mut classes = BTreeSet::from([
        "wdoc-theme-dark".to_string(),
        "wdoc-theme-light".to_string(),
    ]);
    let mut out = String::new();
    // Default font stacks (themed sites only — keeps a site-less doc bare).
    // A theme's `font_*` fields override these via `theme_font_vars` below.
    if let Some(defaults) = styles.get("wdoc-theme-font-defaults") {
        out.push_str(&defaults.text);
        out.push('\n');
        classes.extend(defaults.classes.iter().cloned());
    }
    writeln!(out, ":root{{{dv}}}").expect("write to String");
    writeln!(out, "@media (prefers-color-scheme: light){{:root{{{lv}}}}}")
        .expect("write to String");
    writeln!(out, ":root[data-theme=\"dark\"]{{{dv}}}").expect("write to String");
    writeln!(out, ":root[data-theme=\"light\"]{{{lv}}}").expect("write to String");
    // Subtree-scoped palettes: a wrapper carrying `.wdoc-theme-light` /
    // `.wdoc-theme-dark` re-defines the `--wdoc-*` vars for its descendants,
    // so a doc can show the *same* content under both palettes at once
    // (the `demo` block's side-by-side preview) regardless of the reader's
    // global toggle. Custom properties inherit, so a closer ancestor wins.
    writeln!(out, ".wdoc-theme-dark{{{dv}}}").expect("write to String");
    writeln!(out, ".wdoc-theme-light{{{lv}}}").expect("write to String");
    // Theme font stacks and type metrics (mode-independent), after the
    // palette blocks.
    theme_font_vars(&chain, &mut out);
    theme_metric_vars(&chain, &mut out);
    // The accent selector is generated because its declaration comes from
    // site data. Every static authored rule lives in WCL below it.
    writeln!(out, ":root{{--wdoc-accent:{accent_expr};}}").expect("write to String");
    if let Some(apply) = styles.get("wdoc-theme-apply") {
        out.push_str(&apply.text);
        classes.extend(apply.classes.iter().cloned());
    }
    Some(RenderedCss { text: out, classes })
}
