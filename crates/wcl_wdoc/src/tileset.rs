//! Tileset + tilemap support: a named spritesheet registry and the
//! placeable `tilemap` diagram block.
//!
//! A `tileset` block names an external image plus the geometry needed
//! to slice it into fixed-size tiles. Like the icon registry, this is
//! the first wdoc feature to consume a *user-provided* binary asset:
//! `TilesetRegistry::load` records each declared sheet (reading its
//! pixel dimensions from the file header), rendering records which
//! sheets actually get used, and `copy_used_images` copies those into
//! `_wdoc/` after the page loop. Pages reference the copied file by
//! relative URL (`_wdoc/tileset-<name>.png`), so — like icons — tiles
//! only resolve when the output is *served*, not opened via `file://`.
//!
//! Each tile is drawn as a nested `<svg>` whose `viewBox` windows into
//! the shared spritesheet (no per-tile clip-path, no per-tile data
//! URI), so a `tilemap` lowers to a single `<g class="wdoc-tilemap">`.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use wcl_lang::{Block, Document, Value};

use crate::build::BuildError;

/// Source-rectangle inset (in sheet pixels) applied to every tile to
/// stop the scaled spritesheet from bleeding the neighbouring tile in
/// at a tile's edge. Half a pixel is enough to keep samples inside the
/// tile while costing no visible content.
const HALF_PIXEL: f64 = 0.5;
use crate::render::{
    escape_html, field_bool, field_f64, field_i64, field_id, field_utf8, field_utf8_list,
    label_string,
};

/// Resolved configuration for one `@block("tileset")`.
struct TilesetConfig {
    /// The output file name within `_wdoc/` (`tileset-<name>.<ext>`),
    /// deterministic per tileset so two sheets never collide.
    out_file: String,
    /// Absolute / relative path to the source image on disk.
    src_path: PathBuf,
    tile_w: i64,
    tile_h: i64,
    /// Tiles per sheet row, used to turn a flat index into a source
    /// (col, row). Resolved to a positive value at load.
    columns: i64,
    margin: i64,
    spacing: i64,
    /// Sheet pixel dimensions, needed to size the inner `<image>`.
    img_w: i64,
    img_h: i64,
}

pub(crate) struct TilesetRegistry {
    sets: HashMap<String, TilesetConfig>,
    /// Names of tilesets referenced by a rendered tilemap. Only these
    /// images are copied into `_wdoc/`.
    used: RefCell<BTreeSet<String>>,
}

impl TilesetRegistry {
    /// Enumerate every `@block("tileset")` at the document root,
    /// resolving its source path (against `base_dir`) and reading the
    /// sheet's pixel dimensions from the file header unless the block
    /// gives them explicitly. A tileset whose dimensions can't be
    /// determined is a build error — it could never render correctly.
    pub(crate) fn load(doc: &Document, base_dir: Option<&Path>) -> Result<Self, BuildError> {
        let mut sets = HashMap::new();
        for block in doc.blocks() {
            if block.kind() != "tileset" {
                continue;
            }
            let Some(name) = label_string(&block) else {
                continue;
            };
            let source = field_utf8(&block, "source").ok_or_else(|| {
                BuildError::Tileset(format!("tileset \"{name}\" has no `source`"))
            })?;
            let src_path = match base_dir {
                Some(dir) => dir.join(&source),
                None => PathBuf::from(&source),
            };

            let tile_w = field_i64(&block, "tile_width").unwrap_or(0);
            let tile_h = field_i64(&block, "tile_height").unwrap_or(0);
            if tile_w <= 0 || tile_h <= 0 {
                return Err(BuildError::Tileset(format!(
                    "tileset \"{name}\" needs positive `tile_width` / `tile_height`"
                )));
            }
            let margin = field_i64(&block, "margin").unwrap_or(0).max(0);
            let spacing = field_i64(&block, "spacing").unwrap_or(0).max(0);

            // Pixel dimensions: explicit fields win; otherwise read the
            // file header. Either way both axes must resolve.
            let (img_w, img_h) = match (
                field_i64(&block, "image_width"),
                field_i64(&block, "image_height"),
            ) {
                (Some(w), Some(h)) => (w, h),
                _ => {
                    let bytes = fs::read(&src_path).map_err(|e| {
                        BuildError::Tileset(format!(
                            "tileset \"{name}\": cannot read {} ({e})",
                            src_path.display()
                        ))
                    })?;
                    let (w, h) = image_dims(&bytes).ok_or_else(|| {
                        BuildError::Tileset(format!(
                            "tileset \"{name}\": could not read image dimensions from {} \
                             — set `image_width` / `image_height`",
                            src_path.display()
                        ))
                    })?;
                    (w as i64, h as i64)
                }
            };

            // `columns` defaults to as many whole tiles as fit across
            // the sheet (accounting for margin + spacing).
            let columns = match field_i64(&block, "columns") {
                Some(c) if c > 0 => c,
                _ => ((img_w - 2 * margin + spacing) / (tile_w + spacing)).max(1),
            };

            let ext = src_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("png");
            sets.insert(
                name.clone(),
                TilesetConfig {
                    out_file: format!("tileset-{name}.{ext}"),
                    src_path,
                    tile_w,
                    tile_h,
                    columns,
                    margin,
                    spacing,
                    img_w,
                    img_h,
                },
            );
        }
        Ok(TilesetRegistry {
            sets,
            used: RefCell::new(BTreeSet::new()),
        })
    }

    fn record(&self, name: &str) {
        self.used.borrow_mut().insert(name.to_string());
    }

    /// Resolve an emitted tilemap `<image href>` (a `_wdoc/tileset-…` URL)
    /// back to the sheet's raw bytes, for inlining into PDF-embedded SVG.
    pub(crate) fn bytes_for_url(&self, href: &str) -> Option<Vec<u8>> {
        let cfg = self.sets.values().find(|c| href.ends_with(&c.out_file))?;
        fs::read(&cfg.src_path).ok()
    }

    /// Copy every spritesheet referenced by a rendered tilemap into
    /// `<out>/_wdoc/`. No-op when no tilemap was rendered.
    pub(crate) fn copy_used_images(&self, out_dir: &Path) -> Result<(), BuildError> {
        let used = self.used.borrow();
        if used.is_empty() {
            return Ok(());
        }
        let dir = out_dir.join(crate::terminal::ASSET_DIR);
        fs::create_dir_all(&dir)
            .map_err(|e| BuildError::Io(e, format!("create_dir_all {}", dir.display())))?;
        for name in used.iter() {
            let Some(cfg) = self.sets.get(name) else {
                continue;
            };
            let dest = dir.join(&cfg.out_file);
            fs::copy(&cfg.src_path, &dest).map_err(|e| {
                BuildError::Io(
                    e,
                    format!("copy {} -> {}", cfg.src_path.display(), dest.display()),
                )
            })?;
        }
        Ok(())
    }
}

/// Render a `@block("tilemap")` to a `<g>` of nested-`<svg>` tile
/// crops. A miss (unknown `set`, no grid data) renders nothing —
/// best-effort, like a failed shape lowering.
pub(crate) fn render_tilemap(
    block: &Block<'_>,
    registry: &TilesetRegistry,
    parent_w: f64,
    parent_h: f64,
) -> String {
    let Some(set_name) = field_id(block, "set") else {
        return String::new();
    };
    let Some(cfg) = registry.sets.get(&set_name) else {
        return String::new();
    };
    let grid = read_grid(block);
    if grid.is_empty() {
        return String::new();
    }
    registry.record(&set_name);

    let scale = field_f64(block, "scale").unwrap_or(1.0);
    let tw = cfg.tile_w as f64;
    let th = cfg.tile_h as f64;
    let dtw = tw * scale;
    let dth = th * scale;
    let cols = grid.iter().map(Vec::len).max().unwrap_or(0);
    let rows = grid.len();
    let (ox, oy) = place(
        block,
        parent_w,
        parent_h,
        cols as f64 * dtw,
        rows as f64 * dth,
    );

    let empty = field_i64(block, "empty").unwrap_or(-1);
    let margin = cfg.margin as f64;
    let spacing = cfg.spacing as f64;
    let columns = cfg.columns.max(1);
    let href = format!("{}/{}", crate::terminal::ASSET_DIR, cfg.out_file);
    let href = escape_html(&href);

    // Snap each cell's destination edges to the integer pixel grid via
    // cumulative rounding: adjacent tiles then share an exact edge (no
    // sub-pixel gap from a fractional `scale`, no drift across a row),
    // and `shape-rendering="crispEdges"` on the clipping viewport keeps
    // that shared edge from being antialiased into a hairline seam.
    let edge_x = |c: usize| (ox + c as f64 * dtw).round();
    let edge_y = |r: usize| (oy + r as f64 * dth).round();

    let mut body = String::new();
    for (r, row) in grid.iter().enumerate() {
        for (c, &idx) in row.iter().enumerate() {
            if idx < 0 || idx == empty {
                continue;
            }
            let scol = (idx % columns) as f64;
            let srow = (idx / columns) as f64;
            // Sample the tile's interior, inset by half a source pixel on
            // each side. The `viewBox` clips to the tile region, but at the
            // exact edge the scaled image still blends in the neighbouring
            // sheet tile (a brown crate beside a water tile bleeds a brown
            // hairline); the inset keeps every sample strictly inside the
            // tile. Costs half a source pixel at the edges — invisible.
            let sx = margin + scol * (tw + spacing) + HALF_PIXEL;
            let sy = margin + srow * (th + spacing) + HALF_PIXEL;
            let sw = (tw - 2.0 * HALF_PIXEL).max(0.0);
            let sh = (th - 2.0 * HALF_PIXEL).max(0.0);
            let dx = edge_x(c);
            let dy = edge_y(r);
            let w = edge_x(c + 1) - dx;
            let h = edge_y(r + 1) - dy;
            let _ = write!(
                body,
                "<svg x=\"{dx}\" y=\"{dy}\" width=\"{w}\" height=\"{h}\" \
                 viewBox=\"{sx} {sy} {sw} {sh}\" preserveAspectRatio=\"none\" \
                 shape-rendering=\"crispEdges\">\
                 <image href=\"{href}\" x=\"0\" y=\"0\" width=\"{iw}\" height=\"{ih}\" \
                 preserveAspectRatio=\"none\"/></svg>",
                iw = cfg.img_w,
                ih = cfg.img_h,
            );
        }
    }

    let mut classes = vec!["wdoc-tilemap".to_string()];
    if field_bool(block, "smooth") == Some(true) {
        classes.push("smooth".to_string());
    }
    classes.extend(field_utf8_list(block, "class"));
    let joined = classes
        .iter()
        .map(|s| escape_html(s))
        .collect::<Vec<_>>()
        .join(" ");
    let mut out = format!("<g class=\"{joined}\"");
    if let Some(id) = field_id(block, "id") {
        let _ = write!(out, " id=\"{}\"", escape_html(&id));
    }
    let _ = write!(out, ">{body}</g>");
    out
}

/// The absolute-in-parent bounding box `(x, y, w, h)` of a tilemap,
/// for the collect pass (edges + viewBox fit). Zero-sized when the
/// `set` is unknown or there's no grid data.
pub(crate) fn tilemap_bbox(
    block: &Block<'_>,
    registry: &TilesetRegistry,
    parent_w: f64,
    parent_h: f64,
) -> (f64, f64, f64, f64) {
    let Some(set_name) = field_id(block, "set") else {
        return (0.0, 0.0, 0.0, 0.0);
    };
    let Some(cfg) = registry.sets.get(&set_name) else {
        return (0.0, 0.0, 0.0, 0.0);
    };
    let grid = read_grid(block);
    let scale = field_f64(block, "scale").unwrap_or(1.0);
    let dtw = cfg.tile_w as f64 * scale;
    let dth = cfg.tile_h as f64 * scale;
    let cols = grid.iter().map(Vec::len).max().unwrap_or(0);
    let rows = grid.len();
    let w = cols as f64 * dtw;
    let h = rows as f64 * dth;
    let (x, y) = place(block, parent_w, parent_h, w, h);
    (x, y, w, h)
}

/// Resolve a fixed-size raster's top-left corner. Like
/// `resolve_point_anchored` for shapes, but a far anchor
/// (`anchor_right` / `anchor_bottom`) offsets by the raster's own size —
/// the content is fixed-size, so anchors position it without resizing.
/// Shared by `tilemap` and the diagram `image` shape.
pub(crate) fn place(block: &Block<'_>, parent_w: f64, parent_h: f64, w: f64, h: f64) -> (f64, f64) {
    let x = match (
        field_f64(block, "anchor_left"),
        field_f64(block, "anchor_right"),
    ) {
        (Some(l), _) => l,
        (None, Some(r)) => parent_w - r - w,
        _ => field_f64(block, "x").unwrap_or(0.0),
    };
    let y = match (
        field_f64(block, "anchor_top"),
        field_f64(block, "anchor_bottom"),
    ) {
        (Some(t), _) => t,
        (None, Some(b)) => parent_h - b - h,
        _ => field_f64(block, "y").unwrap_or(0.0),
    };
    (x, y)
}

/// Build the index grid from the tilemap's `map` (symbolic, via the
/// `tile` legend) when present, else its numeric `tiles`. An empty
/// result means "nothing to draw".
fn read_grid(block: &Block<'_>) -> Vec<Vec<i64>> {
    let rows = field_utf8_list(block, "map");
    if !rows.is_empty() {
        let legend = read_legend(block);
        return rows
            .iter()
            .map(|line| {
                line.chars()
                    .map(|c| legend.get(&c).copied().unwrap_or(-1))
                    .collect()
            })
            .collect();
    }
    read_i64_grid(block, "tiles")
}

/// Read the `tile` legend (glyph -> index) entries off a tilemap.
fn read_legend(block: &Block<'_>) -> HashMap<char, i64> {
    let mut map = HashMap::new();
    for entry in block.blocks().filter(|b| b.kind() == "tile") {
        let Some(glyph) = label_string(&entry) else {
            continue;
        };
        let Some(ch) = glyph.chars().next() else {
            continue;
        };
        if let Some(idx) = field_i64(&entry, "index") {
            map.insert(ch, idx);
        }
    }
    map
}

/// Read a `list<list<i64>>` field as a grid of indices.
fn read_i64_grid(block: &Block<'_>, name: &str) -> Vec<Vec<i64>> {
    let Some(field) = block.field(name) else {
        return Vec::new();
    };
    let Ok(Value::List(rows)) = field.value() else {
        return Vec::new();
    };
    rows.iter()
        .map(|row| match row {
            Value::List(cells) => cells.iter().filter_map(as_i64).collect(),
            _ => Vec::new(),
        })
        .collect()
}

fn as_i64(v: &Value) -> Option<i64> {
    match v {
        Value::I64(n) => Some(*n),
        Value::I32(n) => Some(*n as i64),
        Value::U32(n) => Some(*n as i64),
        Value::U64(n) => Some(*n as i64),
        _ => None,
    }
}

/// Read a raster image's pixel dimensions from its header bytes.
/// Supports PNG, GIF and JPEG with a tiny no-dep reader (no image
/// decoding); returns `None` for anything else so the caller can fall
/// back to explicit `image_width` / `image_height`. Shared with the
/// `image` block's registry.
pub(crate) fn image_dims(b: &[u8]) -> Option<(u32, u32)> {
    png_dims(b).or_else(|| gif_dims(b)).or_else(|| jpeg_dims(b))
}

fn png_dims(b: &[u8]) -> Option<(u32, u32)> {
    const SIG: &[u8] = b"\x89PNG\r\n\x1a\n";
    if b.len() < 24 || &b[..8] != SIG {
        return None;
    }
    // IHDR is the first chunk: width @16, height @20 (big-endian u32).
    let w = u32::from_be_bytes([b[16], b[17], b[18], b[19]]);
    let h = u32::from_be_bytes([b[20], b[21], b[22], b[23]]);
    Some((w, h))
}

fn gif_dims(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 10 || (&b[..6] != b"GIF87a" && &b[..6] != b"GIF89a") {
        return None;
    }
    let w = u16::from_le_bytes([b[6], b[7]]) as u32;
    let h = u16::from_le_bytes([b[8], b[9]]) as u32;
    Some((w, h))
}

fn jpeg_dims(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 4 || b[0] != 0xFF || b[1] != 0xD8 {
        return None;
    }
    let mut i = 2usize;
    // The widest read below is b[i + 8] (a SOF segment's width low byte).
    while i + 8 < b.len() {
        if b[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = b[i + 1];
        // Padding fill byte, or standalone markers carrying no length.
        if marker == 0xFF {
            i += 1;
            continue;
        }
        if marker == 0x01 || (0xD0..=0xD9).contains(&marker) {
            i += 2;
            continue;
        }
        let len = ((b[i + 2] as usize) << 8) | b[i + 3] as usize;
        // Start-of-frame markers carry height then width (big-endian).
        let is_sof = matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF);
        if is_sof {
            let h = u16::from_be_bytes([b[i + 5], b[i + 6]]) as u32;
            let w = u16::from_be_bytes([b[i + 7], b[i + 8]]) as u32;
            return Some((w, h));
        }
        i += 2 + len;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 64×48 fake PNG: a valid 8-byte signature + IHDR chunk header
    /// carrying the dimensions. We only ever read IHDR, so the pixel
    /// data / CRCs needn't be valid.
    fn fake_png(w: u32, h: u32) -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
        v.extend_from_slice(&[0, 0, 0, 13]); // IHDR length
        v.extend_from_slice(b"IHDR");
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&[8, 6, 0, 0, 0]); // bit depth, colour type, …
        v
    }

    #[test]
    fn reads_png_dimensions() {
        assert_eq!(image_dims(&fake_png(64, 48)), Some((64, 48)));
    }

    #[test]
    fn reads_gif_dimensions() {
        let mut v = b"GIF89a".to_vec();
        v.extend_from_slice(&32u16.to_le_bytes());
        v.extend_from_slice(&16u16.to_le_bytes());
        assert_eq!(image_dims(&v), Some((32, 16)));
    }

    #[test]
    fn reads_jpeg_dimensions() {
        // FF D8, then a SOF0 (FF C0) segment: length, precision,
        // height, width.
        let mut v = vec![0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x11, 0x08];
        v.extend_from_slice(&100u16.to_be_bytes()); // height
        v.extend_from_slice(&200u16.to_be_bytes()); // width
        assert_eq!(image_dims(&v), Some((200, 100)));
    }

    #[test]
    fn rejects_unknown_format() {
        assert_eq!(image_dims(b"not an image"), None);
    }
}
