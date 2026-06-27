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
    // the APPLY rule, so the override still wins by source order.
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

/// Default `--wdoc-font-*` stacks (matching the `wdoc-fonts` `@font-face`
/// families). Emitted only for themed sites; a theme's `font_*` fields
/// override these by source order.
const FONT_DEFAULTS: &str = ":root{--wdoc-font-head:'IBM Plex Sans',system-ui,sans-serif;--wdoc-font-body:'Source Serif 4',Georgia,serif;--wdoc-font-mono:'JetBrains Mono',ui-monospace,monospace;}\n";

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
:root{--wdoc-accent:{ACCENT_EXPR};}
body,.wdoc-body{background:var(--wdoc-bg);color:var(--wdoc-fg);font-family:var(--wdoc-font-body);font-size:17px;line-height:1.7;}
::selection{background:color-mix(in srgb, var(--wdoc-selection) 28%, transparent);}
a,.link{color:var(--wdoc-link);text-decoration:none;border-bottom:1px solid color-mix(in srgb, var(--wdoc-link) 35%, transparent);}
a:hover,.link:hover{border-bottom-color:var(--wdoc-link);}
.heading-1,.heading-2,.heading-3,.heading-4,.heading-5,.heading-6{color:var(--wdoc-heading);font-family:var(--wdoc-font-head);}
strong,.bold{color:var(--wdoc-heading);}
.code,code,kbd{font-family:var(--wdoc-font-mono);background:var(--wdoc-bg-alt);border:1px solid var(--wdoc-border);border-radius:4px;padding:0.05em 0.35em;}
kbd{border-color:var(--wdoc-border-strong);border-bottom-width:2px;color:var(--wdoc-fg-muted);}
blockquote{border-left:3px solid var(--wdoc-accent);color:var(--wdoc-fg-muted);}
hr{border:none;border-top:1px solid var(--wdoc-border);}
figcaption{color:var(--wdoc-fg-muted);}
.code-card{background:var(--wdoc-bg-alt);border:1px solid var(--wdoc-border);}
.code-filename{color:var(--wdoc-fg-muted);border-bottom:1px solid var(--wdoc-border);font-family:var(--wdoc-font-mono);}
.code-lang{color:var(--wdoc-fg-subtle);}
.code-dots span{background:color-mix(in srgb, var(--wdoc-fg) 22%, transparent);}
.code-card .code-line::before{color:var(--wdoc-fg-subtle);}
.heading-marker{color:var(--wdoc-accent);font-family:var(--wdoc-font-mono);}
.book-kicker{color:var(--wdoc-accent);font-family:var(--wdoc-font-mono);}
.book-meta{color:var(--wdoc-fg-muted);border-top:1px solid var(--wdoc-border);}
.book-rail-title,.book-onpage-title{color:var(--wdoc-fg-subtle);}
.book-onpage-link{color:var(--wdoc-fg-muted);border-left:2px solid transparent;}
.book-onpage-link:hover{color:var(--wdoc-fg);}
.book-onpage-link.active{color:var(--wdoc-accent);border-left-color:var(--wdoc-accent);}
.footnotes{border-top:1px solid var(--wdoc-border);color:var(--wdoc-fg-muted);}
.footnote-ref{color:var(--wdoc-accent);}
.wdoc-badge{font-family:var(--wdoc-font-head);background:color-mix(in srgb, var(--wdoc-accent) 16%, var(--wdoc-book-bg));color:var(--wdoc-accent);border:1px solid color-mix(in srgb, var(--wdoc-accent) 32%, transparent);}
pre.code-block{background:var(--wdoc-bg-alt);color:var(--wdoc-fg);border-color:var(--wdoc-border);font-family:var(--wdoc-font-mono);}
.tok-comment{color:var(--wdoc-syn-comment);}
.tok-keyword{color:var(--wdoc-syn-kw);}
.tok-storage{color:var(--wdoc-syn-kw);}
.tok-storage.tok-type{color:var(--wdoc-syn-type);}
.tok-string{color:var(--wdoc-syn-str);}
.tok-constant{color:var(--wdoc-syn-num);}
.tok-constant.tok-numeric{color:var(--wdoc-syn-num);}
.tok-entity.tok-name.tok-function{color:var(--wdoc-syn-fn);}
.tok-entity.tok-name.tok-tag{color:var(--wdoc-red);}
.tok-entity.tok-name.tok-class{color:var(--wdoc-syn-type);}
.tok-variable{color:var(--wdoc-fg);}
.tok-support{color:var(--wdoc-syn-type);}
.tok-punctuation{color:var(--wdoc-syn-punct);}
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
.wdoc-boundary{fill:none;stroke:var(--wdoc-fg-muted);stroke-dasharray:6 4;}
.wdoc-boundary-label{fill:var(--wdoc-fg-muted);font-weight:600;}
.wdoc-edge-label{fill:var(--wdoc-fg-muted);}
.wdoc-participant{fill:var(--wdoc-bg-alt);stroke:var(--wdoc-blue);}
.wdoc-participant-line{stroke:var(--wdoc-blue);}
.wdoc-lifeline{stroke:var(--wdoc-fg-muted);}
.wdoc-seq-message{stroke:var(--wdoc-fg);}
.wdoc-seq-arrow{fill:var(--wdoc-fg);}
.wdoc-seq-text{fill:var(--wdoc-fg);}
.wdoc-note{fill:var(--wdoc-bg-alt);stroke:var(--wdoc-yellow);}
.wdoc-note-text{fill:var(--wdoc-fg);}
.wdoc-state{fill:var(--wdoc-bg-alt);stroke:var(--wdoc-blue);}
.wdoc-state-initial{fill:var(--wdoc-fg);}
.callout.note{--callout-accent:var(--wdoc-blue);}
.callout.info{--callout-accent:var(--wdoc-cyan);}
.callout.tip{--callout-accent:var(--wdoc-green);}
.callout.warning{--callout-accent:var(--wdoc-yellow);}
.callout.error{--callout-accent:var(--wdoc-red);}
.callout.success{--callout-accent:var(--wdoc-green);}
.callout{background:color-mix(in srgb, var(--callout-accent) 9%, var(--wdoc-book-bg));}
.wdoc-terminal-error,.wdoc-math-error{color:var(--wdoc-red);}
.wdoc-table th{background:var(--wdoc-bg-alt);color:var(--wdoc-fg-muted);font-family:var(--wdoc-font-head);}
.wdoc-table th,.wdoc-table td{border-color:var(--wdoc-border);}
.wdoc-table tbody tr:nth-child(even){background:color-mix(in srgb, var(--wdoc-fg) 4%, transparent);}
.wdoc-map-card{background:var(--wdoc-bg-alt);color:var(--wdoc-fg);border-color:var(--wdoc-border);}
.wdoc-card{background:var(--wdoc-bg-alt);color:var(--wdoc-fg);border-color:var(--wdoc-border);}
.wdoc-preview{background:var(--wdoc-bg);color:var(--wdoc-fg);border-color:var(--wdoc-border);}
.wdoc-node-table-frame{fill:var(--wdoc-bg-alt);stroke:var(--wdoc-border);}
.wdoc-node-table-sep{stroke:var(--wdoc-border);}
.wdoc-node-table-port{fill:var(--wdoc-blue);stroke:var(--wdoc-border);}
.wdoc-node-table-title,.wdoc-node-row{color:var(--wdoc-fg);}";

/// Append `--wdoc-<role>:<hex>;` for every role the palette block sets.
fn palette_vars(pal: &Block<'_>, out: &mut String) {
    for (field, var) in ROLES {
        if let Some(c) = field_utf8(pal, field) {
            write!(out, "--wdoc-{var}:{c};").expect("write to String");
        }
    }
}

/// Emit `:root{ --wdoc-font-*: … }` for the font stacks a `theme` sets.
/// Mode-independent, so written once and placed after the `wdoc-fonts`
/// lib defaults — a theme (e.g. `paper`) overrides them by source order.
fn theme_font_vars(theme: &Block<'_>, out: &mut String) {
    let mut decl = String::new();
    for (field, var) in [
        ("font_head", "font-head"),
        ("font_body", "font-body"),
        ("font_mono", "font-mono"),
    ] {
        if let Some(v) = field_utf8(theme, field) {
            write!(decl, "--wdoc-{var}:{v};").expect("write to String");
        }
    }
    if !decl.is_empty() {
        writeln!(out, ":root{{{decl}}}").expect("write to String");
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
            bg: "#eceff4".into(), // nord light `book_bg` (panel surface)
            bg_alt: "#e5e9f0".into(),
            bg_inset: "#e5e9f0".into(),
            overlay: "#d8dee9".into(),
            border: "#d8dee9".into(),
            fg: "#3b4252".into(),
            fg_muted: "#566173".into(),
            accent: "#5e81ac".into(), // nord light `blue`
        }
    } else {
        ThemeRoles {
            bg: "#2e3440".into(), // nord dark `book_bg` (panel surface)
            bg_alt: "#3b4252".into(),
            bg_inset: "#3b4252".into(),
            overlay: "#434c5e".into(),
            border: "#3b4252".into(),
            fg: "#d8dee9".into(),
            fg_muted: "#a0aabe".into(),
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
        // The wireframe panel sits on the reading surface (`book_bg`), not
        // the darker outer gutter (`bg`); fall back to `bg` then nord.
        bg: field_utf8(&pal, "book_bg")
            .or_else(|| field_utf8(&pal, "bg"))
            .unwrap_or_else(|| def.bg.clone()),
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

    // The active accent: a `site.accent` hue wins (re-points `--wdoc-accent`
    // at that hue var); otherwise the theme's own `accent` role drives it
    // (via `--wdoc-accent-pal`). So a theme looks "designed" out of the box,
    // and `accent = :green` still overrides on demand.
    let accent_expr = match field_symbol(block, "accent") {
        Some(a) if HUES.contains(&a.as_str()) => format!("var(--wdoc-{a})"),
        _ => "var(--wdoc-accent-pal)".to_string(),
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
    // Default font stacks (themed sites only — keeps a site-less doc bare).
    // A theme's `font_*` fields override these via `theme_font_vars` below.
    out.push_str(FONT_DEFAULTS);
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
    // Theme font stacks (mode-independent), after the palette blocks.
    theme_font_vars(&theme, &mut out);
    out.push_str(&APPLY.replace("{ACCENT_EXPR}", &accent_expr));
    Some(out)
}
