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

use std::collections::BTreeSet;
use std::fmt::Write as _;

use wcl_lang::{Block, Document};

use super::{RenderedCss, field_symbol, field_utf8, label_string, render_styles};

const DEFAULT_THEME: &str = "forge";

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

/// The hue roles a site's `accent` may name; anything else falls back to
/// `blue`.
const HUES: &[&str] = &[
    "red", "orange", "yellow", "green", "cyan", "blue", "purple", "pink",
];

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
/// falling back to the built-in `forge` when the name doesn't resolve.
fn find_theme<'a>(doc: &'a Document, name: &str) -> Option<Block<'a>> {
    let is_theme =
        |b: &Block<'_>, n: &str| b.kind() == "theme" && label_string(b).as_deref() == Some(n);
    doc.blocks()
        .find(|b| is_theme(b, name))
        .or_else(|| doc.blocks().find(|b| is_theme(b, DEFAULT_THEME)))
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
            theme: DEFAULT_THEME.to_string(),
            accent: "blue".to_string(),
            mode: "dark".to_string(),
        }
    }
}

/// The built-in Forge palette for a mode (lib/theme.wcl) — the fallback for any
/// role a custom palette omits, and for documents with no `theme`.
fn default_roles(mode: &str) -> ThemeRoles {
    if mode == "light" {
        ThemeRoles {
            bg: "#ffffff".into(),
            bg_alt: "#f4f5f7".into(),
            bg_inset: "#f4f5f7".into(),
            overlay: "#eaecef".into(),
            border: "#dcdfe4".into(),
            fg: "#3d4654".into(),
            fg_muted: "#6b7383".into(),
            accent: "#0069ca".into(),
        }
    } else {
        ThemeRoles {
            bg: "#11141a".into(),
            bg_alt: "#171b22".into(),
            bg_inset: "#171b22".into(),
            overlay: "#1e232c".into(),
            border: "#262c36".into(),
            fg: "#b7bdc8".into(),
            fg_muted: "#7c8593".into(),
            accent: "#2389e2".into(),
        }
    }
}

/// Read the UI theme a `site` selects: its `ui_theme`/`ui_accent`/`ui_mode`
/// fields, falling back to the document `theme`/`accent` (mode `dark`). A
/// site-less document gets the Forge-dark default.
pub(crate) fn resolve_ui_theme(site: Option<&Block<'_>>) -> UiTheme {
    let Some(site) = site else {
        return UiTheme::default();
    };
    let theme = field_symbol(site, "ui_theme")
        .or_else(|| field_symbol(site, "theme"))
        .unwrap_or_else(|| DEFAULT_THEME.to_string());
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
/// role colours: find the named `theme` block (fallback Forge), read the
/// matching-mode `palette` child, and fill any missing role from the Forge
/// palette of that mode. Unknown accent ⇒ blue; unknown mode ⇒ dark.
pub(crate) fn resolve_roles(doc: &Document, theme: &str, accent: &str, mode: &str) -> ThemeRoles {
    let mode = if mode == "light" { "light" } else { "dark" };
    let accent_hue = if HUES.contains(&accent) {
        accent
    } else {
        "blue"
    };
    let def = default_roles(mode);
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
        // the darker outer gutter (`bg`); fall back to `bg` then Forge.
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

    // Find the named `theme` block (built-in or user-declared), falling
    // back to the built-in `forge` when the name doesn't resolve.
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
    // Theme font stacks (mode-independent), after the palette blocks.
    theme_font_vars(&theme, &mut out);
    // The accent selector is generated because its declaration comes from
    // site data. Every static authored rule lives in WCL below it.
    writeln!(out, ":root{{--wdoc-accent:{accent_expr};}}").expect("write to String");
    if let Some(apply) = styles.get("wdoc-theme-apply") {
        out.push_str(&apply.text);
        classes.extend(apply.classes.iter().cloned());
    }
    Some(RenderedCss { text: out, classes })
}
