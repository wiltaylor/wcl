//! SVG emission: resolving a [`Grid`] into coloured [`Run`]s, the
//! pixel [`Geom`]etry, and painting a static grid (window chrome + one
//! frame) as inline SVG. One renderer (`grid_to_runs` + `runs_to_svg`)
//! serves both the HTML path and the self-contained PDF path.

use super::*;

use crate::render::escape_html;

/// Cell advance as a fraction of font size. JetBrains Mono advances
/// 600/1000 em per glyph; the Nerd Font *Mono* variant forces every
/// glyph (icons included) to that single-cell width.
const CELL_W_RATIO: f64 = 0.6;
/// Baseline offset within a cell, as a fraction of the line box.
pub(super) const BASELINE_RATIO: f64 = 0.78;

// Style bit flags shared by the Rust emitter and the JS player.
/// Style flags packed into one byte per run, so a run compares and
/// copies cheaply while emitting.
const F_BOLD: u8 = 1;
/// Italic.
const F_ITALIC: u8 = 1 << 1;
/// Underline.
const F_UNDERLINE: u8 = 1 << 2;
/// Strikethrough.
const F_STRIKE: u8 = 1 << 3;
/// Blink.
const F_BLINK: u8 = 1 << 4;

/// A horizontal run of cells sharing fg / bg / flags. `fg`/`bg` are
/// `None` when the cell uses the terminal's *default* colour — fg
/// `None` renders as `currentColor` (so a WCL `class`'s `color` themes
/// it) and bg `None` renders nothing (the terminal `<div>`'s `class`
/// background shows through).
pub(super) struct Run {
    /// Column the run starts at.
    pub(super) col: usize,
    /// The run text.
    pub(super) text: String,
    /// Foreground colour; `None` means the terminal default.
    pub(super) fg: Option<(u8, u8, u8)>,
    /// Background colour; `None` means the terminal default.
    pub(super) bg: Option<(u8, u8, u8)>,
    /// Packed style flags.
    pub(super) flags: u8,
}

/// A cell's resolved presentation: fg/bg (`None` ⇒ default, themed by
/// the class system via `currentColor` / the `<div>` background) and
/// the style flags the emitter / player need.
struct Paint {
    /// Resolved foreground; `None` means default.
    fg: Option<(u8, u8, u8)>,
    /// Resolved background; `None` means default.
    bg: Option<(u8, u8, u8)>,
    /// Packed style flags.
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
pub(super) fn grid_to_runs(grid: &Grid, pal: &Palette) -> Vec<Vec<Run>> {
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

/// Pixel geometry of a rendered terminal: cell size, the grid it
/// holds, and the chrome around it.
pub(super) struct Geom {
    /// Terminal width in cells.
    pub(super) cols: usize,
    /// Terminal height in cells.
    pub(super) rows: usize,
    /// Cell width in px.
    pub(super) cw: f64,
    /// Cell height in px.
    pub(super) ch: f64,
    /// Font size in px.
    pub(super) font_px: f64,
    /// X of the grid origin, inside the chrome.
    pub(super) left: f64,
    /// Y of the grid origin, below the chrome.
    pub(super) top: f64,
    /// Height of the titlebar chrome.
    pub(super) chrome_h: f64,
    /// Total SVG width.
    pub(super) width: f64,
    /// Total SVG height.
    pub(super) height: f64,
}

impl Geom {
    /// Compute the geometry for a grid of the given size.
    pub(super) fn new(
        cols: usize,
        rows: usize,
        font_px: f64,
        line_height: f64,
        chrome: bool,
    ) -> Self {
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

/// SVG presentation attributes for a run's style flags.
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
///
/// `default_fg` controls how a *default*-coloured run (`run.fg == None`)
/// is painted: `None` ⇒ `currentColor` (the HTML path, themed by the
/// wrapping `<div>`'s class `color`); `Some(c)` ⇒ that concrete colour
/// (the self-contained PDF path, where there is no `<div>` / CSS to
/// resolve `currentColor` against the terminal theme).
fn runs_to_svg(rows: &[Vec<Run>], g: &Geom, default_fg: Option<(u8, u8, u8)>) -> String {
    let mut bgs = String::new();
    let mut fgs = String::new();
    // The HTML path gets the monospace font stack from the `.wdoc-terminal-svg
    // text` CSS rule; the self-contained PDF path has no injected CSS, so name
    // the embedded Nerd Font family explicitly (not the generic `monospace`,
    // which can resolve to the other bundled mono whose block glyphs don't
    // fill the cell, leaving gaps in the █/░ bars). `monospace` trails as a
    // fallback only.
    let font_attr = if default_fg.is_some() {
        format!(" font-family=\"'{NERD_FONT_FAMILY}', monospace\"")
    } else {
        String::new()
    };
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
            let fill = match default_fg {
                Some(d) => match run.fg {
                    Some(c) => hex(c),
                    None => hex(d),
                },
                None => ink(run.fg),
            };
            // Emit one centred glyph per cell. Spaces paint nothing, so
            // skip them unless the run is underlined/struck (then the
            // decoration must still span the blank cells).
            for (i, ch) in run.text.chars().enumerate() {
                if ch == ' ' && !has_deco {
                    continue;
                }
                let x0 = (run.col + i) as f64 * g.cw;
                // FULL BLOCK (█) means "fill the whole cell" — solid bars
                // (e.g. `tui_progress`) are runs of it. In the self-contained
                // SVG render it as a cell-spanning `<rect>` so adjacent blocks
                // tile seamlessly: a centred glyph per cell leaves hairline
                // gaps once embedded via usvg, however the font is named. (The
                // HTML path keeps the glyph — the browser tiles it fine.)
                if default_fg.is_some() && ch == '\u{2588}' {
                    fgs.push_str(&format!(
                        "<rect x=\"{x0:.2}\" y=\"{y_rect:.2}\" width=\"{:.2}\" height=\"{:.2}\" fill=\"{fill}\" shape-rendering=\"crispEdges\"/>",
                        g.cw, g.ch,
                    ));
                    continue;
                }
                let cx = x0 + g.cw / 2.0;
                fgs.push_str(&format!(
                    "<text x=\"{cx:.2}\" y=\"{y_text:.2}\" text-anchor=\"middle\" xml:space=\"preserve\" fill=\"{fill}\"{font_attr}{attrs}>{}</text>",
                    escape_html(&ch.to_string())
                ));
            }
        }
    }
    format!("{bgs}{fgs}")
}

/// Draw the cursor block, or nothing when the grid hides it.
fn cursor_svg(grid: &Grid, g: &Geom, sc_fg: Option<(u8, u8, u8)>) -> String {
    // The HTML path styles `.term-cursor` via CSS (`fill: currentColor;
    // opacity: 0.65`); the self-contained PDF path has no CSS, so bake the
    // same fill + opacity inline (else usvg paints the default solid black).
    let fill = match sc_fg {
        Some(c) => format!(" fill=\"{}\" fill-opacity=\"0.65\"", hex(c)),
        None => String::new(),
    };
    match grid.cursor {
        Some((col, row)) => format!(
            "<rect class=\"term-cursor\"{fill} x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\"/>",
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
///
/// When `self_contained` is set the SVG carries its own opaque window
/// background `<rect>` and bakes the terminal palette's default fg into
/// the cells + chrome (instead of `currentColor`), so it renders
/// correctly with no wrapping `<div>` / injected CSS — used by the PDF
/// backend, which embeds the bare `<svg>`.
pub(super) fn grid_svg(
    grid: &Grid,
    pal: &Palette,
    g: &Geom,
    title: Option<&str>,
    cell_group_id: Option<&str>,
    replay: bool,
    self_contained: bool,
) -> String {
    let default_fg = self_contained.then_some(pal.fg);
    let rows = grid_to_runs(grid, pal);
    let cells = runs_to_svg(&rows, g, default_fg);
    let cursor = cursor_svg(grid, g, default_fg);
    let id_attr = cell_group_id
        .map(|id| format!(" id=\"{}\"", escape_html(id)))
        .unwrap_or_default();
    // The HTML path paints no opaque window rect: the terminal background
    // is the wrapping `<div class="wdoc-terminal …">`'s CSS background, so
    // a WCL `class` (and its dark/light modes) themes it. The PDF
    // (`self_contained`) path has no such `<div>`, so it bakes a full-bounds
    // background rect from the palette before chrome + cells + glyphs.
    let window_bg = if self_contained {
        format!(
            "<rect x=\"0\" y=\"0\" width=\"{w:.0}\" height=\"{h:.0}\" fill=\"{bg}\"/>",
            w = g.width,
            h = g.height,
            bg = hex(pal.bg),
        )
    } else {
        String::new()
    };
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" class=\"wdoc-terminal-svg\" \
         width=\"{w:.0}\" height=\"{h:.0}\" viewBox=\"0 0 {w:.0} {h:.0}\">\
         {window_bg}{chrome}\
         <g class=\"term-cells\" transform=\"translate({left:.2} {top:.2})\"{id_attr}>{cells}{cursor}</g>\
         </svg>",
        w = g.width,
        h = g.height,
        chrome = chrome_svg(g, title, replay, default_fg),
        left = g.left,
        top = g.top,
    )
}

/// Window title bar: a close `✕` on the right, a centred title, and —
/// for a replay terminal — a play/pause/replay glyph to the left of the
/// `✕` (the JS player swaps its glyph and wires the click). Drawn only
/// when `chrome_h > 0`. Strokes/fills inherit the terminal text colour
/// (`currentColor`) via `TERMINAL_CSS`, so they follow the `class` theme.
///
/// `sc_fg` bakes explicit colours into the chrome instead of relying on
/// the `.term-*` CSS: `None` ⇒ the CSS-styled HTML path; `Some(fg)` ⇒
/// the self-contained PDF path, where the bar fill / title fill / close
/// stroke are emitted inline (same hues + opacities the CSS uses), since
/// usvg never sees the `wdoc-terminal` structured browser rules.
fn chrome_svg(g: &Geom, title: Option<&str>, replay: bool, sc_fg: Option<(u8, u8, u8)>) -> String {
    if g.chrome_h <= 0.0 {
        return String::new();
    }
    // Inline attrs for each chrome part when self-contained, else empty
    // (the CSS class supplies fill/stroke/opacity on the HTML path).
    let bar_attr = match sc_fg {
        Some(c) => format!(" fill=\"{}\" fill-opacity=\"0.08\"", hex(c)),
        None => String::new(),
    };
    let close_attr = match sc_fg {
        Some(c) => format!(
            " stroke=\"{}\" stroke-opacity=\"0.55\" stroke-width=\"1.5\" stroke-linecap=\"round\"",
            hex(c)
        ),
        None => String::new(),
    };
    // The title is sans-serif at ~0.75em via the `.term-title` CSS on the HTML
    // path; bake the family + size inline when self-contained (no injected CSS).
    let title_attr = match sc_fg {
        Some(c) => format!(
            " fill=\"{}\" fill-opacity=\"0.6\" font-family=\"sans-serif\" font-size=\"{:.1}\"",
            hex(c),
            g.font_px * 0.75
        ),
        None => String::new(),
    };
    let btn_attr = match sc_fg {
        Some(c) => format!(
            " fill=\"{}\" fill-opacity=\"0.55\" font-size=\"{:.1}\"",
            hex(c),
            g.font_px * 0.8
        ),
        None => String::new(),
    };
    let cy = g.chrome_h / 2.0;
    // Close glyph: two crossing strokes near the right edge, inset from
    // the right by roughly the cell-area padding.
    let s = (g.font_px * 0.24).max(3.0);
    let cx = g.width - g.left - s;
    let close = format!(
        "<g class=\"term-close\"{close_attr}><line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\"/>\
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
            "<text class=\"term-chrome-btn\"{btn_attr} x=\"{:.2}\" y=\"{:.2}\" text-anchor=\"middle\" aria-label=\"Play\">\u{25B6}\u{FE0E}</text>",
            cx - s - g.font_px,
            cy + g.font_px * 0.34,
        )
    } else {
        String::new()
    };
    let title_svg = match title {
        Some(t) if !t.is_empty() => format!(
            "<text class=\"term-title\"{title_attr} x=\"{:.2}\" y=\"{:.2}\" text-anchor=\"middle\">{}</text>",
            g.width / 2.0,
            cy + g.font_px * 0.32,
            escape_html(t)
        ),
        _ => String::new(),
    };
    format!(
        "<g class=\"term-chrome\"><rect x=\"0\" y=\"0\" width=\"{:.0}\" height=\"{:.2}\" class=\"term-chrome-bar\"{bar_attr}/>{title_svg}{play}{close}</g>",
        g.width, g.chrome_h,
    )
}
