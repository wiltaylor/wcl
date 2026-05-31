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
.wdoc-terminal-error,.wdoc-math-error{color:var(--wdoc-red);}
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

/// Find a `theme` block by its inline name (built-in or user-declared),
/// falling back to the built-in `nord` when the name doesn't resolve.
fn find_theme<'a>(doc: &'a Document, name: &str) -> Option<Block<'a>> {
    let is_theme =
        |b: &Block<'_>, n: &str| b.kind() == "theme" && label_string(b).as_deref() == Some(n);
    doc.blocks()
        .find(|b| is_theme(b, name))
        .or_else(|| doc.blocks().find(|b| is_theme(b, "nord")))
}

/// Concrete colours for the theme roles the wireframe renderer bakes into
/// its SVG (it has no CSS / `currentColor` to lean on — the PDF embed would
/// rewrite `currentColor` to the document fg). Resolved from a chosen
/// `theme` + `mode` (`dark`/`light`) so a wireframe is a self-contained
/// themed panel on any page, mirroring the terminal window.
pub(crate) struct ThemeRoles {
    pub bg: String,
    pub bg_alt: String,
    pub bg_inset: String,
    pub overlay: String,
    pub border: String,
    pub fg: String,
    pub fg_muted: String,
    pub accent: String,
}

/// The selected UI theme for an application mock-up: a theme name, an accent
/// hue, and a mode (`dark`/`light`). Resolved per site from its `ui_*` fields
/// (falling back to the document `theme`/`accent`, dark) and overridable per
/// wireframe element.
#[derive(Clone)]
pub(crate) struct UiTheme {
    pub theme: String,
    pub accent: String,
    pub mode: String,
}

impl Default for UiTheme {
    fn default() -> Self {
        UiTheme {
            theme: "nord".to_string(),
            accent: "blue".to_string(),
            mode: "dark".to_string(),
        }
    }
}

/// The built-in Nord palette for a mode (lib/theme.wcl) — the fallback for any
/// role a custom palette omits, and for documents with no `theme`.
fn nord_roles(mode: &str) -> ThemeRoles {
    if mode == "light" {
        ThemeRoles {
            bg: "#eceff4".into(),
            bg_alt: "#e5e9f0".into(),
            bg_inset: "#d8dee9".into(),
            overlay: "#d8dee9".into(),
            border: "#d8dee9".into(),
            fg: "#2e3440".into(),
            fg_muted: "#4c566a".into(),
            accent: "#5e81ac".into(), // nord light `blue`
        }
    } else {
        ThemeRoles {
            bg: "#2e3440".into(),
            bg_alt: "#3b4252".into(),
            bg_inset: "#272c36".into(),
            overlay: "#434c5e".into(),
            border: "#4c566a".into(),
            fg: "#d8dee9".into(),
            fg_muted: "#9aa5b8".into(),
            accent: "#81a1c1".into(), // nord dark `blue`
        }
    }
}

/// Read the UI theme a `site` selects: its `ui_theme`/`ui_accent`/`ui_mode`
/// fields, falling back to the document `theme`/`accent` (mode `dark`). A
/// site-less document gets the Nord-dark default.
pub(crate) fn resolve_ui_theme(site: Option<&Block<'_>>) -> UiTheme {
    let Some(site) = site else {
        return UiTheme::default();
    };
    let theme = field_symbol(site, "ui_theme")
        .or_else(|| field_symbol(site, "theme"))
        .unwrap_or_else(|| "nord".to_string());
    let accent_raw = field_symbol(site, "ui_accent")
        .or_else(|| field_symbol(site, "accent"))
        .unwrap_or_default();
    let accent = if HUES.contains(&accent_raw.as_str()) {
        accent_raw
    } else {
        "blue".to_string()
    };
    let mode = match field_symbol(site, "ui_mode").as_deref() {
        Some("light") => "light".to_string(),
        _ => "dark".to_string(),
    };
    UiTheme {
        theme,
        accent,
        mode,
    }
}

/// Resolve a `theme` name + `accent` hue + `mode` (`dark`/`light`) to concrete
/// role colours: find the named `theme` block (fallback nord), read the
/// matching-mode `palette` child, and fill any missing role from the Nord
/// palette of that mode. Unknown accent ⇒ blue; unknown mode ⇒ dark.
pub(crate) fn resolve_roles(doc: &Document, theme: &str, accent: &str, mode: &str) -> ThemeRoles {
    let mode = if mode == "light" { "light" } else { "dark" };
    let accent_hue = if HUES.contains(&accent) {
        accent
    } else {
        "blue"
    };
    let def = nord_roles(mode);
    let Some(theme) = find_theme(doc, theme) else {
        return def;
    };
    let Some(pal) = theme
        .blocks()
        .find(|b| b.kind() == "palette" && label_string(b).as_deref() == Some(mode))
    else {
        return def;
    };
    let role =
        |f: &str, fallback: &str| field_utf8(&pal, f).unwrap_or_else(|| fallback.to_string());
    ThemeRoles {
        bg: role("bg", &def.bg),
        bg_alt: role("bg_alt", &def.bg_alt),
        bg_inset: role("bg_inset", &def.bg_inset),
        overlay: role("overlay", &def.overlay),
        border: role("border", &def.border),
        fg: role("fg", &def.fg),
        fg_muted: role("fg_muted", &def.fg_muted),
        accent: role(accent_hue, &def.accent),
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
    let theme = find_theme(doc, &name)?;

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
