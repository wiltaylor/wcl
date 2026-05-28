//! Terminal rendering for `@block("terminal")`.
//!
//! A `terminal` is a monospace character grid rendered as inline SVG,
//! drawn with the bundled JetBrains Mono Nerd Font so box-drawing
//! glyphs, powerline symbols, and Nerd Font icons render faithfully.
//!
//! Everything funnels through a single styled [`Grid`]. There are three
//! ways to populate it:
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

use std::path::Path;

use wcl_lang::{Block, Document, Value, VariantPayload};

use crate::render::{
    MAX_LOWER_DEPTH, block_to_record_raw, escape_html, field_bool, field_f64, field_i64, field_id,
    field_symbol, field_utf8, field_utf8_list, kind_for_variant, label_string, lookup_block_lower,
    map_bool, map_i64, map_utf8,
};

/// Bundled replay player, written into `<out>/_wdoc/` and referenced by
/// pages that contain a replay terminal.
pub(crate) const PLAYER_JS: &str = include_str!("../assets/terminal-player.js");

/// Embedded JetBrains Mono Nerd Font (Mono variant) faces. Written into
/// `<out>/_wdoc/` whenever a document uses a terminal so the page's
/// `@font-face` rules resolve. `(filename, bytes)`.
pub(crate) const FONT_FILES: &[(&str, &[u8])] = &[
    (
        "JetBrainsMonoNerdFontMono-Regular.woff2",
        include_bytes!("../assets/fonts/JetBrainsMonoNerdFontMono-Regular.woff2"),
    ),
    (
        "JetBrainsMonoNerdFontMono-Bold.woff2",
        include_bytes!("../assets/fonts/JetBrainsMonoNerdFontMono-Bold.woff2"),
    ),
    (
        "JetBrainsMonoNerdFontMono-Italic.woff2",
        include_bytes!("../assets/fonts/JetBrainsMonoNerdFontMono-Italic.woff2"),
    ),
];

/// Subdirectory under the output root that holds the bundled terminal
/// assets (fonts + player).
pub(crate) const ASSET_DIR: &str = "_wdoc";

/// True when `block`'s subtree contains a `terminal` (so the build knows
/// to emit the font + player assets).
pub(crate) fn uses_terminal(block: &Block<'_>) -> bool {
    block.kind() == "terminal" || block.blocks().any(|b| uses_terminal(&b))
}

// ── Geometry ────────────────────────────────────────────────────────

/// Cell advance as a fraction of font size. JetBrains Mono advances
/// 600/1000 em per glyph; the Nerd Font *Mono* variant forces every
/// glyph (icons included) to that single-cell width.
const CELL_W_RATIO: f64 = 0.6;
/// Default line height as a multiple of font size.
const DEFAULT_LINE_HEIGHT: f64 = 1.2;
/// Default font size in px when `font_size` is omitted.
const DEFAULT_FONT_PX: f64 = 14.0;
/// Baseline offset within a cell, as a fraction of the line box.
const BASELINE_RATIO: f64 = 0.78;
/// Minimum frame spacing for replay (seconds): events closer than this
/// are coalesced into one frame so a busy recording stays small.
const MIN_FRAME_DT: f64 = 1.0 / 30.0;

// Style bit flags shared by the Rust emitter and the JS player.
const F_BOLD: u8 = 1;
const F_ITALIC: u8 = 1 << 1;
const F_UNDERLINE: u8 = 1 << 2;
const F_STRIKE: u8 = 1 << 3;
const F_BLINK: u8 = 1 << 4;

// ── Colour ──────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum Color {
    /// Use the palette's default fg/bg.
    Default,
    /// 0..=255 indexed colour (0..16 themeable, then cube + greyscale).
    Indexed(u8),
    Rgb(u8, u8, u8),
}

/// A resolved 16-colour palette plus default fg/bg.
struct Palette {
    fg: (u8, u8, u8),
    bg: (u8, u8, u8),
    ansi: [(u8, u8, u8); 16],
}

/// The classic Tango 16-colour set — widely assumed by recordings and
/// legible on either a dark or a light background.
const TANGO: [(u8, u8, u8); 16] = [
    (0x00, 0x00, 0x00),
    (0xcc, 0x00, 0x00),
    (0x4e, 0x9a, 0x06),
    (0xc4, 0xa0, 0x00),
    (0x34, 0x65, 0xa4),
    (0x75, 0x50, 0x7b),
    (0x06, 0x98, 0x9a),
    (0xd3, 0xd7, 0xcf),
    (0x55, 0x57, 0x53),
    (0xef, 0x29, 0x29),
    (0x8a, 0xe2, 0x34),
    (0xfc, 0xe9, 0x4f),
    (0x72, 0x9f, 0xcf),
    (0xad, 0x7f, 0xa8),
    (0x34, 0xe2, 0xe2),
    (0xee, 0xee, 0xec),
];

impl Palette {
    fn new(preset: Option<&str>, fg: Option<&str>, bg: Option<&str>) -> Self {
        let (mut dfg, mut dbg) = match preset {
            Some("light") => ((0x1c, 0x1c, 0x1c), (0xff, 0xff, 0xff)),
            _ => ((0xd0, 0xd0, 0xd0), (0x1c, 0x1c, 0x1c)),
        };
        if let Some(c) = fg.and_then(parse_hex) {
            dfg = c;
        }
        if let Some(c) = bg.and_then(parse_hex) {
            dbg = c;
        }
        Palette {
            fg: dfg,
            bg: dbg,
            ansi: TANGO,
        }
    }

    /// Resolve an indexed colour to RGB across the full 256-colour space.
    fn indexed(&self, i: u8) -> (u8, u8, u8) {
        match i {
            0..=15 => self.ansi[i as usize],
            16..=231 => {
                let i = i - 16;
                let steps = [0u8, 95, 135, 175, 215, 255];
                (
                    steps[(i / 36) as usize],
                    steps[((i / 6) % 6) as usize],
                    steps[(i % 6) as usize],
                )
            }
            _ => {
                let v = 8u16 + 10 * (i as u16 - 232);
                let v = v.min(255) as u8;
                (v, v, v)
            }
        }
    }

    fn rgb_of(&self, c: Color) -> Option<(u8, u8, u8)> {
        match c {
            Color::Default => None,
            Color::Indexed(i) => Some(self.indexed(i)),
            Color::Rgb(r, g, b) => Some((r, g, b)),
        }
    }
}

fn hex(c: (u8, u8, u8)) -> String {
    format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
}

/// Render a foreground colour for SVG `fill`: a concrete colour as hex,
/// or the terminal's default as `currentColor` so a WCL `class`'s
/// `color` (and its dark/light modes) themes it.
fn ink(c: Option<(u8, u8, u8)>) -> String {
    match c {
        Some(c) => hex(c),
        None => "currentColor".to_string(),
    }
}

fn lerp(a: (u8, u8, u8), b: (u8, u8, u8), t: f64) -> (u8, u8, u8) {
    let mix = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * t).round() as u8;
    (mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2))
}

/// Parse a `#rgb` / `#rrggbb` hex colour.
fn parse_hex(s: &str) -> Option<(u8, u8, u8)> {
    let h = s.strip_prefix('#')?;
    match h.len() {
        3 => {
            let d = |i: usize| u8::from_str_radix(&h[i..=i], 16).ok().map(|v| v * 17);
            Some((d(0)?, d(1)?, d(2)?))
        }
        6 => {
            let d = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
            Some((d(0)?, d(2)?, d(4)?))
        }
        _ => None,
    }
}

/// Parse a colour field: `#rrggbb`, a 0..=255 index, or an ANSI name
/// (`red`, `bright_red`, `brightblue`, …). Unknown ⇒ `Default`.
fn parse_color(s: &str) -> Color {
    let s = s.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("default") {
        return Color::Default;
    }
    if let Some((r, g, b)) = parse_hex(s) {
        return Color::Rgb(r, g, b);
    }
    if let Ok(i) = s.parse::<u8>() {
        return Color::Indexed(i);
    }
    let key = s.to_ascii_lowercase();
    let key = key.replace([' ', '-'], "_");
    let (bright, base) = match key
        .strip_prefix("bright_")
        .or_else(|| key.strip_prefix("bright"))
    {
        Some(rest) => (true, rest),
        None => (false, key.as_str()),
    };
    let idx = match base {
        "black" => 0,
        "red" => 1,
        "green" => 2,
        "yellow" => 3,
        "blue" => 4,
        "magenta" | "purple" => 5,
        "cyan" => 6,
        "white" | "grey" | "gray" => 7,
        _ => return Color::Default,
    };
    Color::Indexed(if bright { idx + 8 } else { idx })
}

// ── Grid ────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Default)]
struct Style {
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    blink: bool,
    inverse: bool,
    conceal: bool,
}

#[derive(Clone, Copy)]
struct Cell {
    ch: char,
    fg: Color,
    bg: Color,
    style: Style,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            ch: ' ',
            fg: Color::Default,
            bg: Color::Default,
            style: Style::default(),
        }
    }
}

struct Grid {
    cols: usize,
    rows: usize,
    cells: Vec<Cell>,
    cursor: Option<(usize, usize)>,
}

impl Grid {
    fn new(cols: usize, rows: usize) -> Self {
        Grid {
            cols,
            rows,
            cells: vec![Cell::default(); cols * rows],
            cursor: None,
        }
    }

    fn set(&mut self, row: usize, col: usize, cell: Cell) {
        if row < self.rows && col < self.cols {
            self.cells[row * self.cols + col] = cell;
        }
    }

    fn row(&self, r: usize) -> &[Cell] {
        &self.cells[r * self.cols..(r + 1) * self.cols]
    }
}

// ── Resolved runs ───────────────────────────────────────────────────

/// A horizontal run of cells sharing fg / bg / flags. `fg`/`bg` are
/// `None` when the cell uses the terminal's *default* colour — fg
/// `None` renders as `currentColor` (so a WCL `class`'s `color` themes
/// it) and bg `None` renders nothing (the terminal `<div>`'s `class`
/// background shows through).
struct Run {
    col: usize,
    text: String,
    fg: Option<(u8, u8, u8)>,
    bg: Option<(u8, u8, u8)>,
    flags: u8,
}

/// A cell's resolved presentation: fg/bg (`None` ⇒ default, themed by
/// the class system via `currentColor` / the `<div>` background) and
/// the style flags the emitter / player need.
struct Paint {
    fg: Option<(u8, u8, u8)>,
    bg: Option<(u8, u8, u8)>,
    flags: u8,
}

/// Resolve a cell to its painted form, applying inverse / conceal /
/// dim. A *plain* default-coloured cell keeps `fg`/`bg` as `None` so
/// the class system themes it; only styled cells (inverse / conceal /
/// dim) need concrete defaults, which come from the palette.
fn resolve(cell: &Cell, pal: &Palette) -> Paint {
    let fg0 = pal.rgb_of(cell.fg); // None ⇒ default fg
    let bg0 = pal.rgb_of(cell.bg); // None ⇒ default bg

    // Bold maps to font-weight only; we deliberately don't brighten the
    // colour, so an authored `fg = "green" bold = true` stays green
    // (matching modern terminals, where bold-as-bright is opt-in).
    let mut fg = fg0;
    let mut bg = bg0;
    if cell.style.inverse {
        // Swap, materialising defaults so the swap is visible.
        fg = Some(bg0.unwrap_or(pal.bg));
        bg = Some(fg0.unwrap_or(pal.fg));
    }
    if cell.style.conceal {
        fg = Some(bg.unwrap_or(pal.bg));
    }
    if cell.style.dim {
        let base = bg.unwrap_or(pal.bg);
        fg = Some(lerp(fg.unwrap_or(pal.fg), base, 0.45));
    }

    let mut flags = 0u8;
    if cell.style.bold {
        flags |= F_BOLD;
    }
    if cell.style.italic {
        flags |= F_ITALIC;
    }
    if cell.style.underline {
        flags |= F_UNDERLINE;
    }
    if cell.style.strike {
        flags |= F_STRIKE;
    }
    if cell.style.blink {
        flags |= F_BLINK;
    }
    Paint { fg, bg, flags }
}

/// Group a grid into per-row runs of identical style.
fn grid_to_runs(grid: &Grid, pal: &Palette) -> Vec<Vec<Run>> {
    (0..grid.rows)
        .map(|r| {
            let mut runs: Vec<Run> = Vec::new();
            for (col, cell) in grid.row(r).iter().enumerate() {
                let Paint { fg, bg, flags } = resolve(cell, pal);
                match runs.last_mut() {
                    Some(run) if run.fg == fg && run.bg == bg && run.flags == flags => {
                        run.text.push(cell.ch);
                    }
                    _ => runs.push(Run {
                        col,
                        text: cell.ch.to_string(),
                        fg,
                        bg,
                        flags,
                    }),
                }
            }
            runs
        })
        .collect()
}

// ── Geometry resolved from a block ──────────────────────────────────

struct Geom {
    cols: usize,
    rows: usize,
    cw: f64,
    ch: f64,
    font_px: f64,
    left: f64,
    top: f64,
    chrome_h: f64,
    width: f64,
    height: f64,
}

impl Geom {
    fn new(cols: usize, rows: usize, font_px: f64, line_height: f64, chrome: bool) -> Self {
        // Snap the cell box to whole pixels so every cell origin is
        // integer-aligned — block and box-drawing glyphs then tile with
        // no hairline seams (the artifact that fractional positions cause).
        let cw = (font_px * CELL_W_RATIO).round();
        let ch = (font_px * line_height).round();
        let pad = (font_px * 0.45).round();
        let chrome_h = if chrome { (font_px * 1.7).round() } else { 0.0 };
        let inner_w = cols as f64 * cw;
        let inner_h = rows as f64 * ch;
        Geom {
            cols,
            rows,
            cw,
            ch,
            font_px,
            left: pad,
            top: pad + chrome_h,
            chrome_h,
            width: inner_w + 2.0 * pad,
            height: inner_h + 2.0 * pad + chrome_h,
        }
    }
}

// ── SVG emission ────────────────────────────────────────────────────

fn run_attrs(flags: u8) -> String {
    let mut out = String::new();
    if flags & F_BOLD != 0 {
        out.push_str(" font-weight=\"bold\"");
    }
    if flags & F_ITALIC != 0 {
        out.push_str(" font-style=\"italic\"");
    }
    let deco = match (flags & F_UNDERLINE != 0, flags & F_STRIKE != 0) {
        (true, true) => Some("underline line-through"),
        (true, false) => Some("underline"),
        (false, true) => Some("line-through"),
        (false, false) => None,
    };
    if let Some(d) = deco {
        out.push_str(&format!(" text-decoration=\"{d}\""));
    }
    if flags & F_BLINK != 0 {
        out.push_str(" class=\"term-blink\"");
    }
    out
}

/// Render the cell-area contents for one grid as a true character grid:
/// a background `<rect>` per contiguous coloured run, then **one
/// `<text>` glyph per occupied cell**, each centred in its cell. With
/// integer cell metrics this makes block/box-drawing glyphs tile
/// seamlessly — no vector-shape special-casing. Coordinates are
/// relative to the cell-area group origin.
fn runs_to_svg(rows: &[Vec<Run>], g: &Geom) -> String {
    let mut bgs = String::new();
    let mut fgs = String::new();
    for (r, runs) in rows.iter().enumerate() {
        let y_rect = r as f64 * g.ch;
        // Glyphs are centred vertically and horizontally in their cell.
        let y_text = r as f64 * g.ch + g.ch * BASELINE_RATIO;
        for run in runs {
            // One background rect spans the whole coloured run.
            if let Some(bg) = run.bg {
                let w = run.text.chars().count() as f64 * g.cw;
                bgs.push_str(&format!(
                    "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" fill=\"{}\" shape-rendering=\"crispEdges\"/>",
                    run.col as f64 * g.cw,
                    y_rect,
                    w,
                    g.ch,
                    hex(bg)
                ));
            }
            let has_deco = run.flags & (F_UNDERLINE | F_STRIKE) != 0;
            let attrs = run_attrs(run.flags);
            let fill = ink(run.fg);
            // Emit one centred glyph per cell. Spaces paint nothing, so
            // skip them unless the run is underlined/struck (then the
            // decoration must still span the blank cells).
            for (i, ch) in run.text.chars().enumerate() {
                if ch == ' ' && !has_deco {
                    continue;
                }
                let cx = (run.col + i) as f64 * g.cw + g.cw / 2.0;
                fgs.push_str(&format!(
                    "<text x=\"{cx:.2}\" y=\"{y_text:.2}\" text-anchor=\"middle\" xml:space=\"preserve\" fill=\"{fill}\"{attrs}>{}</text>",
                    escape_html(&ch.to_string())
                ));
            }
        }
    }
    format!("{bgs}{fgs}")
}

fn cursor_svg(grid: &Grid, g: &Geom) -> String {
    match grid.cursor {
        Some((col, row)) => format!(
            "<rect class=\"term-cursor\" x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\"/>",
            col as f64 * g.cw,
            row as f64 * g.ch,
            g.cw,
            g.ch
        ),
        None => String::new(),
    }
}

/// The static SVG for a single grid (window chrome + one painted frame).
/// When `cell_group_id` is set the cell-area `<g>` carries that id (so
/// the JS player can replace its children); otherwise it's anonymous.
fn grid_svg(
    grid: &Grid,
    pal: &Palette,
    g: &Geom,
    title: Option<&str>,
    cell_group_id: Option<&str>,
    replay: bool,
) -> String {
    let rows = grid_to_runs(grid, pal);
    let cells = runs_to_svg(&rows, g);
    let cursor = cursor_svg(grid, g);
    let id_attr = cell_group_id
        .map(|id| format!(" id=\"{}\"", escape_html(id)))
        .unwrap_or_default();
    // No opaque window rect: the terminal background is the wrapping
    // `<div class="wdoc-terminal …">`'s CSS background, so a WCL `class`
    // (and its dark/light modes) themes it. The SVG paints only chrome,
    // coloured cells, and glyphs over that background.
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" class=\"wdoc-terminal-svg\" \
         width=\"{w:.0}\" height=\"{h:.0}\" viewBox=\"0 0 {w:.0} {h:.0}\">\
         {chrome}\
         <g class=\"term-cells\" transform=\"translate({left:.2} {top:.2})\"{id_attr}>{cells}{cursor}</g>\
         </svg>",
        w = g.width,
        h = g.height,
        chrome = chrome_svg(g, title, replay),
        left = g.left,
        top = g.top,
    )
}

/// Window title bar: a close `✕` on the right, a centred title, and —
/// for a replay terminal — a play/pause/replay glyph to the left of the
/// `✕` (the JS player swaps its glyph and wires the click). Drawn only
/// when `chrome_h > 0`. Strokes/fills inherit the terminal text colour
/// (`currentColor`) via `TERMINAL_CSS`, so they follow the `class` theme.
fn chrome_svg(g: &Geom, title: Option<&str>, replay: bool) -> String {
    if g.chrome_h <= 0.0 {
        return String::new();
    }
    let cy = g.chrome_h / 2.0;
    // Close glyph: two crossing strokes near the right edge, inset from
    // the right by roughly the cell-area padding.
    let s = (g.font_px * 0.24).max(3.0);
    let cx = g.width - g.left - s;
    let close = format!(
        "<g class=\"term-close\"><line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\"/>\
         <line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\"/></g>",
        cx - s,
        cy - s,
        cx + s,
        cy + s,
        cx - s,
        cy + s,
        cx + s,
        cy - s,
    );
    // Replay control: a clickable ▶ glyph left of the ✕. The trailing
    // U+FE0E (text variation selector) forces a monochrome glyph so it
    // follows `currentColor` instead of rendering as a colour emoji.
    let play = if replay {
        format!(
            "<text class=\"term-chrome-btn\" x=\"{:.2}\" y=\"{:.2}\" text-anchor=\"middle\" aria-label=\"Play\">\u{25B6}\u{FE0E}</text>",
            cx - s - g.font_px,
            cy + g.font_px * 0.34,
        )
    } else {
        String::new()
    };
    let title_svg = match title {
        Some(t) if !t.is_empty() => format!(
            "<text class=\"term-title\" x=\"{:.2}\" y=\"{:.2}\" text-anchor=\"middle\">{}</text>",
            g.width / 2.0,
            cy + g.font_px * 0.32,
            escape_html(t)
        ),
        _ => String::new(),
    };
    format!(
        "<g class=\"term-chrome\"><rect x=\"0\" y=\"0\" width=\"{:.0}\" height=\"{:.2}\" class=\"term-chrome-bar\"/>{title_svg}{play}{close}</g>",
        g.width, g.chrome_h,
    )
}

// ── Populator A: the `term_text` primitive + element lowering ────────

fn pcolor(block: &Block<'_>, name: &str) -> Color {
    field_utf8(block, name)
        .map(|s| parse_color(&s))
        .unwrap_or(Color::Default)
}

/// Walk a terminal's children, drawing each into the grid. The one base
/// primitive (`term_text`) draws directly; every other child kind is a
/// higher-level element — its `lower` function decomposes it into
/// `TermFundamental::Text` runs (boxes, fills, glyphs, and the `tui_*`
/// controls are all just text), which we recursively draw (the cell-grid
/// analogue of the SVG/HTML `lower` dispatch). The root content origin is
/// `(0, 0)`, so top-level text is unaffected.
fn populate_primitives(grid: &mut Grid, doc: &Document, block: &Block<'_>) {
    for child in block.blocks() {
        place_child(grid, doc, &child, child.kind(), 0, (0, 0));
    }
}

/// Place one terminal child — the `term_text` primitive drawn at
/// `base + its position`, or an element lowered into text runs. `base` is
/// the parent's content origin (0-based cell offset); it accumulates as
/// containers nest.
fn place_child(
    grid: &mut Grid,
    doc: &Document,
    block: &Block<'_>,
    kind: &str,
    depth: usize,
    base: (usize, usize),
) {
    match kind {
        "term_text" => prim_text(grid, block, base),
        _ => populate_lowered(grid, doc, block, kind, depth, base),
    }
}

/// Lower a widget block and draw the primitives it returns. The widget's
/// own origin (`base + its row/col`) becomes the origin for the variants
/// its `lower` emits, which use local coordinates from `(1, 1)`.
fn populate_lowered(
    grid: &mut Grid,
    doc: &Document,
    block: &Block<'_>,
    kind: &str,
    depth: usize,
    base: (usize, usize),
) {
    if depth > MAX_LOWER_DEPTH {
        return;
    }
    let Some(arg) = block_to_record_raw(doc, block, kind) else {
        return;
    };
    let Some(fv) = lookup_block_lower(doc, block, kind) else {
        return;
    };
    let Ok(Value::List(items)) = doc.call_value(&fv, &[arg]) else {
        return;
    };
    let wbase = offset(base, cell_pos(block));
    for item in &items {
        draw_variant(grid, doc, block, item, depth, wbase);
    }
}

/// Draw one `TermFundamental` variant. `wbase` is the emitting widget's
/// origin; a `Text` run draws at `wbase + (its pos − 1)`, and a `Children`
/// slot recurses into the widget's child blocks at the slot's (local)
/// origin. There are no other fundamentals — every higher-level element
/// decomposes to `Text` (nesting is via `Children` + block recursion).
fn draw_variant(
    grid: &mut Grid,
    doc: &Document,
    block: &Block<'_>,
    value: &Value,
    depth: usize,
    wbase: (usize, usize),
) {
    let Value::Variant {
        variant, payload, ..
    } = value
    else {
        return;
    };
    let VariantPayload::Record(map) = payload else {
        return;
    };
    match kind_for_variant(variant).as_str() {
        "text" => draw_text_variant(grid, map, wbase),
        "children" => {
            let cbase = offset(wbase, map_pos(map));
            for child in block.blocks() {
                place_child(grid, doc, &child, child.kind(), depth + 1, cbase);
            }
        }
        _ => {}
    }
}

/// Add a cell offset to a local position.
fn offset(base: (usize, usize), pos: (usize, usize)) -> (usize, usize) {
    (base.0 + pos.0, base.1 + pos.1)
}

/// Read 1-based `row`/`col` from a block as a 0-based offset (clamped).
fn cell_pos(block: &Block<'_>) -> (usize, usize) {
    let row = (field_i64(block, "row").unwrap_or(1) - 1).max(0) as usize;
    let col = (field_i64(block, "col").unwrap_or(1) - 1).max(0) as usize;
    (row, col)
}

/// Read 1-based `row`/`col` from a variant payload as a 0-based offset.
fn map_pos(map: &std::collections::BTreeMap<String, Value>) -> (usize, usize) {
    let row = (map_i64(map, "row").unwrap_or(1) - 1).max(0) as usize;
    let col = (map_i64(map, "col").unwrap_or(1) - 1).max(0) as usize;
    (row, col)
}

fn prim_style(block: &Block<'_>) -> Style {
    Style {
        bold: field_bool(block, "bold").unwrap_or(false),
        dim: field_bool(block, "dim").unwrap_or(false),
        italic: field_bool(block, "italic").unwrap_or(false),
        underline: field_bool(block, "underline").unwrap_or(false),
        strike: field_bool(block, "strike").unwrap_or(false),
        blink: field_bool(block, "blink").unwrap_or(false),
        inverse: field_bool(block, "inverse").unwrap_or(false),
        conceal: field_bool(block, "conceal").unwrap_or(false),
    }
}

/// Variant-payload counterpart of [`pcolor`].
fn vcolor(map: &std::collections::BTreeMap<String, Value>, name: &str) -> Color {
    map_utf8(map, name)
        .map(|s| parse_color(&s))
        .unwrap_or(Color::Default)
}

/// Variant-payload counterpart of [`prim_style`].
fn vstyle(map: &std::collections::BTreeMap<String, Value>) -> Style {
    Style {
        bold: map_bool(map, "bold").unwrap_or(false),
        dim: map_bool(map, "dim").unwrap_or(false),
        italic: map_bool(map, "italic").unwrap_or(false),
        underline: map_bool(map, "underline").unwrap_or(false),
        strike: map_bool(map, "strike").unwrap_or(false),
        blink: map_bool(map, "blink").unwrap_or(false),
        inverse: map_bool(map, "inverse").unwrap_or(false),
        conceal: map_bool(map, "conceal").unwrap_or(false),
    }
}

// ── Drawing the one primitive: styled text ───────────────────────────

fn draw_text(
    grid: &mut Grid,
    row: usize,
    col: usize,
    content: &str,
    fg: Color,
    bg: Color,
    st: Style,
) {
    for (dr, line) in content.split('\n').enumerate() {
        for (dc, ch) in line.chars().enumerate() {
            grid.set(
                row + dr,
                col + dc,
                Cell {
                    ch,
                    fg,
                    bg,
                    style: st,
                },
            );
        }
    }
}

/// The `term_text` primitive, read from its `Block` and drawn at
/// `base + its position`.
fn prim_text(grid: &mut Grid, block: &Block<'_>, base: (usize, usize)) {
    let (row, col) = offset(base, cell_pos(block));
    let content = label_string(block).unwrap_or_default();
    draw_text(
        grid,
        row,
        col,
        &content,
        pcolor(block, "fg"),
        pcolor(block, "bg"),
        prim_style(block),
    );
}

/// A `TermFundamental::Text` run (emitted by an element's `lower`), read
/// from its variant payload and drawn at `base + its position`.
fn draw_text_variant(
    grid: &mut Grid,
    map: &std::collections::BTreeMap<String, Value>,
    base: (usize, usize),
) {
    let (row, col) = offset(base, map_pos(map));
    let content = map_utf8(map, "content").unwrap_or_default();
    draw_text(
        grid,
        row,
        col,
        &content,
        vcolor(map, "fg"),
        vcolor(map, "bg"),
        vstyle(map),
    );
}

// ── Populators B & C: avt virtual terminal ──────────────────────────

fn avt_color(c: avt::Color) -> Color {
    match c {
        avt::Color::Indexed(i) => Color::Indexed(i),
        avt::Color::RGB(rgb) => Color::Rgb(rgb.r, rgb.g, rgb.b),
    }
}

/// Snapshot the visible screen of an `avt` virtual terminal into a grid.
fn snapshot(vt: &avt::Vt, cols: usize, rows: usize) -> Grid {
    let mut grid = Grid::new(cols, rows);
    for (r, line) in vt.view().enumerate().take(rows) {
        for (c, cell) in line.cells().iter().enumerate().take(cols) {
            let pen = cell.pen();
            grid.set(
                r,
                c,
                Cell {
                    ch: cell.char(),
                    fg: pen.foreground().map(avt_color).unwrap_or(Color::Default),
                    bg: pen.background().map(avt_color).unwrap_or(Color::Default),
                    style: Style {
                        bold: pen.is_bold(),
                        dim: pen.is_faint(),
                        italic: pen.is_italic(),
                        underline: pen.is_underline(),
                        strike: pen.is_strikethrough(),
                        blink: pen.is_blink(),
                        inverse: pen.is_inverse(),
                        conceal: false,
                    },
                },
            );
        }
    }
    let cur = vt.cursor();
    if cur.visible {
        grid.cursor = Some((cur.col, cur.row));
    }
    grid
}

/// Feed inline `text` to a fresh virtual terminal and snapshot one grid.
/// Bare `\n` is promoted to `\r\n` so each authored line starts at
/// column 0 (a terminal's line feed alone only moves down a row).
fn populate_inline(cols: usize, rows: usize, text: &str) -> Grid {
    let normalized = text.replace("\r\n", "\n").replace('\n', "\r\n");
    let mut vt = avt::Vt::new(cols, rows);
    vt.feed_str(&normalized);
    snapshot(&vt, cols, rows)
}

/// One replay frame: its start time (ms) and the screen at that point.
struct Frame {
    t_ms: u32,
    grid: Grid,
}

/// Parsed asciicast: the terminal size plus the replay frames.
struct Cast {
    cols: usize,
    rows: usize,
    frames: Vec<Frame>,
}

/// Parse an asciicast v2 recording and replay it into coalesced frames.
/// Falls back to the block's `cols`/`rows` when the header omits a size.
fn parse_cast(src: &str, def_cols: usize, def_rows: usize) -> Cast {
    let mut lines = src.lines().filter(|l| !l.trim().is_empty());
    let (cols, rows) = lines
        .next()
        .and_then(|h| serde_json::from_str::<serde_json::Value>(h).ok())
        .map(|h| {
            (
                h.get("width")
                    .and_then(serde_json::Value::as_u64)
                    .map_or(def_cols, |v| v as usize),
                h.get("height")
                    .and_then(serde_json::Value::as_u64)
                    .map_or(def_rows, |v| v as usize),
            )
        })
        .unwrap_or((def_cols, def_rows));
    let cols = cols.max(1);
    let rows = rows.max(1);

    let mut vt = avt::Vt::new(cols, rows);
    let mut frames: Vec<Frame> = Vec::new();
    let mut last_t = f64::NEG_INFINITY;
    let mut last_data_t = 0.0;
    for line in lines {
        let Ok(serde_json::Value::Array(ev)) = serde_json::from_str::<serde_json::Value>(line)
        else {
            continue;
        };
        let (Some(t), Some(code), Some(data)) = (
            ev.first().and_then(serde_json::Value::as_f64),
            ev.get(1).and_then(serde_json::Value::as_str),
            ev.get(2).and_then(serde_json::Value::as_str),
        ) else {
            continue;
        };
        if code != "o" {
            continue;
        }
        vt.feed_str(data);
        last_data_t = t;
        if t - last_t >= MIN_FRAME_DT {
            frames.push(Frame {
                t_ms: (t * 1000.0).max(0.0) as u32,
                grid: snapshot(&vt, cols, rows),
            });
            last_t = t;
        }
    }
    // Capture the final state so the recording ends on its last screen,
    // unless the last event already produced a frame at that time (which
    // would just duplicate it).
    let final_t = (last_data_t * 1000.0).max(0.0) as u32;
    if frames.last().is_none_or(|f| f.t_ms != final_t) {
        frames.push(Frame {
            t_ms: final_t,
            grid: snapshot(&vt, cols, rows),
        });
    }
    // A recording whose first event is delayed should start blank.
    if frames.first().is_some_and(|f| f.t_ms > 0) {
        frames.insert(
            0,
            Frame {
                t_ms: 0,
                grid: Grid::new(cols, rows),
            },
        );
    }
    Cast { cols, rows, frames }
}

// ── Frame JSON for the JS player ────────────────────────────────────

fn run_to_json(run: &Run) -> serde_json::Value {
    serde_json::json!([
        run.col,
        run.text,
        ink(run.fg),
        run.bg.map(hex).unwrap_or_default(),
        run.flags,
    ])
}

fn frame_to_json(frame: &Frame, pal: &Palette) -> serde_json::Value {
    let rows: Vec<serde_json::Value> = grid_to_runs(&frame.grid, pal)
        .iter()
        .map(|runs| serde_json::Value::Array(runs.iter().map(run_to_json).collect()))
        .collect();
    let mut obj = serde_json::Map::new();
    obj.insert("t".into(), frame.t_ms.into());
    obj.insert("rows".into(), serde_json::Value::Array(rows));
    if let Some((c, r)) = frame.grid.cursor {
        obj.insert("cur".into(), serde_json::json!([c, r]));
    }
    serde_json::Value::Object(obj)
}

fn frames_json(cast: &Cast, pal: &Palette, g: &Geom, opts: &Opts) -> String {
    let frames: Vec<serde_json::Value> =
        cast.frames.iter().map(|f| frame_to_json(f, pal)).collect();
    let payload = serde_json::json!({
        "cols": g.cols,
        "rows": g.rows,
        "cw": g.cw,
        "ch": g.ch,
        "left": g.left,
        "top": g.top,
        "baseline": BASELINE_RATIO,
        "loop": opts.loop_,
        "autoplay": opts.autoplay,
        "speed": opts.speed,
        "frames": frames,
    });
    payload.to_string()
}

// ── Public entry ────────────────────────────────────────────────────

struct Opts {
    autoplay: bool,
    loop_: bool,
    speed: f64,
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
    let pal = Palette::new(preset.as_deref(), pal_fg.as_deref(), pal_bg.as_deref());

    let mut classes = vec!["wdoc-terminal".to_string()];
    classes.extend(user_classes);
    let class_attr = classes.join(" ");
    let style_attr = div_style(preset.as_deref(), fg_field.as_deref(), bg_field.as_deref());
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
    let svg = grid_svg(&grid, &pal, &g, title.as_deref(), None, false);
    format!("<div class=\"{class_attr}\"{style_attr}{id_attr}>{svg}</div>")
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

/// Inline `style` for the terminal `<div>` from explicit `fg`/`bg`
/// fields and the `:light` preset (these override any `class`). Empty
/// when the terminal relies purely on classes / the default theme.
fn div_style(preset: Option<&str>, fg: Option<&str>, bg: Option<&str>) -> String {
    let mut s = String::new();
    let bg = bg
        .map(str::to_string)
        .or_else(|| (preset == Some("light")).then(|| "#ffffff".to_string()));
    let fg = fg
        .map(str::to_string)
        .or_else(|| (preset == Some("light")).then(|| "#1c1c1c".to_string()));
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

#[allow(clippy::too_many_arguments)]
fn render_replay(
    block: &Block<'_>,
    base_dir: Option<&Path>,
    src_rel: &str,
    def_cols: usize,
    def_rows: usize,
    font_px: f64,
    line_height: f64,
    chrome: bool,
    title: Option<&str>,
    pal: &Palette,
    class_attr: &str,
    style_attr: &str,
    id_attr: &str,
) -> String {
    let path = match base_dir {
        Some(dir) => dir.join(src_rel),
        None => Path::new(src_rel).to_path_buf(),
    };
    let Ok(src) = std::fs::read_to_string(&path) else {
        return format!(
            "<div class=\"{class_attr} wdoc-terminal-error\"{id_attr}>cannot read cast: {}</div>",
            escape_html(&path.display().to_string())
        );
    };
    let cast = parse_cast(&src, def_cols, def_rows);
    let g = Geom::new(cast.cols, cast.rows, font_px, line_height, chrome);
    let opts = Opts {
        autoplay: field_bool(block, "autoplay").unwrap_or(false),
        loop_: field_bool(block, "loop").unwrap_or(false),
        speed: field_f64(block, "speed").unwrap_or(1.0),
    };

    // A stable id ties the SVG cell group, the controls, and the frames
    // JSON together for the player.
    let pid = format!(
        "wterm-{:x}",
        (path.to_string_lossy().len() as u64) ^ (cast.frames.len() as u64).rotate_left(17)
    );
    let cell_id = format!("{pid}-cells");
    let first = cast
        .frames
        .first()
        .map(|f| &f.grid)
        .expect("parse_cast always yields at least one frame");
    let svg = grid_svg(first, pal, &g, title, Some(&cell_id), true);
    let json = frames_json(&cast, pal, &g, &opts);

    // Controls: a big centred play button overlaid on the terminal, plus
    // the play/pause/replay glyph the renderer placed in the chrome next
    // to the ✕. The JS player wires both. No bottom scrubber/speed UI.
    format!(
        "<div class=\"{class_attr} wdoc-terminal-player\"{style_attr}{id_attr} data-term-player=\"{pid}\" data-term-cells=\"{cell_id}\">\
         {svg}\
         <button type=\"button\" class=\"term-overlay-play\" aria-label=\"Play\">\u{25B6}\u{FE0E}</button>\
         <script type=\"application/json\" class=\"term-frames\" data-for=\"{pid}\">{json}</script>\
         </div>",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_forms() {
        assert_eq!(parse_hex("#ff0000"), Some((255, 0, 0)));
        assert_eq!(parse_hex("#0f0"), Some((0, 255, 0)));
        assert_eq!(parse_hex("nope"), None);
    }

    #[test]
    fn parse_color_names_and_indices() {
        assert!(matches!(parse_color("red"), Color::Indexed(1)));
        assert!(matches!(parse_color("bright_red"), Color::Indexed(9)));
        assert!(matches!(parse_color("brightblue"), Color::Indexed(12)));
        assert!(matches!(parse_color("200"), Color::Indexed(200)));
        assert!(matches!(parse_color("#102030"), Color::Rgb(16, 32, 48)));
        assert!(matches!(parse_color("default"), Color::Default));
    }

    #[test]
    fn palette_256_cube_and_grayscale() {
        let p = Palette::new(None, None, None);
        assert_eq!(p.indexed(1), TANGO[1]);
        // 16 is the first cube entry → (0,0,0).
        assert_eq!(p.indexed(16), (0, 0, 0));
        // 231 is the last cube entry → white.
        assert_eq!(p.indexed(231), (255, 255, 255));
        // greyscale ramp.
        assert_eq!(p.indexed(232), (8, 8, 8));
    }

    #[test]
    fn inverse_swaps_colors() {
        let pal = Palette::new(None, None, None);
        let cell = Cell {
            ch: 'x',
            fg: Color::Indexed(1), // red
            bg: Color::Default,
            style: Style {
                inverse: true,
                ..Style::default()
            },
        };
        let p = resolve(&cell, &pal);
        // Inverse materialises defaults: text takes the (default)
        // background colour, the block takes the red foreground.
        assert_eq!(p.fg, Some(pal.bg));
        assert_eq!(p.bg, Some(TANGO[1]));
    }

    #[test]
    fn plain_default_cell_uses_currentcolor() {
        let pal = Palette::new(None, None, None);
        // A plain default-coloured cell stays `None` (→ currentColor /
        // class theming), not a baked palette colour.
        let p = resolve(&Cell::default(), &pal);
        assert_eq!(p.fg, None);
        assert_eq!(p.bg, None);
        assert_eq!(ink(p.fg), "currentColor");
    }

    #[test]
    fn runs_group_by_style() {
        let pal = Palette::new(None, None, None);
        let mut grid = Grid::new(4, 1);
        let red = Cell {
            ch: 'a',
            fg: Color::Indexed(1),
            bg: Color::Default,
            style: Style::default(),
        };
        grid.set(0, 0, red);
        grid.set(0, 1, red);
        grid.set(
            0,
            2,
            Cell {
                ch: 'b',
                fg: Color::Indexed(2),
                bg: Color::Default,
                style: Style::default(),
            },
        );
        let rows = grid_to_runs(&grid, &pal);
        assert_eq!(rows[0][0].text, "aa");
        assert_eq!(rows[0][1].text.chars().next(), Some('b'));
    }

    #[test]
    fn cast_replays_into_frames() {
        // Minimal asciicast v2: header + two output events.
        let cast =
            "{\"version\":2,\"width\":10,\"height\":2}\n[0.0,\"o\",\"hi\"]\n[0.5,\"o\",\"!\"]\n";
        let parsed = parse_cast(cast, 80, 24);
        assert_eq!((parsed.cols, parsed.rows), (10, 2));
        assert!(parsed.frames.len() >= 2);
        // Final frame shows the full "hi!" on row 0.
        let last = &parsed.frames[parsed.frames.len() - 1].grid;
        let text: String = last.row(0).iter().map(|c| c.ch).collect();
        assert!(text.starts_with("hi!"), "got {text:?}");
    }
}
