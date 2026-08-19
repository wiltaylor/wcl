//! Colour-theme resolution.
//!
//! A `site` names a `theme` block via its `theme` symbol (see
//! `lib/theme.wcl`); this module finds that block and resolves it — plus an
//! accent hue and a `dark`/`light` mode — to the concrete role colours a
//! backend paints with. Backend-neutral on purpose: the PDF palette and the
//! wireframe/terminal SVG painters bake these colours in directly, since
//! they have no stylesheet to inherit from. The CSS reading of the same
//! theme (the `--wdoc-*` custom properties) lives in
//! [`crate::html::theme`].

use wcl_lang::{Block, Document};

use super::{field_symbol, field_utf8, label_string};

/// Theme used when a site declares none.
pub(crate) const DEFAULT_THEME: &str = "forge";

/// The hue roles a site's `accent` may name; anything else falls back to
/// `blue`.
pub(crate) const HUES: &[&str] = &[
    "red", "orange", "yellow", "green", "cyan", "blue", "purple", "pink",
];

/// Find a `theme` block by its inline name (built-in or user-declared),
/// falling back to the built-in `forge` when the name doesn't resolve.
pub(crate) fn find_theme<'a>(doc: &'a Document, name: &str) -> Option<Block<'a>> {
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
    /// Page background.
    pub bg: String,
    /// Alternate surface background.
    pub bg_alt: String,
    /// Inset surface background — code blocks and wells.
    pub bg_inset: String,
    /// Overlay background, for menus and dialogs.
    pub overlay: String,
    /// Border and rule colour.
    pub border: String,
    /// Body text colour.
    pub fg: String,
    /// Secondary text colour.
    pub fg_muted: String,
    /// Accent colour, for links and highlights.
    pub accent: String,
}

/// The selected UI theme for an application mock-up: a theme name, an accent
/// hue, and a mode (`dark`/`light`). Resolved per site from its `ui_*` fields
/// (falling back to the document `theme`/`accent`, dark) and overridable per
/// wireframe element.
#[derive(Clone)]
pub(crate) struct UiTheme {
    /// Name of the selected theme.
    pub theme: String,
    /// Accent colour, for links and highlights.
    pub accent: String,
    /// Light or dark mode.
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
