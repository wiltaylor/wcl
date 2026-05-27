//! Colour-theme CSS emission.
//!
//! A `site` selects a `ColourTheme` value (see `lib/theme.wcl`); this
//! module reads that value's record and emits the themed stylesheet: the
//! `--wdoc-*` custom properties on `:root` for the dark palette (the
//! default), under `@media (prefers-color-scheme: light)` for the light
//! palette, and per explicit `:root[data-theme=…]` (the book toggle),
//! plus the [`APPLY`] rules that paint everything the renderer produces
//! with `var(--wdoc-*)`. `build.rs::site_css` splices the result between
//! the library `class` rules and the user ones, so a theme overrides the
//! built-in defaults (chart palette, syntax tokens) while user `class`
//! blocks still win. The palette *data* lives in WCL; only this CSS
//! template is Rust (mirroring the other `*_CSS` constants).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use wcl_lang::{Block, Document, Value, VariantPayload};

use super::field_symbol;

/// The 18 `Palette` roles, paired with the CSS custom-property suffix
/// (`bg_alt` → `--wdoc-bg-alt`). Emission order is fixed so output is
/// deterministic.
const ROLES: &[(&str, &str)] = &[
    ("bg", "bg"),
    ("bg_alt", "bg-alt"),
    ("bg_inset", "bg-inset"),
    ("overlay", "overlay"),
    ("border", "border"),
    ("fg", "fg"),
    ("fg_muted", "fg-muted"),
    ("fg_subtle", "fg-subtle"),
    ("heading", "heading"),
    ("selection", "selection"),
    ("red", "red"),
    ("orange", "orange"),
    ("yellow", "yellow"),
    ("green", "green"),
    ("cyan", "cyan"),
    ("blue", "blue"),
    ("purple", "purple"),
    ("pink", "pink"),
];

/// The hue roles a site's `accent` may name; anything else falls back to
/// `blue`.
const HUES: &[&str] = &[
    "red", "orange", "yellow", "green", "cyan", "blue", "purple", "pink",
];

/// Maps the theme vars onto everything the renderer emits. Selectors
/// match the existing stylesheets/classes (so the cascade overrides
/// them) and the syntax-token classes from `assets/code-theme.css`
/// (compound selectors matched at equal specificity). Mode-independent
/// — it only references `var(--wdoc-*)` — so it is emitted once.
/// `{ACCENT}` is replaced with the chosen hue.
const APPLY: &str = "\
:root{--wdoc-accent:var(--wdoc-{ACCENT});}
body,.wdoc-body{background:var(--wdoc-bg);color:var(--wdoc-fg);}
::selection{background:var(--wdoc-selection);}
a,.link{color:var(--wdoc-accent);}
.heading-1,.heading-2,.heading-3,.heading-4,.heading-5,.heading-6{color:var(--wdoc-heading);}
pre.code-block{background:var(--wdoc-bg-inset);color:var(--wdoc-fg);border-color:var(--wdoc-border);}
.tok-comment{color:var(--wdoc-fg-subtle);}
.tok-keyword{color:var(--wdoc-purple);}
.tok-storage{color:var(--wdoc-purple);}
.tok-storage.tok-type{color:var(--wdoc-yellow);}
.tok-string{color:var(--wdoc-green);}
.tok-constant{color:var(--wdoc-orange);}
.tok-constant.tok-numeric{color:var(--wdoc-orange);}
.tok-entity.tok-name.tok-function{color:var(--wdoc-blue);}
.tok-entity.tok-name.tok-tag{color:var(--wdoc-red);}
.tok-entity.tok-name.tok-class{color:var(--wdoc-yellow);}
.tok-variable{color:var(--wdoc-fg);}
.tok-support{color:var(--wdoc-cyan);}
.tok-punctuation{color:var(--wdoc-fg-muted);}
.tok-meta.tok-tag{color:var(--wdoc-red);}
.tok-invalid{color:var(--wdoc-bg);background:var(--wdoc-red);}
.wdoc-series-1{fill:var(--wdoc-blue);stroke:var(--wdoc-blue);}
.wdoc-series-2{fill:var(--wdoc-green);stroke:var(--wdoc-green);}
.wdoc-series-3{fill:var(--wdoc-yellow);stroke:var(--wdoc-yellow);}
.wdoc-series-4{fill:var(--wdoc-red);stroke:var(--wdoc-red);}
.wdoc-series-5{fill:var(--wdoc-purple);stroke:var(--wdoc-purple);}
.wdoc-series-6{fill:var(--wdoc-cyan);stroke:var(--wdoc-cyan);}
.wdoc-series-7{fill:var(--wdoc-orange);stroke:var(--wdoc-orange);}
.wdoc-series-8{fill:var(--wdoc-pink);stroke:var(--wdoc-pink);}
.callout.note{--callout-accent:var(--wdoc-blue);}
.callout.info{--callout-accent:var(--wdoc-cyan);}
.callout.tip{--callout-accent:var(--wdoc-green);}
.callout.warning{--callout-accent:var(--wdoc-yellow);}
.callout.error{--callout-accent:var(--wdoc-red);}
.callout.success{--callout-accent:var(--wdoc-green);}
.wdoc-table th{background:var(--wdoc-bg-alt);}
.wdoc-table th,.wdoc-table td{border-color:var(--wdoc-border);}";

/// Borrow the named fields of a record-shaped value — a single-variant
/// union (`Palette::Of {…}` / `ColourTheme::Of {…}`) or a bare record.
fn record_fields(v: &Value) -> Option<&BTreeMap<String, Value>> {
    match v {
        Value::Variant {
            payload: VariantPayload::Record(m),
            ..
        } => Some(m),
        Value::Record { fields, .. } => Some(fields),
        _ => None,
    }
}

/// Append `--wdoc-<role>:<hex>;` for every role present in `pal`.
fn palette_vars(pal: &BTreeMap<String, Value>, out: &mut String) {
    for (field, var) in ROLES {
        if let Some(Value::Utf8(c) | Value::Ascii(c)) = pal.get(*field) {
            write!(out, "--wdoc-{var}:{c};").expect("write to String");
        }
    }
}

/// The themed `<style>` content for one site, or `None` when there is no
/// `site` block (bare documents stay unthemed) or the selected theme
/// value is malformed. A `site` without an explicit `theme` defaults to
/// `nord`.
pub(crate) fn site_theme_css(doc: &Document, site_block: Option<&Block<'_>>) -> Option<String> {
    let block = site_block?;

    // The selected `ColourTheme` value, or the `nord` default. `nord`
    // is a top-level `let` in the imported stdlib, so it resolves
    // through the document's root scope.
    let theme = match block.field("theme").and_then(|f| f.value().ok()) {
        Some(v) if !matches!(v, Value::None) => v.clone(),
        _ => {
            let expr = wcl_lang::parse_expr("nord", "<wdoc-theme>").ok()?;
            doc.eval_expr(&expr).ok()?
        }
    };

    let accent_raw = field_symbol(block, "accent").unwrap_or_default();
    let accent = if HUES.contains(&accent_raw.as_str()) {
        accent_raw.as_str()
    } else {
        "blue"
    };

    let theme = record_fields(&theme)?;
    let dark = record_fields(theme.get("dark")?)?;
    let light = record_fields(theme.get("light")?)?;

    let mut dv = String::new();
    palette_vars(dark, &mut dv);
    let mut lv = String::new();
    palette_vars(light, &mut lv);

    let mut out = String::new();
    writeln!(out, ":root{{{dv}}}").expect("write to String");
    writeln!(out, "@media (prefers-color-scheme: light){{:root{{{lv}}}}}")
        .expect("write to String");
    writeln!(out, ":root[data-theme=\"dark\"]{{{dv}}}").expect("write to String");
    writeln!(out, ":root[data-theme=\"light\"]{{{lv}}}").expect("write to String");
    out.push_str(&APPLY.replace("{ACCENT}", accent));
    Some(out)
}
