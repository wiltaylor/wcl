//! The `avt` virtual-terminal paths: snapshotting inline `text` into a
//! grid, parsing an asciicast recording into coalesced [`Frame`]s, and —
//! for replay — serialising those frames to the JSON the bundled JS
//! player steps through plus building the player `<div>`.

use super::*;

use std::path::Path;

use wcl_lang::Block;

use crate::render::{escape_html, field_bool, field_f64};

/// Minimum frame spacing for replay (seconds): events closer than this
/// are coalesced into one frame so a busy recording stays small.
const MIN_FRAME_DT: f64 = 1.0 / 30.0;

/// Map the VT emulator's colour type onto this crate's.
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
pub(super) fn populate_inline(cols: usize, rows: usize, text: &str) -> Grid {
    let normalized = text.replace("\r\n", "\n").replace('\n', "\r\n");
    let mut vt = avt::Vt::new(cols, rows);
    vt.feed_str(&normalized);
    snapshot(&vt, cols, rows)
}

/// One replay frame: its start time (ms) and the screen at that point.
pub(super) struct Frame {
    /// When this frame starts, in ms from the recording start.
    pub(super) t_ms: u32,
    /// The screen at that moment.
    pub(super) grid: Grid,
}

/// Parsed asciicast: the terminal size plus the replay frames.
pub(super) struct Cast {
    /// Terminal width in cells.
    pub(super) cols: usize,
    /// Terminal height in cells.
    pub(super) rows: usize,
    /// Coalesced frames, in time order.
    pub(super) frames: Vec<Frame>,
}

/// Parse an asciicast v2 recording and replay it into coalesced frames.
/// Falls back to the block's `cols`/`rows` when the header omits a size.
pub(super) fn parse_cast(src: &str, def_cols: usize, def_rows: usize) -> Cast {
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

/// Serialize one run into the compact array the player expects.
fn run_to_json(run: &Run) -> serde_json::Value {
    serde_json::json!([
        run.col,
        run.text,
        ink(run.fg),
        run.bg.map(hex).unwrap_or_default(),
        run.flags,
    ])
}

/// Serialize one frame into its player representation.
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

/// Serialize the whole recording as the JSON payload embedded beside
/// the SVG for the player to drive.
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

/// Playback options read from the `terminal` block.
struct Opts {
    /// Start playing without user interaction.
    autoplay: bool,
    /// Restart when the recording ends.
    loop_: bool,
    /// Playback rate multiplier.
    speed: f64,
}

#[allow(clippy::too_many_arguments)]
/// Render a recording as a static first frame plus the JSON payload
/// the player animates.
pub(super) fn render_replay(
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
    let svg = grid_svg(first, pal, &g, title, Some(&cell_id), true, false);
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
