//! The `tree` shape: an indented file-tree. Each `tree_node` is one
//! fixed-height row, indented by its depth, with ├─ └─ │ connector guides
//! drawn between a parent and its children — the classic file-explorer
//! look. A node carries a `title` (its positional label) plus an optional
//! `icon` and `color`.
//!
//! Like `card` / `node_table`, it is `@native` (see [`crate::native`]):
//! the guides, icons and labels are SVG primitives drawn here. The whole
//! tree is a positioned shape (anchor-aware like `rect`); its height is
//! *derived* — one `row_height` per node — because the renderer can't
//! measure content.
//!
//! Per-node connectivity reuses the edge machinery: a node with an `id` is
//! registered as its own sub-shape (see `collect_shape_positions` in
//! `src/render/svg/shapes.rs`), so an edge can target a single node.

use std::fmt::Write as _;

use wcl_lang::Block;

use crate::icons::ShapeOverride;
use crate::render::{
    RenderCtx, escape_html, expand_container_children, field_f64, field_id, field_utf8,
    field_utf8_list, label_string, resolve_rect_box,
};

/// Default per-node row height when `row_height` is unset.
const ROW_HEIGHT: f64 = 24.0;
/// Default horizontal indent per depth level when `indent` is unset.
const INDENT: f64 = 18.0;
/// Side length of a node's icon glyph.
const ICON_SIZE: f64 = 16.0;
/// Gap after the connector elbow before the row content starts.
const PAD: f64 = 4.0;
/// Gap between a node's icon and its label.
const GAP: f64 = 6.0;
/// Label font size (SVG user units).
const FONT_SIZE: f64 = 13.0;
/// Hard cap on nesting depth, a backstop against pathological trees.
const MAX_TREE_DEPTH: usize = 64;
/// Shared class attribute for every connector guide line.
const GUIDE_CLS: &str = " class=\"wdoc-tree-guide\"";

/// A tree row's height, from its declared field or the default.
fn row_height(block: &Block<'_>) -> f64 {
    field_f64(block, "row_height").unwrap_or(ROW_HEIGHT)
}

/// Horizontal indent applied per nesting level.
fn indent(block: &Block<'_>) -> f64 {
    field_f64(block, "indent").unwrap_or(INDENT)
}

/// The direct `tree_node` children of `block`, in source order, with
/// `wdoc_repeater` / `wdoc_instance` / component instances expanded in
/// place — so a tree (or a sub-tree) can be data-driven, mirroring the
/// `node_table` row collector.
fn child_nodes<'a>(block: &Block<'a>) -> Vec<Block<'a>> {
    expand_container_children(block)
        .into_iter()
        .filter(|child| child.kind() == "tree_node")
        .collect()
}

/// Flatten the nested nodes into the rendered row order (pre-order), each
/// paired with its connector profile: a `verticals` vector of length
/// `depth`, where entry `j` (for `j < depth - 1`) says whether a `│`
/// pass-through guide is drawn in ancestor column `j`, and the last entry
/// (`j == depth - 1`) says whether the node's own elbow is a `├` (the node
/// has a following sibling) or a `└` (it's the last child). Top-level
/// nodes have an empty profile (depth 0) and draw no connector.
fn flatten<'a>(block: &Block<'a>) -> Vec<(Block<'a>, Vec<bool>)> {
    let mut out = Vec::new();
    for root in child_nodes(block) {
        out.push((root.clone(), Vec::new()));
        walk(child_nodes(&root), Vec::new(), &mut out);
    }
    out
}

/// Flatten a node tree to rows, carrying for each the "is last child"
/// flags of its ancestors — which is what lets the emitter draw the
/// connecting rules.
fn walk<'a>(nodes: Vec<Block<'a>>, parent: Vec<bool>, out: &mut Vec<(Block<'a>, Vec<bool>)>) {
    if parent.len() >= MAX_TREE_DEPTH {
        return;
    }
    let n = nodes.len();
    for (i, node) in nodes.into_iter().enumerate() {
        let is_last = i + 1 == n;
        // The child's column flags are the parent's (its elbow column now
        // becomes a pass-through carrying "does this node continue") plus
        // the node's own "not last" flag for its elbow.
        let mut v = parent.clone();
        v.push(!is_last);
        out.push((node.clone(), v.clone()));
        walk(child_nodes(&node), v, out);
    }
}

/// Resolve the tree's absolute-local box. Width comes from `width` /
/// anchors (like `rect`); height is derived — one `row_height` per node.
pub(crate) fn tree_bbox(block: &Block<'_>, parent_w: f64, parent_h: f64) -> (f64, f64, f64, f64) {
    let (x, y, w, _) = resolve_rect_box(block, parent_w, parent_h);
    let h = flatten(block).len() as f64 * row_height(block);
    (x, y, w, h)
}

/// Each node paired with its absolute-local row bbox (full tree width, so
/// an edge attaches to the node's west / east edge). Shared by the
/// renderer and the position collector so SVG and anchors agree.
pub(crate) fn node_rows<'a>(
    block: &Block<'a>,
    x: f64,
    y: f64,
    w: f64,
) -> Vec<(Block<'a>, (f64, f64, f64, f64))> {
    let rh = row_height(block);
    flatten(block)
        .into_iter()
        .enumerate()
        .map(|(i, (node, _))| (node, (x, y + i as f64 * rh, w, rh)))
        .collect()
}

/// Render a `@block("tree")`: one row per node — connector guides, an
/// optional icon, and the label — drawn as SVG primitives.
pub(crate) fn render_tree(
    block: &Block<'_>,
    ctx: RenderCtx<'_>,
    parent_w: f64,
    parent_h: f64,
) -> String {
    let (x, y, _w, _h) = tree_bbox(block, parent_w, parent_h);
    let rh = row_height(block);
    let ind = indent(block);
    let base_classes = field_utf8_list(block, "class");

    let mut svg = String::new();
    for (i, (node, verticals)) in flatten(block).into_iter().enumerate() {
        let depth = verticals.len();
        let y_top = y + i as f64 * rh;
        let y_mid = y_top + rh / 2.0;

        // Connector guides: a `│` pass-through for each continuing
        // ancestor column, then the node's own `├` / `└` elbow.
        for (j, cont) in verticals.iter().enumerate() {
            let col_x = x + (j as f64 + 0.5) * ind;
            if j + 1 < depth {
                if *cont {
                    svg.push_str(&line(col_x, y_top, col_x, y_top + rh));
                }
            } else {
                let v_bottom = if *cont { y_top + rh } else { y_mid };
                svg.push_str(&line(col_x, y_top, col_x, v_bottom));
                svg.push_str(&line(col_x, y_mid, x + depth as f64 * ind, y_mid));
            }
        }

        // Content: an optional icon, then the label.
        let color = field_utf8(&node, "color");
        let mut content_x = x + depth as f64 * ind + PAD;
        if let Some(name) = field_utf8(&node, "icon") {
            let set = field_utf8(&node, "icon_set");
            let over = ShapeOverride {
                color: color.clone(),
                ..Default::default()
            };
            let geom = (content_x, y_mid - ICON_SIZE / 2.0, ICON_SIZE, ICON_SIZE);
            if let Some(icon_svg) = ctx.icons.resolve_shape(&name, set.as_deref(), geom, &over) {
                svg.push_str(&icon_svg);
                content_x += ICON_SIZE + GAP;
            }
        }

        let title = label_string(&node).unwrap_or_default();
        let mut cls = String::from("wdoc-tree-label");
        let node_classes = field_utf8_list(&node, "class");
        for c in base_classes.iter().chain(node_classes.iter()) {
            cls.push(' ');
            cls.push_str(&escape_html(c));
        }
        let id_attr = field_id(&node, "id")
            .map(|id| format!(" data-tree-node-id=\"{}\"", escape_html(&id)))
            .unwrap_or_default();
        let fill_attr = color
            .as_deref()
            .map(|c| format!(" fill=\"{}\"", escape_html(c)))
            .unwrap_or_default();
        let _ = write!(
            svg,
            "<text class=\"{cls}\" x=\"{content_x}\" y=\"{y_mid}\" font-size=\"{FONT_SIZE}\" \
             text-anchor=\"start\" dominant-baseline=\"middle\"{fill_attr}{id_attr}>{}</text>",
            escape_html(&title)
        );
    }
    svg
}

/// A connector guide line (class-only paint; stroke comes from CSS).
fn line(x1: f64, y1: f64, x2: f64, y2: f64) -> String {
    crate::render::emit_line(GUIDE_CLS, x1, y1, x2, y2, None, None)
}
