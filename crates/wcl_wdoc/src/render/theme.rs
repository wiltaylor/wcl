//! Colour-theme CSS emission.
//!
//! A `site` names a `theme` block via its `theme` symbol (see
//! `lib/theme.wcl`); this module finds that block + its `dark` / `light`
//! `palette` children and emits the themed stylesheet: the `--wdoc-*`
//! custom properties on `:root` for the dark palette (the default), under
//! `@media (prefers-color-scheme: light)` for the light palette, and per
//! explicit `:root[data-theme=…]` (the book toggle), plus the [`APPLY`]
//! rules that paint everything the renderer produces with `var(--wdoc-*)`.
//! `build.rs::site_css` splices the result between the library `class`
//! rules and the user ones, so a theme overrides the built-in defaults
//! (chart palette, syntax tokens) while user `class` blocks still win. The
//! palette *data* lives in WCL; only this CSS template is Rust (mirroring
//! the other `*_CSS` constants).

use std::fmt::Write as _;

use wcl_lang::{Block, Document};

use super::{field_symbol, field_utf8, label_string};

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
.wdoc-annotation{fill:var(--wdoc-accent);}
.wdoc-point-label{fill:var(--wdoc-fg-muted);}
.wdoc-process{fill:var(--wdoc-bg-alt);stroke:var(--wdoc-blue);}
.wdoc-decision{fill:var(--wdoc-bg-alt);stroke:var(--wdoc-orange);}
.wdoc-terminator{fill:var(--wdoc-bg-alt);stroke:var(--wdoc-green);}
.wdoc-node{fill:var(--wdoc-bg-alt);stroke:var(--wdoc-border);}
.wdoc-shape-text{fill:var(--wdoc-fg);}
.callout.note{--callout-accent:var(--wdoc-blue);}
.callout.info{--callout-accent:var(--wdoc-cyan);}
.callout.tip{--callout-accent:var(--wdoc-green);}
.callout.warning{--callout-accent:var(--wdoc-yellow);}
.callout.error{--callout-accent:var(--wdoc-red);}
.callout.success{--callout-accent:var(--wdoc-green);}
.wdoc-table th{background:var(--wdoc-bg-alt);}
.wdoc-table th,.wdoc-table td{border-color:var(--wdoc-border);}
.wdoc-map-card{background:var(--wdoc-bg-alt);color:var(--wdoc-fg);border-color:var(--wdoc-border);}
.wdoc-card{background:var(--wdoc-bg-alt);color:var(--wdoc-fg);border-color:var(--wdoc-border);}
.bold{color:var(--wdoc-orange);}
.code{background:var(--wdoc-bg-inset);border-radius:4px;padding:0.05em 0.3em;}";

/// Append `--wdoc-<role>:<hex>;` for every role the palette block sets.
fn palette_vars(pal: &Block<'_>, out: &mut String) {
    for (field, var) in ROLES {
        if let Some(c) = field_utf8(pal, field) {
            write!(out, "--wdoc-{var}:{c};").expect("write to String");
        }
    }
}

/// The themed `<style>` content for one site, or `None` when there is no
/// `site` block (bare documents stay unthemed) or no `theme` block can be
/// resolved. A `site` without an explicit `theme` defaults to `nord`; an
/// unknown name also falls back to `nord`.
pub(crate) fn site_theme_css(doc: &Document, site_block: Option<&Block<'_>>) -> Option<String> {
    let block = site_block?;

    // The `theme` symbol names a `theme` block; default to `nord`.
    let name = field_symbol(block, "theme").unwrap_or_else(|| "nord".to_string());

    let accent_raw = field_symbol(block, "accent").unwrap_or_default();
    let accent = if HUES.contains(&accent_raw.as_str()) {
        accent_raw.as_str()
    } else {
        "blue"
    };

    // Find the named `theme` block (built-in or user-declared), falling
    // back to the built-in `nord` when the name doesn't resolve.
    let is_theme =
        |b: &Block<'_>, n: &str| b.kind() == "theme" && label_string(b).as_deref() == Some(n);
    let theme = doc
        .blocks()
        .find(|b| is_theme(b, &name))
        .or_else(|| doc.blocks().find(|b| is_theme(b, "nord")))?;

    // Pull the `--wdoc-*` vars from its `dark` / `light` palette children.
    let mut dv = String::new();
    let mut lv = String::new();
    for pal in theme.blocks().filter(|b| b.kind() == "palette") {
        match label_string(&pal).as_deref() {
            Some("dark") => palette_vars(&pal, &mut dv),
            Some("light") => palette_vars(&pal, &mut lv),
            _ => {}
        }
    }

    let mut out = String::new();
    writeln!(out, ":root{{{dv}}}").expect("write to String");
    writeln!(out, "@media (prefers-color-scheme: light){{:root{{{lv}}}}}")
        .expect("write to String");
    writeln!(out, ":root[data-theme=\"dark\"]{{{dv}}}").expect("write to String");
    writeln!(out, ":root[data-theme=\"light\"]{{{lv}}}").expect("write to String");
    out.push_str(&APPLY.replace("{ACCENT}", accent));
    Some(out)
}
