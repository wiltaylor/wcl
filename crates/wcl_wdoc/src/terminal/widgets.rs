//! Populating the grid from a terminal's child blocks: the one base
//! primitive (`term_text`) drawn directly, and every other child kind
//! lowered into `TermFundamental::Text` runs via its `lower` function
//! (the cell-grid analogue of the SVG/HTML `lower` dispatch).

use super::*;

use wcl_lang::{Block, Document, Value, VariantPayload};

use crate::render::{
    MAX_LOWER_DEPTH, ValueSource, block_to_record_raw, kind_for_variant, label_string,
    lookup_block_lower, map_utf8, value_as_i64,
};

/// Walk a terminal's children, drawing each into the grid. The one base
/// primitive (`term_text`) draws directly; every other child kind is a
/// higher-level element — its `lower` function decomposes it into
/// `TermFundamental::Text` runs (boxes, fills, glyphs, and the `tui_*`
/// controls are all just text), which we recursively draw (the cell-grid
/// analogue of the SVG/HTML `lower` dispatch). The root content origin is
/// `(0, 0)`, so top-level text is unaffected.
pub(super) fn populate_primitives(grid: &mut Grid, doc: &Document, block: &Block<'_>) {
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
    let wbase = offset(base, src_pos(block));
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
            let cbase = offset(wbase, src_pos(map));
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

/// Read 1-based `row`/`col` (block field or variant payload) as a 0-based
/// offset, clamped at the top-left.
fn src_pos<S: ValueSource>(s: S) -> (usize, usize) {
    let read = |name: &str| s.lookup(name).as_ref().and_then(value_as_i64).unwrap_or(1);
    let row = (read("row") - 1).max(0) as usize;
    let col = (read("col") - 1).max(0) as usize;
    (row, col)
}

/// Read the eight ANSI style bits from either a `term_text` block or a
/// lowered `Text` variant payload; an absent or non-bool field is `false`.
fn src_style<S: ValueSource>(s: S) -> Style {
    let on = |name: &str| matches!(s.lookup(name), Some(Value::Bool(true)));
    Style {
        bold: on("bold"),
        dim: on("dim"),
        italic: on("italic"),
        underline: on("underline"),
        strike: on("strike"),
        blink: on("blink"),
        inverse: on("inverse"),
        conceal: on("conceal"),
    }
}

/// Read a colour field from either a `term_text` block or a lowered
/// `Text` variant payload (`fg`/`bg`); absent or non-string ⇒ default.
fn src_color<S: ValueSource>(s: S, name: &str) -> Color {
    match s.lookup(name) {
        Some(Value::Utf8(x) | Value::Ascii(x)) => parse_color(&x),
        _ => Color::Default,
    }
}

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
    let (row, col) = offset(base, src_pos(block));
    let content = label_string(block).unwrap_or_default();
    draw_text(
        grid,
        row,
        col,
        &content,
        src_color(block, "fg"),
        src_color(block, "bg"),
        src_style(block),
    );
}

/// A `TermFundamental::Text` run (emitted by an element's `lower`), read
/// from its variant payload and drawn at `base + its position`.
fn draw_text_variant(
    grid: &mut Grid,
    map: &std::collections::BTreeMap<String, Value>,
    base: (usize, usize),
) {
    let (row, col) = offset(base, src_pos(map));
    let content = map_utf8(map, "content").unwrap_or_default();
    draw_text(
        grid,
        row,
        col,
        &content,
        src_color(map, "fg"),
        src_color(map, "bg"),
        src_style(map),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn src_helpers_read_payload_maps() {
        use std::collections::BTreeMap;
        let mut map: BTreeMap<String, Value> = BTreeMap::new();
        map.insert("fg".into(), Value::Utf8("red".into()));
        map.insert("row".into(), Value::I64(3));
        map.insert("col".into(), Value::I64(5));
        map.insert("bold".into(), Value::Bool(true));
        map.insert("italic".into(), Value::Bool(true));

        // The generic readers replace the old vcolor / vstyle / map_pos.
        assert!(matches!(src_color(&map, "fg"), Color::Indexed(1)));
        assert!(matches!(src_color(&map, "bg"), Color::Default)); // absent ⇒ default
        assert_eq!(src_pos(&map), (2, 4)); // 1-based row/col → 0-based offset
        let st = src_style(&map);
        assert!(st.bold && st.italic);
        assert!(!st.underline && !st.dim);
    }
}
