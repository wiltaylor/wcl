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

/// How many `extends` links a chain may follow before it is treated as
/// malformed and truncated. A cycle is caught by name before this bites;
/// the cap is the backstop for a chain nobody meant to write.
const MAX_THEME_DEPTH: usize = 16;

/// Find a `theme` block by its inline name, with no fallback.
fn find_theme_exact<'a>(doc: &'a Document, name: &str) -> Option<Block<'a>> {
    doc.blocks()
        .find(|b| b.kind() == "theme" && label_string(b).as_deref() == Some(name))
}

/// Find a `theme` block by its inline name (built-in or user-declared),
/// falling back to the built-in `forge` when the name doesn't resolve.
pub(crate) fn find_theme<'a>(doc: &'a Document, name: &str) -> Option<Block<'a>> {
    find_theme_exact(doc, name).or_else(|| find_theme_exact(doc, DEFAULT_THEME))
}

/// The inheritance chain a theme name resolves to, nearest first: the named
/// theme, then whatever its `extends` symbol names, and so on. Every lookup
/// walks this list and takes the first link that states the thing, so a
/// derived theme overrides role by role rather than wholesale.
///
/// The head resolves through [`find_theme`] (so an unknown site theme still
/// lands on `forge`), but a link does not: an `extends` naming no theme ends
/// the chain rather than silently splicing `forge` into the middle of it. A
/// name already in the chain ends it too, so a cycle terminates.
pub(crate) fn theme_chain<'a>(doc: &'a Document, name: &str) -> Vec<Block<'a>> {
    let Some(head) = find_theme(doc, name) else {
        return Vec::new();
    };
    let mut seen: Vec<String> = vec![label_string(&head).unwrap_or_default()];
    let mut chain = vec![head];
    while chain.len() < MAX_THEME_DEPTH {
        let Some(parent) = chain.last().and_then(|b| field_symbol(b, "extends")) else {
            break;
        };
        if seen.contains(&parent) {
            break;
        }
        let Some(block) = find_theme_exact(doc, &parent) else {
            break;
        };
        seen.push(parent);
        chain.push(block);
    }
    chain
}

/// The first value a theme chain gives for a direct `theme` field
/// (`font_head`, …), nearest link first.
pub(crate) fn chain_field(chain: &[Block<'_>], field: &str) -> Option<String> {
    chain.iter().find_map(|t| field_utf8(t, field))
}

/// The first value a theme chain gives for one `metrics` field, nearest
/// link first. A link with no `metrics` child, or one that omits the field,
/// defers to the next.
pub(crate) fn chain_metric(chain: &[Block<'_>], field: &str) -> Option<String> {
    chain.iter().find_map(|t| {
        t.blocks()
            .filter(|b| b.kind() == "metrics")
            .find_map(|m| field_utf8(&m, field))
    })
}

/// The first colour a theme chain gives for one palette role in one mode,
/// nearest link first. Resolution is per role, not per palette: a derived
/// theme that restates only `bg` inherits the other 30 roles.
pub(crate) fn chain_role(chain: &[Block<'_>], mode: &str, role: &str) -> Option<String> {
    chain.iter().find_map(|t| {
        t.blocks()
            .find(|b| b.kind() == "palette" && label_string(b).as_deref() == Some(mode))
            .and_then(|p| field_utf8(&p, role))
    })
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

/// A theme's eight-hue ring, resolved for one mode. The hue-driven systems
/// — chart series, callout accents, diagram shapes — read from this, so a
/// backend with no stylesheet paints the same colours the CSS would.
pub(crate) struct Hues {
    /// The eight ring roles, in `HUES` order.
    hues: [String; 8],
}

impl Hues {
    /// The colour for one ring role (`"red"`, `"blue"`, …), or `None` when
    /// the name is not a hue.
    pub(crate) fn get(&self, name: &str) -> Option<&str> {
        let i = HUES.iter().position(|h| *h == name)?;
        Some(&self.hues[i])
    }
}

/// Resolve a named theme's hue ring for one mode, or `None` when the name
/// resolves to no `theme` block or the chain states no palette for the
/// mode. A role no link in the chain states is left out too, so the caller
/// keeps its own default rather than inheriting an unrelated theme's
/// colour.
pub(crate) fn resolve_hues(doc: &Document, theme: &str, mode: &str) -> Option<Hues> {
    let mode = if mode == "light" { "light" } else { "dark" };
    let chain = theme_chain(doc, theme);
    let mut hues: [String; 8] = Default::default();
    for (i, role) in HUES.iter().enumerate() {
        hues[i] = chain_role(&chain, mode, role)?;
    }
    Some(Hues { hues })
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
/// role colours: walk the theme's `extends` chain (fallback Forge) for the
/// matching-mode `palette` role, and fill any role no link states from the
/// Forge palette of that mode. Unknown accent ⇒ blue; unknown mode ⇒ dark.
pub(crate) fn resolve_roles(doc: &Document, theme: &str, accent: &str, mode: &str) -> ThemeRoles {
    let mode = if mode == "light" { "light" } else { "dark" };
    let accent_hue = if HUES.contains(&accent) {
        accent
    } else {
        "blue"
    };
    let def = default_roles(mode);
    let chain = theme_chain(doc, theme);
    if chain.is_empty() {
        return def;
    }
    let role = |f: &str, fallback: &str| {
        chain_role(&chain, mode, f).unwrap_or_else(|| fallback.to_string())
    };
    ThemeRoles {
        // The wireframe panel sits on the reading surface (`book_bg`), not
        // the darker outer gutter (`bg`); fall back to `bg` then Forge.
        bg: chain_role(&chain, mode, "book_bg")
            .or_else(|| chain_role(&chain, mode, "bg"))
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
