//! Terminal rendering for `@block("terminal")`.
//!
//! A `terminal` is a monospace character grid rendered as inline SVG,
//! drawn with the bundled JetBrains Mono Nerd Font so box-drawing
//! glyphs, powerline symbols, and Nerd Font icons render faithfully.
//!
//! Everything funnels through a single styled [`grid::Grid`]. There are
//! three ways to populate it:
//!
//! * **primitives** — `term_box` / `term_text` / `term_glyph` /
//!   `term_fill` child blocks placed at character coordinates, with the
//!   complete set of terminal text styles (bold, dim, italic, underline,
//!   strikethrough, blink, inverse, conceal);
//! * **inline text** — a `text = "..."` field fed to a virtual terminal
//!   (`avt`), so newlines/tabs lay out as a real terminal would (WCL
//!   string literals can't carry raw escape bytes, so this path is
//!   effectively plain text);
//! * **replay** — a `source = "rec.cast"` asciinema recording, parsed
//!   and replayed through `avt` into a sequence of frames that a small
//!   bundled JS player ([`crate::PLAYER_JS`]) steps through.
//!
//! One renderer turns a grid into SVG (`grid_to_runs` + `runs_to_svg`);
//! the replay path serialises every frame's runs to JSON next to the
//! SVG and lets the player rebuild the cell group per frame.
//!
//! The implementation is split into focused submodules: [`color`] (the
//! colour model + palette), [`grid`] (the styled cell store), [`svg`]
//! (resolving a grid to coloured runs + painting the static SVG),
//! [`widgets`] (populating from child blocks + the `lower` dispatch),
//! and [`replay`] (the `avt` virtual terminal + asciicast playback).

use std::path::Path;

use wcl_lang::{Block, Document};

use crate::render::{
    escape_html, field_bool, field_f64, field_i64, field_id, field_symbol, field_utf8,
    field_utf8_list, label_string, resolve_roles,
};

mod color;
mod grid;
mod replay;
mod svg;
mod widgets;

use color::*;
use grid::*;
use replay::*;
use svg::*;
use widgets::*;

/// Bundled replay player, written into `<out>/_wdoc/` and referenced by
/// pages that contain a replay terminal.
pub(crate) const PLAYER_JS: &str = include_str!("../../assets/terminal-player.js");

/// Embedded JetBrains Mono Nerd Font (Mono variant) faces. Written into
/// `<out>/_wdoc/` whenever a document uses a terminal so the page's
/// `@font-face` rules resolve. `(filename, bytes)`.
pub(crate) const FONT_FILES: &[(&str, &[u8])] = &[
    (
        "JetBrainsMonoNerdFontMono-Regular.woff2",
        include_bytes!("../../assets/fonts/JetBrainsMonoNerdFontMono-Regular.woff2"),
    ),
    (
        "JetBrainsMonoNerdFontMono-Bold.woff2",
        include_bytes!("../../assets/fonts/JetBrainsMonoNerdFontMono-Bold.woff2"),
    ),
    (
        "JetBrainsMonoNerdFontMono-Italic.woff2",
        include_bytes!("../../assets/fonts/JetBrainsMonoNerdFontMono-Italic.woff2"),
    ),
];

/// Subdirectory under the output root that holds the bundled terminal
/// assets (fonts + player).
pub(crate) const ASSET_DIR: &str = "_wdoc";

/// The bundled JetBrains Mono Nerd Font's family name. The self-contained
/// (PDF) terminal SVG names it explicitly on every cell `<text>` so usvg
/// shapes the grid against this *embedded* face — not a system font, and not
/// the other bundled monospace (NotoSansMono), whose block glyphs leave gaps
/// in the cell grid. The PDF embed fontdb registers the same name as its
/// monospace family (see `pdf::svg_embed`).
pub(crate) const NERD_FONT_FAMILY: &str = "JetBrainsMono Nerd Font Mono";

/// Default line height as a multiple of font size.
const DEFAULT_LINE_HEIGHT: f64 = 1.2;
/// Default font size in px when `font_size` is omitted.
const DEFAULT_FONT_PX: f64 = 14.0;

/// True when `block`'s subtree contains a `terminal` (so the build knows
/// to emit the font + player assets).
pub(crate) fn uses_terminal(block: &Block<'_>) -> bool {
    block.kind() == "terminal" || block.blocks().any(|b| uses_terminal(&b))
}

/// Render a `@block("terminal")` to an HTML fragment: a `wdoc-terminal`
/// `<div>` wrapping the inline-SVG grid, plus — for replay — the player
/// controls and the frames JSON the bundled player steps through.
pub(crate) fn render_terminal(
    doc: &Document,
    block: &Block<'_>,
    base_dir: Option<&Path>,
) -> String {
    let font_px = field_f64(block, "font_size").unwrap_or(DEFAULT_FONT_PX);
    let line_height = field_f64(block, "line_height").unwrap_or(DEFAULT_LINE_HEIGHT);
    let chrome = field_bool(block, "chrome").unwrap_or(true);
    let title = field_utf8(block, "title");
    let preset = field_symbol(block, "palette");
    let fg_field = field_utf8(block, "fg");
    let bg_field = field_utf8(block, "bg");
    let def_cols = field_i64(block, "cols").unwrap_or(80).max(1) as usize;
    let def_rows = field_i64(block, "rows").unwrap_or(24).max(1) as usize;

    // The terminal is themed by the WCL `class` system: its `class` list
    // reaches the wrapping `<div>`, so a `class { background color … }`
    // (with dark/light modes) sets the terminal's default bg + text.
    // Explicit `fg`/`bg`/`palette` fields override the class via inline
    // style. We also seed the palette's concrete defaults from the same
    // sources so styled cells (inverse/dim) match the visible theme.
    let user_classes = field_utf8_list(block, "class");
    let pal_fg = fg_field
        .clone()
        .or_else(|| class_color(doc, &user_classes, "color"));
    let pal_bg = bg_field
        .clone()
        .or_else(|| class_color(doc, &user_classes, "background"));
    // The `:light` preset follows the site theme's light palette when the doc
    // is themed; an unthemed doc keeps the concrete white/dark default.
    let light = light_preset_colors(doc);
    let light_ref = light.as_ref().map(|(f, b)| (f.as_str(), b.as_str()));
    let pal = Palette::new(
        preset.as_deref(),
        pal_fg.as_deref(),
        pal_bg.as_deref(),
        light_ref,
    );

    let mut classes = vec!["wdoc-terminal".to_string()];
    classes.extend(user_classes);
    let class_attr = classes.join(" ");
    let style_attr = div_style(
        preset.as_deref(),
        fg_field.as_deref(),
        bg_field.as_deref(),
        light_ref,
    );
    let id_attr = field_id(block, "id")
        .map(|id| format!(" id=\"{}\"", escape_html(&id)))
        .unwrap_or_default();

    // Replay mode: a `source` .cast file wins over everything else.
    if let Some(src_rel) = field_utf8(block, "source") {
        return render_replay(
            block,
            base_dir,
            &src_rel,
            def_cols,
            def_rows,
            font_px,
            line_height,
            chrome,
            title.as_deref(),
            &pal,
            &class_attr,
            &style_attr,
            &id_attr,
        );
    }

    // Static mode: inline text fed to a VT, else authored primitives.
    let grid = match field_utf8(block, "text") {
        Some(text) if !text.is_empty() => populate_inline(def_cols, def_rows, &text),
        _ => {
            let mut grid = Grid::new(def_cols, def_rows);
            populate_primitives(&mut grid, doc, block);
            grid
        }
    };
    let g = Geom::new(def_cols, def_rows, font_px, line_height, chrome);
    let svg = grid_svg(&grid, &pal, &g, title.as_deref(), None, false, false);
    format!("<div class=\"{class_attr}\"{style_attr}{id_attr}>{svg}</div>")
}

/// Render a `@block("terminal")` to a bare, self-contained `<svg>` for
/// the PDF backend: no wrapping `<div>` and no reliance on injected CSS.
/// The window background, default text colour, and chrome are baked from
/// the resolved palette (see [`svg::grid_svg`]'s `self_contained` path). A
/// replay (`source`) terminal is captured as a single static snapshot of
/// its last frame — a print has no player.
pub(crate) fn render_terminal_pdf(
    doc: &Document,
    block: &Block<'_>,
    base_dir: Option<&Path>,
) -> String {
    let font_px = field_f64(block, "font_size").unwrap_or(DEFAULT_FONT_PX);
    let line_height = field_f64(block, "line_height").unwrap_or(DEFAULT_LINE_HEIGHT);
    let chrome = field_bool(block, "chrome").unwrap_or(true);
    let title = field_utf8(block, "title");
    let preset = field_symbol(block, "palette");
    let fg_field = field_utf8(block, "fg");
    let bg_field = field_utf8(block, "bg");
    let def_cols = field_i64(block, "cols").unwrap_or(80).max(1) as usize;
    let def_rows = field_i64(block, "rows").unwrap_or(24).max(1) as usize;

    // Same palette resolution as the HTML path: explicit fg/bg/preset, else
    // the terminal's referenced `class` colours, else the dark default.
    let user_classes = field_utf8_list(block, "class");
    let pal_fg = fg_field
        .clone()
        .or_else(|| class_color(doc, &user_classes, "color"));
    let pal_bg = bg_field
        .clone()
        .or_else(|| class_color(doc, &user_classes, "background"));
    let light = light_preset_colors(doc);
    let light_ref = light.as_ref().map(|(f, b)| (f.as_str(), b.as_str()));
    let pal = Palette::new(
        preset.as_deref(),
        pal_fg.as_deref(),
        pal_bg.as_deref(),
        light_ref,
    );

    // Replay: snapshot the last coalesced frame as a static grid.
    if let Some(src_rel) = field_utf8(block, "source") {
        let path = match base_dir {
            Some(dir) => dir.join(&src_rel),
            None => Path::new(&src_rel).to_path_buf(),
        };
        if let Ok(src) = std::fs::read_to_string(&path) {
            let cast = parse_cast(&src, def_cols, def_rows);
            let g = Geom::new(cast.cols, cast.rows, font_px, line_height, chrome);
            let last = cast
                .frames
                .last()
                .map(|f| &f.grid)
                .expect("parse_cast always yields at least one frame");
            return grid_svg(last, &pal, &g, title.as_deref(), None, false, true);
        }
        // Unreadable cast: fall through to an empty static grid below.
    }

    let grid = match field_utf8(block, "text") {
        Some(text) if !text.is_empty() => populate_inline(def_cols, def_rows, &text),
        _ => {
            let mut grid = Grid::new(def_cols, def_rows);
            populate_primitives(&mut grid, doc, block);
            grid
        }
    };
    let g = Geom::new(def_cols, def_rows, font_px, line_height, chrome);
    grid_svg(&grid, &pal, &g, title.as_deref(), None, false, true)
}

/// First referenced `class` that sets `field` (`color` / `background`),
/// returned verbatim. Used to seed the palette's concrete defaults so
/// inverse/dim cells match a class-themed terminal.
fn class_color(doc: &Document, classes: &[String], field: &str) -> Option<String> {
    for name in classes {
        if let Some(b) = doc
            .blocks()
            .find(|b| b.kind() == "class" && label_string(b).as_deref() == Some(name))
            && let Some(v) = field_utf8(&b, field)
        {
            return Some(v);
        }
    }
    None
}

/// The site theme's light-mode `(fg, bg)` for the `:light` terminal preset,
/// or `None` when the doc has no `site` (then the concrete white/dark
/// default applies). The default (dark) terminal is unaffected — only the
/// explicit `palette = :light` opt-in follows the theme.
fn light_preset_colors(doc: &Document) -> Option<(String, String)> {
    let site = doc.blocks().find(|b| b.kind() == "site")?;
    let theme = field_symbol(&site, "theme").unwrap_or_else(|| "nord".to_string());
    let accent = field_symbol(&site, "accent").unwrap_or_else(|| "blue".to_string());
    let roles = resolve_roles(doc, &theme, &accent, "light");
    Some((roles.fg, roles.bg))
}

/// Inline `style` for the terminal `<div>` from explicit `fg`/`bg`
/// fields and the `:light` preset (these override any `class`). Empty
/// when the terminal relies purely on classes / the default theme.
/// `light` is the site theme's light-mode `(fg, bg)` for the `:light`
/// preset, or `None` for the concrete white/dark default.
fn div_style(
    preset: Option<&str>,
    fg: Option<&str>,
    bg: Option<&str>,
    light: Option<(&str, &str)>,
) -> String {
    let mut s = String::new();
    let (light_fg, light_bg) = light.unwrap_or(("#1c1c1c", "#ffffff"));
    let bg = bg
        .map(str::to_string)
        .or_else(|| (preset == Some("light")).then(|| light_bg.to_string()));
    let fg = fg
        .map(str::to_string)
        .or_else(|| (preset == Some("light")).then(|| light_fg.to_string()));
    if let Some(c) = bg {
        s.push_str(&format!("background:{};", escape_html(&c)));
    }
    if let Some(c) = fg {
        s.push_str(&format!("color:{};", escape_html(&c)));
    }
    if s.is_empty() {
        String::new()
    } else {
        format!(" style=\"{s}\"")
    }
}
