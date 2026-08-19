//! Icon support: bundled SVG packs, the `:name:` inline handler, and
//! the diagram `icon` block.
//!
//! The packs (Lucide, Bootstrap Icons) are compiled into the binary by
//! `build.rs`, which emits the `ICON_PACKS` manifest `include!`d below.
//! At build time `IconRegistry::load` reads every `iconset` block to
//! learn which pack each set draws from and its default styling.
//!
//! Resolved icons are referenced from a shared sprite rather than
//! inlined: each occurrence emits a tiny `<use href="_wdoc/icons.svg#id">`
//! and the build writes one `_wdoc/icons.svg` holding a `<symbol>` per
//! icon actually used. `currentColor` propagates through `<use>`, so the
//! `class` system's `color` recolours icons exactly like text.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;
use std::sync::LazyLock;

use regex::Regex;
use wcl_lang::Document;

use crate::render::{escape_html, field_utf8, field_utf8_list, label_string};

/// A bundled icon pack: its `name`, the concatenated SVG `blob`, and a
/// name-sorted `(stem, offset, len)` index into it. Populated by the
/// generated manifest.
pub(crate) struct IconPack {
    /// Pack name, as an author writes it.
    name: &'static str,
    /// The packed SVG sources, concatenated.
    blob: &'static [u8],
    /// Name-sorted `(stem, offset, len)` index into `blob`, so a lookup
    /// is a binary search plus a slice.
    index: &'static [(&'static str, u32, u32)],
}

include!(concat!(env!("OUT_DIR"), "/icon_manifest.rs"));

/// The `_wdoc/` subdirectory + filename the sprite is written to and
/// referenced by. Relative so it resolves both under the dev server and
/// on a deployed static host (mirrors the terminal asset convention).
pub(crate) const SPRITE_HREF: &str = "_wdoc/icons.svg";
/// Filename of the per-site sprite sheet every used icon lands in.
pub(crate) const SPRITE_FILE: &str = "icons.svg";

/// Look up an icon's raw SVG source from a bundled pack by name.
fn pack_lookup(pack: &str, name: &str) -> Option<&'static str> {
    let p = ICON_PACKS.iter().find(|p| p.name == pack)?;
    let idx = p.index.binary_search_by(|&(n, _, _)| n.cmp(name)).ok()?;
    let (_, offset, len) = p.index[idx];
    let bytes = &p.blob[offset as usize..(offset + len) as usize];
    std::str::from_utf8(bytes).ok()
}

/// Default styling for one icon set, plus any per-icon overrides. A
/// per-icon `icon_def` field, when present, wins over the set default;
/// a block-level field on an `icon` placement / the inline span wins
/// over both.
#[derive(Default, Clone)]
struct IconStyle {
    /// Rendered size, as a CSS length.
    size: Option<String>,
    /// Stroke / text colour.
    color: Option<String>,
    /// Fill colour.
    fill: Option<String>,
    /// Background behind the glyph.
    background: Option<String>,
    /// Extra classes to put on the element.
    classes: Vec<String>,
}

impl IconStyle {
    /// Layer `over` on top of `self`, returning the merged style. Scalar
    /// fields take `over`'s value when set; class lists concatenate.
    fn merged(&self, over: &IconStyle) -> IconStyle {
        IconStyle {
            size: over.size.clone().or_else(|| self.size.clone()),
            color: over.color.clone().or_else(|| self.color.clone()),
            fill: over.fill.clone().or_else(|| self.fill.clone()),
            background: over.background.clone().or_else(|| self.background.clone()),
            classes: self
                .classes
                .iter()
                .chain(over.classes.iter())
                .cloned()
                .collect(),
        }
    }
}

/// One `icon_set` declaration: which pack it draws from, its default
/// styling, and any per-icon overrides.
struct SetConfig {
    /// The set name authors reference.
    name: String,
    /// Bundled pack the set draws from.
    pack: String,
    /// Styling applied to every icon of the set.
    defaults: IconStyle,
    /// Per-icon overrides, which beat the set defaults.
    per_icon: HashMap<String, IconStyle>,
}

/// Every icon set a document declares, plus the record of which icons
/// were actually used — the sprite ships only those.
pub(crate) struct IconRegistry {
    /// The declared sets, in source order.
    sets: Vec<SetConfig>,
    /// (pack, name) pairs resolved during rendering. Sorted + deduped so
    /// the emitted sprite is stable and holds each icon once.
    used: RefCell<BTreeSet<(String, String)>>,
}

impl IconRegistry {
    /// Enumerate every `@block("iconset")` at the document root, reading
    /// its pack + default styling + per-icon `icon_def` overrides.
    pub(crate) fn load(doc: &Document) -> Self {
        let mut sets = Vec::new();
        for block in doc.blocks() {
            if block.kind() != "iconset" {
                continue;
            }
            let Some(name) = label_string(&block) else {
                continue;
            };
            // `pack` defaults to the set's own name (so `iconset lucide {}`
            // needs no `pack =`).
            let pack = field_utf8(&block, "pack").unwrap_or_else(|| name.clone());
            let defaults = style_from(&block);
            let mut per_icon = HashMap::new();
            for def in block.blocks().filter(|b| b.kind() == "icon_def") {
                if let Some(icon_name) = label_string(&def) {
                    per_icon.insert(icon_name, style_from(&def));
                }
            }
            sets.push(SetConfig {
                name,
                pack,
                defaults,
                per_icon,
            });
        }
        IconRegistry {
            sets,
            used: RefCell::new(BTreeSet::new()),
        }
    }

    /// Resolve a raw `:name:` token (optionally `set.name`) to inline
    /// HTML, or `None` when no declared set provides the icon (the
    /// caller then emits the literal text). `extra` carries any classes
    /// from the inline span.
    pub(crate) fn resolve_inline(&self, raw: &str, extra: &[String]) -> Option<String> {
        let (set_hint, icon) = split_set(raw);
        let (set, pack) = self.find(set_hint, icon)?;
        self.record(&pack, icon);
        let ov = IconStyle {
            classes: extra.to_vec(),
            ..IconStyle::default()
        };
        let style = self.style_for(set, icon, &ov);
        Some(inline_markup(&pack, icon, &style))
    }

    /// A complete standalone `<svg>` for an inline icon — the raw pack glyph
    /// (which paints with `currentColor`) rather than a `<use>` into the shared
    /// sprite. The PDF backend needs this because the sprite isn't available
    /// when embedding SVG. Returns `None` when no declared iconset provides the
    /// name.
    pub(crate) fn standalone(&self, raw: &str) -> Option<String> {
        let (set_hint, icon) = split_set(raw);
        let (_set, pack) = self.find(set_hint, icon)?;
        pack_lookup(&pack, icon).map(str::to_string)
    }

    /// Resolve an `Html::Icon` (e.g. a `callout`'s built-in
    /// default or an explicit override). Unlike `resolve_inline`, this
    /// also falls back to a `pack.name` token against a compiled-in pack
    /// when no matching `iconset` is declared — so the built-in callout
    /// glyphs render with zero icon configuration. The fallback carries
    /// no inline colour, so the icon inherits its container's colour via
    /// `currentColor`.
    pub(crate) fn resolve_html_icon(&self, raw: &str, extra: &[String]) -> Option<String> {
        if let Some(svg) = self.resolve_inline(raw, extra) {
            return Some(svg);
        }
        let (hint, icon) = split_set(raw);
        let pack = hint?;
        if !ICON_PACKS.iter().any(|p| p.name == pack) {
            return None;
        }
        self.resolve_builtin(pack, icon, extra)
    }

    /// Resolve `name` directly from a compiled-in `pack`, bypassing the
    /// declared-iconset lookup. Records usage so the icon lands in the
    /// shared sprite.
    fn resolve_builtin(&self, pack: &str, name: &str, extra: &[String]) -> Option<String> {
        pack_lookup(pack, name)?;
        self.record(pack, name);
        let style = IconStyle {
            classes: extra.to_vec(),
            ..IconStyle::default()
        };
        Some(inline_markup(pack, name, &style))
    }

    /// Resolve a diagram `icon` placement to SVG (a `<use>`, optionally
    /// behind a background `<rect>`), or `None` on a miss.
    pub(crate) fn resolve_shape(
        &self,
        raw: &str,
        set_field: Option<&str>,
        geom: (f64, f64, f64, f64),
        over: &ShapeOverride,
    ) -> Option<String> {
        // An explicit `set =` field wins; otherwise a `set.name` prefix
        // on the name is honored.
        let (prefix_set, icon) = split_set(raw);
        let hint = set_field.or(prefix_set);
        let (set, pack) = self.find(hint, icon)?;
        self.record(&pack, icon);
        let ov = IconStyle {
            size: None,
            color: over.color.clone(),
            fill: over.fill.clone(),
            background: over.background.clone(),
            classes: over.classes.clone(),
        };
        let style = self.style_for(set, icon, &ov);
        Some(shape_markup(&pack, icon, geom, &style))
    }

    /// The `<symbol>` definitions for every recorded icon (no wrapping `<svg>`),
    /// or `None` when none were used. Shared by [`build_sprite`](Self::build_sprite)
    /// (the web sprite) and the PDF path (spliced into an embedded SVG's
    /// `<defs>` so its `<use href="…#id">` resolves locally).
    pub(crate) fn symbol_defs(&self) -> Option<String> {
        let used = self.used.borrow();
        if used.is_empty() {
            return None;
        }
        let mut symbols = String::new();
        for (pack, name) in used.iter() {
            let Some(raw) = pack_lookup(pack, name) else {
                continue;
            };
            if let Some(sym) = normalize_to_symbol(raw, &format!("{pack}-{name}")) {
                symbols.push_str(&sym);
            }
        }
        if symbols.is_empty() {
            None
        } else {
            Some(symbols)
        }
    }

    /// The sprite `<symbol>` ids (`{pack}-{name}`) for every icon recorded
    /// during this render. The dev server's incremental path compares these
    /// against the on-disk sprite to decide whether a targeted page render
    /// introduced an icon the prior full build's sprite lacks (⇒ fall back
    /// to a full rebuild rather than overwrite the shared sprite).
    pub(crate) fn used_ids(&self) -> Vec<String> {
        self.used
            .borrow()
            .iter()
            .map(|(pack, name)| format!("{pack}-{name}"))
            .collect()
    }

    /// Build the shared sprite from every recorded icon, or `None` when
    /// none were used. Called once after all pages render.
    pub(crate) fn build_sprite(&self) -> Option<String> {
        let symbols = self.symbol_defs()?;
        Some(format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" style=\"display:none\">{symbols}</svg>"
        ))
    }

    /// Find the set + pack that provides `icon`. With a `hint`, only that
    /// set is considered; without one, sets are tried in declaration
    /// order and the first whose pack has the icon wins.
    fn find(&self, hint: Option<&str>, icon: &str) -> Option<(&SetConfig, String)> {
        match hint {
            Some(h) => {
                let set = self.sets.iter().find(|s| s.name == h)?;
                pack_lookup(&set.pack, icon).map(|_| (set, set.pack.clone()))
            }
            None => self
                .sets
                .iter()
                .find(|s| pack_lookup(&s.pack, icon).is_some())
                .map(|s| (s, s.pack.clone())),
        }
    }

    /// Note that an icon was used, so the sprite includes it.
    fn record(&self, pack: &str, icon: &str) {
        self.used
            .borrow_mut()
            .insert((pack.to_string(), icon.to_string()));
    }

    /// Merge a set's default style, the per-icon `icon_def` override, and
    /// a caller-supplied override (block / inline span) in that order.
    fn style_for(&self, set: &SetConfig, icon: &str, over: &IconStyle) -> IconStyle {
        let mut merged = set.defaults.clone();
        if let Some(per) = set.per_icon.get(icon) {
            merged = merged.merged(per);
        }
        merged.merged(over)
    }
}

/// Caller-supplied per-placement style for a diagram `icon`.
#[derive(Default)]
pub(crate) struct ShapeOverride {
    /// Stroke / text colour override.
    pub color: Option<String>,
    /// Fill override.
    pub fill: Option<String>,
    /// Background override.
    pub background: Option<String>,
    /// Extra classes for this placement.
    pub classes: Vec<String>,
}

/// Read the shared styling fields off an `iconset` / `icon_def` block.
fn style_from(block: &wcl_lang::Block<'_>) -> IconStyle {
    IconStyle {
        size: field_utf8(block, "size"),
        color: field_utf8(block, "color"),
        fill: field_utf8(block, "fill"),
        background: field_utf8(block, "background"),
        classes: field_utf8_list(block, "class"),
    }
}

/// Split a `set.name` token into its parts. Icon names never contain a
/// dot (pack file stems are `[a-z0-9-]`), so a leading `set.` is
/// unambiguous; everything else is the bare icon name.
fn split_set(raw: &str) -> (Option<&str>, &str) {
    match raw.split_once('.') {
        Some((set, name)) if !set.is_empty() && !name.is_empty() => (Some(set), name),
        _ => (None, raw),
    }
}

/// CSS `style` attribute for inline icons (size + colour + fill +
/// background). Returns `""` when nothing is set.
fn style_attr(style: &IconStyle, include_size: bool) -> String {
    let mut css = String::new();
    if include_size && let Some(s) = &style.size {
        let _ = write!(css, "width:{s};height:{s};");
    }
    if let Some(c) = &style.color {
        let _ = write!(css, "color:{c};");
    }
    if let Some(f) = &style.fill {
        let _ = write!(css, "fill:{f};");
    }
    if let Some(b) = &style.background {
        let _ = write!(css, "background:{b};");
    }
    if css.is_empty() {
        String::new()
    } else {
        format!(" style=\"{}\"", escape_html(&css))
    }
}

/// Build the `class="…"` attribute for an icon element.
fn class_attr(extra: &[String]) -> String {
    let mut classes = vec!["wdoc-icon".to_string()];
    classes.extend(extra.iter().cloned());
    let joined = classes
        .iter()
        .map(|s| escape_html(s))
        .collect::<Vec<_>>()
        .join(" ");
    format!(" class=\"{joined}\"")
}

/// Markup for an icon used inline in text — a `<use>` into the site
/// sprite.
fn inline_markup(pack: &str, icon: &str, style: &IconStyle) -> String {
    format!(
        "<svg{cls}{style}><use href=\"{SPRITE_HREF}#{pack}-{icon}\"/></svg>",
        cls = class_attr(&style.classes),
        style = style_attr(style, true),
    )
}

/// Markup for an icon placed as a diagram shape, positioned and
/// sized by `geom`.
fn shape_markup(pack: &str, icon: &str, geom: (f64, f64, f64, f64), style: &IconStyle) -> String {
    let (x, y, w, h) = geom;
    let use_el = format!(
        "<use href=\"{SPRITE_HREF}#{pack}-{icon}\" x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\"{cls}{style}/>",
        cls = class_attr(&style.classes),
        style = style_attr(style, false),
    );
    match &style.background {
        Some(bg) => format!(
            "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"{}\" />{use_el}",
            escape_html(bg)
        ),
        None => use_el,
    }
}

/// Matches a bundled icon's root `<svg>` tag, so its attributes can
/// be rewritten as the sprite symbol's.
static ROOT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<svg\b([^>]*)>").unwrap());
/// Matches one `name="value"` attribute, for rewriting the attributes
/// of an icon's root tag.
static ATTR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"([\w:-]+)\s*=\s*"([^"]*)""#).unwrap());

/// Rewrite a pack SVG's root `<svg …>` into a `<symbol id=… viewBox=…>`
/// for the sprite. Presentation attributes (fill / stroke / stroke-width
/// / …) are preserved so stroke-based packs still paint; `width` /
/// `height` / `xmlns` / `class` / `id` are dropped. Returns `None` when
/// the root tag or a usable `viewBox` can't be found (the icon is then
/// simply omitted — never a build failure).
fn normalize_to_symbol(raw: &str, id: &str) -> Option<String> {
    let caps = ROOT_RE.captures(raw)?;
    let whole = caps.get(0)?;
    let attr_blob = caps.get(1)?.as_str();
    let open_end = whole.end();
    let close_start = raw.rfind("</svg>")?;
    if close_start < open_end {
        return None;
    }
    let inner = &raw[open_end..close_start];

    let mut view_box: Option<String> = None;
    let mut width: Option<String> = None;
    let mut height: Option<String> = None;
    let mut kept: Vec<(&str, &str)> = Vec::new();
    for ac in ATTR_RE.captures_iter(attr_blob) {
        let name = ac.get(1).unwrap().as_str();
        let val = ac.get(2).unwrap().as_str();
        match name.to_ascii_lowercase().as_str() {
            "viewbox" => view_box = Some(val.to_string()),
            "width" => width = Some(val.to_string()),
            "height" => height = Some(val.to_string()),
            "id" | "class" | "xmlns" | "aria-hidden" | "role" | "focusable" => {}
            n if n.starts_with("xmlns:") => {}
            _ => kept.push((name, val)),
        }
    }
    let vb = view_box.or_else(|| match (width.as_deref(), height.as_deref()) {
        (Some(w), Some(h)) => Some(format!("0 0 {w} {h}")),
        _ => None,
    })?;

    let mut out = format!(
        "<symbol id=\"{}\" viewBox=\"{}\"",
        escape_html(id),
        escape_html(&vb)
    );
    for (name, val) in kept {
        let _ = write!(out, " {name}=\"{}\"", escape_html(val));
    }
    out.push('>');
    out.push_str(inner);
    out.push_str("</symbol>");
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LUCIDE: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" \
        viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\">\
        <path d=\"M20 6 9 17l-5-5\" /></svg>";
    const BOOTSTRAP: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" \
        fill=\"currentColor\" class=\"bi bi-house\" viewBox=\"0 0 16 16\">\
        <path d=\"M8 0 0 8h2v8h12V8h2z\"/></svg>";

    #[test]
    fn symbol_preserves_viewbox_and_stroke_drops_size() {
        let s = normalize_to_symbol(LUCIDE, "lucide-check").unwrap();
        assert!(s.starts_with("<symbol id=\"lucide-check\" viewBox=\"0 0 24 24\""));
        assert!(s.contains("stroke=\"currentColor\""));
        assert!(s.contains("fill=\"none\""));
        assert!(s.contains("<path d=\"M20 6 9 17l-5-5\" />"));
        assert!(s.ends_with("</symbol>"));
        // The presentation `stroke-width` is kept; the root sizing
        // `width` / `height` / `xmlns` are stripped from the symbol tag.
        let head = &s[..s.find('>').unwrap()];
        assert!(head.contains("stroke-width=\"2\""));
        assert!(!head.contains("width=\"24\""));
        assert!(!head.contains("height=\"24\""));
        assert!(!head.contains("xmlns"));
    }

    #[test]
    fn symbol_preserves_fill_for_bootstrap() {
        let s = normalize_to_symbol(BOOTSTRAP, "bootstrap-house").unwrap();
        assert!(s.contains("viewBox=\"0 0 16 16\""));
        assert!(s.contains("fill=\"currentColor\""));
        // bootstrap's class="bi bi-house" must not leak onto the symbol.
        assert!(!s.contains("bi-house"));
    }

    #[test]
    fn malformed_svg_returns_none() {
        assert!(normalize_to_symbol("not svg at all", "x").is_none());
        // no viewBox and no width/height ⇒ cannot synthesise.
        assert!(normalize_to_symbol("<svg fill=\"red\"></svg>", "x").is_none());
    }

    #[test]
    fn split_set_distinguishes_prefix() {
        assert_eq!(split_set("house"), (None, "house"));
        assert_eq!(split_set("lucide.house"), (Some("lucide"), "house"));
        assert_eq!(split_set("arrow-right"), (None, "arrow-right"));
    }
}
