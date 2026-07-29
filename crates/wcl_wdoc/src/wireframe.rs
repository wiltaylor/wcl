//! Wireframe UI mock-ups rendered as positioned diagram shapes.
//!
//! Wireframe widgets (`wf_window`, the `wf_browser`/`wf_phone`/`wf_tablet`
//! device frames, `wf_button`, `wf_panel`, the controls, and the
//! `wf_row`/`wf_column`/`wf_grid` layout containers) mock up an interface
//! from composable blocks. Each widget `extends SvgBlock`, so it is a **diagram
//! shape**: a legal child of any `diagram` / `container`, placed with `x`/`y`
//! (or anchors) and connectable by edges. Like `card` / `map`, the family is
//! special-cased in the renderer — [`render_wireframe_shape`] measures the
//! widget tree bottom-up and emits a single positioned SVG `<g>`, dispatched
//! from `render_shape` (and the same module sizes the widget for the diagram's
//! layout solvers via [`measured_size`] / [`wireframe_bbox`]).
//!
//! The renderer walks the **raw block tree** — reading fields directly and
//! recursing into container children via `block.blocks()` — so it never
//! depends on the WCL `lower` (now a stub). A widget's size is its measured
//! content; `x`/`y`/anchors only position the group inside the diagram.
//!
//! Theming follows the terminal pattern: neutral colours are the document's
//! resolved theme palette **roles** (`bg_alt` surface, `overlay` titlebar,
//! `bg_inset` controls, `border`, `fg`/`fg_muted` text, the site `accent` for
//! active states), baked into the SVG as concrete hex — so a wireframe reflects
//! the theme on any output (HTML or PDF) without `currentColor`. A widget's own
//! `class` `background`/`color`/`border` overrides its box fill / text / border.
//! The few glyphs (chevron, check, close ✕, dots) are native SVG shapes in
//! baked colours, so there's no icon-sprite dependency. Measurement is
//! theme-independent (the only `doc` use is `theme_of`, which feeds emission,
//! not size), so the layout/collect paths measure with no `doc` at all.

use std::collections::HashMap;

use wcl_lang::{Block, Document};

use crate::inline::InlinePatterns;
use crate::render::{
    RenderCtx, ThemeRoles, escape_html, expand_container_children, field_bool, field_f64,
    field_i64, field_id, field_symbol, field_utf8, field_utf8_list, label_string, resolve_rect_box,
    resolve_roles, shape_anchor_attrs,
};

// ── Geometry (px, ported from the wdoc-wireframe CSS rem values) ─────

const FONT: f64 = 14.0; // 0.9rem control text
const TITLE_FONT: f64 = 15.0; // titlebar / panel heading
const LINE_H: f64 = 19.0; // a text line's box height
const PAD: f64 = 13.0; // window body padding (0.8rem)
const PANEL_PAD: f64 = 10.0; // panel body padding (0.6rem)
const GAP: f64 = 10.0; // vertical gap between stacked widgets (0.6rem)
const ROW_GAP: f64 = 13.0; // horizontal gap in a row (0.8rem)
const CTRL_H: f64 = 30.0; // button / input / dropdown height
const CTRL_PAD_X: f64 = 11.0; // control horizontal padding
const RADIUS: f64 = 4.0;
const TITLEBAR_H: f64 = 30.0;
const BOX: f64 = 16.0; // checkbox square (1rem)
const DOT: f64 = 16.0; // radio circle (1rem)
const TRACK_W: f64 = 35.0; // toggle track (2.2rem)
const TRACK_H: f64 = 19.0; // toggle track (1.2rem)
const WIN_MIN_W: f64 = 256.0; // window min-width (16rem)
const ICON: f64 = 14.0;

// Layout-container guides (the editor's edit-mode chrome) and the placeholder
// footprint an EMPTY `wf_row`/`wf_column`/`wf_grid` occupies so it stays
// visible, selectable and droppable-into instead of collapsing to 0×0.
const EMPTY_CELL_W: f64 = 72.0; // one placeholder slot
const EMPTY_CELL_H: f64 = 34.0;
const GUIDE_FONT: f64 = 10.0; // the container kind tag

// Device frames (browser / phone / tablet). Unlike the other widgets, these
// have a realistic *fixed* default size so content sizes inside them properly;
// an explicit `width`/`height` pins that axis, and height grows past the
// default if the content would otherwise overflow.
const PHONE_W: f64 = 280.0; // phone portrait width
const PHONE_H: f64 = 580.0; // phone portrait height (landscape swaps the two)
const TABLET_W: f64 = 480.0; // tablet portrait width
const TABLET_H: f64 = 640.0; // tablet portrait height (landscape swaps)
const BROWSER_W: f64 = 640.0; // browser frame width
const BROWSER_H: f64 = 440.0; // browser frame height
const DEVICE_BEZEL: f64 = 12.0; // frame thickness around the screen
const STATUS_H: f64 = 26.0; // phone/tablet status bar height
const HOME_IND_H: f64 = 22.0; // bottom reserve for the home-indicator pill
const BROWSER_TOOLBAR_H: f64 = 62.0; // browser dots row + address bar

// Node-graph widget (boxes with ports, wired by links).
const NODE_HEADER_H: f64 = 26.0; // node title band
const NODE_PORT_ROW_H: f64 = 22.0; // height of one input/output port row
const NODE_BODY_PAD_Y: f64 = 8.0; // top/bottom padding of the port body
const NODE_PAD_X: f64 = 12.0; // node horizontal padding (title + port labels)
const NODE_MIN_W: f64 = 92.0; // node minimum width
const NODE_PORT_R: f64 = 4.0; // port marker radius
const PORT_FONT: f64 = 12.0; // port label text
const PORT_COL_GAP: f64 = 24.0; // gap between the input and output label columns
const GRAPH_LAYER_GAP: f64 = 64.0; // space between auto-layout layers (room for links)
const GRAPH_NODE_GAP: f64 = 28.0; // space between nodes within a layer
const GRAPH_PAD: f64 = 8.0; // padding around the whole graph

const SANS: &str = "system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif";

// ── Public entry points ─────────────────────────────────────────────

/// Render a wireframe widget as a positioned SVG `<g>` inside a diagram —
/// the diagram-shape entry point, dispatched from `render_shape` (analogous
/// to `render_card`). The box is positioned via `resolve_rect_box` (so `x`/`y`
/// and anchors work like any shape), but the **size is the measured content**
/// — a declared `width`/`height` is advisory only. Neutral colours are baked
/// from the resolved UI/application theme — the current site's `ui_*` theme,
/// overridden per-element by the root widget's own `theme`/`accent`/`mode`.
pub(crate) fn render_wireframe_shape(
    block: &Block<'_>,
    ctx: RenderCtx<'_>,
    parent_w: f64,
    parent_h: f64,
) -> String {
    // Per-element override (root widget) layered over the site UI theme.
    let base = ctx.patterns.ui_theme();
    let theme = field_symbol(block, "theme").unwrap_or(base.theme);
    let accent = field_symbol(block, "accent").unwrap_or(base.accent);
    let mode = field_symbol(block, "mode").unwrap_or(base.mode);
    let roles = resolve_roles(ctx.doc, &theme, &accent, &mode);
    let (x, y, _, _) = resolve_rect_box(block, parent_w, parent_h);
    let w = build(Some(ctx.doc), block, Some(ctx.patterns));
    let mut body = String::new();
    emit(&w, 0.0, 0.0, &roles, &mut body);
    format!("<g transform=\"translate({x:.2} {y:.2})\">{body}</g>")
}

/// The measured `(width, height)` of a wireframe widget — `doc`-free, because
/// a widget's size depends only on its text content + structure, not its theme.
/// Used by the diagram layout solvers (`effective_dims`) and the collect pass.
pub(crate) fn measured_size(block: &Block<'_>) -> (f64, f64) {
    let w = build(None, block, None);
    (w.w.max(1.0), w.h.max(1.0))
}

/// A wireframe widget's absolute bounding box for the diagram collect pass:
/// `x`/`y` from `resolve_rect_box`, size from the measured content. Mirrors
/// `render_wireframe_shape`'s geometry so edges + the viewBox fit agree with
/// what's drawn.
pub(crate) fn wireframe_bbox(
    block: &Block<'_>,
    parent_w: f64,
    parent_h: f64,
) -> (f64, f64, f64, f64) {
    let (x, y, _, _) = resolve_rect_box(block, parent_w, parent_h);
    let (w, h) = measured_size(block);
    (x, y, w, h)
}

/// Is `kind` a wireframe widget block (and thus rendered by this module)?
pub(crate) fn is_wireframe_kind(kind: &str) -> bool {
    matches!(
        kind,
        "wf_window"
            | "wf_browser"
            | "wf_phone"
            | "wf_tablet"
            | "wf_panel"
            | "wf_button"
            | "wf_input"
            | "wf_dropdown"
            | "wf_checkbox"
            | "wf_radio"
            | "wf_toggle"
            | "wf_label"
            | "wf_row"
            | "wf_column"
            | "wf_grid"
            | "wf_node_graph"
    )
}

// ── Model ───────────────────────────────────────────────────────────

#[derive(Default, Clone)]
struct Theme {
    bg: Option<String>,
    fg: Option<String>,
    border: Option<String>,
}

/// A measured widget: its kind-specific content, its laid-out `(w, h)`, and
/// whether it's disabled (dimmed). Children are measured before their parent,
/// so a container's size is known by the time it's built.
struct Widget {
    kind: Kind,
    w: f64,
    h: f64,
    disabled: bool,
    /// Pre-rendered `data-wcl-shape …` anchor attributes for a NESTED child
    /// widget (edit-mode builds only) — the emission wraps the widget in an
    /// anchored `<g>` so the editor can select, drag and re-home it. The
    /// ROOT widget's anchor is stamped by the diagram-child dispatch
    /// instead, so it stays `None` here.
    anchor: Option<String>,
    /// Edit-mode build (the editor's preview): layout containers draw their
    /// guide chrome — dashed boundary, kind tag, `data-wf-slot` drop zones.
    edit: bool,
}

enum Kind {
    Label {
        text: String,
        theme: Theme,
    },
    Button {
        text: String,
        theme: Theme,
    },
    Input {
        text: String,
        placeholder: bool,
        theme: Theme,
    },
    Dropdown {
        text: String,
        theme: Theme,
    },
    Checkbox {
        label: String,
        on: bool,
        theme: Theme,
    },
    Radio {
        label: String,
        on: bool,
        theme: Theme,
    },
    Toggle {
        label: Option<String>,
        on: bool,
        theme: Theme,
    },
    Window {
        title: String,
        controls: bool,
        body: Vec<Widget>,
        theme: Theme,
    },
    /// A web-browser frame: a toolbar (traffic-light dots + address bar) over a
    /// content area. The inline label is the address-bar URL.
    Browser {
        url: String,
        body: Vec<Widget>,
        theme: Theme,
    },
    /// A phone / tablet frame: a bezel around a screen with a status bar and a
    /// home-indicator pill. `tablet` picks the larger frame + squarer corners
    /// (orientation only affects the measured size, not the emitted chrome).
    /// The inline label is an optional status-bar caption.
    Device {
        tablet: bool,
        title: Option<String>,
        body: Vec<Widget>,
        theme: Theme,
    },
    Panel {
        title: Option<String>,
        body: Vec<Widget>,
        theme: Theme,
    },
    Row(Vec<Widget>),
    Column(Vec<Widget>),
    Grid {
        cols: usize,
        items: Vec<Widget>,
    },
    /// A node-graph editor: boxes with labeled input/output ports, wired by
    /// links. Nodes carry their final laid-out `(x, y)` (auto-layout offset, or
    /// an explicit pin) so emission just draws; links are routed at emit time.
    NodeGraph {
        nodes: Vec<GNode>,
        links: Vec<GLink>,
    },
    /// An unknown / unsupported child — rendered as nothing (zero size).
    Empty,
}

/// One node in a [`Kind::NodeGraph`]: a titled box with input ports down the
/// left edge and output ports down the right edge, placed at its laid-out
/// `(x, y)` in the graph's local space.
struct GNode {
    title: String,
    inputs: Vec<String>,
    outputs: Vec<String>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    theme: Theme,
}

/// One link in a [`Kind::NodeGraph`], from a source node's output port to a
/// destination node's input port (both stored as resolved indices).
struct GLink {
    src: usize,
    src_port: usize,
    dst: usize,
    dst_port: usize,
    label: Option<String>,
}

// ── Build (read fields + measure, bottom-up) ─────────────────────────

fn build(doc: Option<&Document>, block: &Block<'_>, patterns: Option<&InlinePatterns>) -> Widget {
    let disabled = field_bool(block, "disabled").unwrap_or(false);
    // Theme feeds emission, not size — the measure-only path passes `None`.
    let theme = doc.map(|d| theme_of(d, block)).unwrap_or_default();
    let kind = block.kind();
    let mut widget = match kind {
        "wf_label" => {
            let text = label_string(block).unwrap_or_default();
            sized(
                Kind::Label {
                    text: text.clone(),
                    theme,
                },
                text_w(&text, FONT) + 2.0,
                LINE_H,
                disabled,
            )
        }
        "wf_button" => {
            let text = label_string(block).unwrap_or_default();
            let w = text_w(&text, FONT) + 2.0 * CTRL_PAD_X;
            sized(Kind::Button { text, theme }, w, CTRL_H, disabled)
        }
        "wf_input" => {
            let value = field_utf8(block, "value");
            let placeholder = value.is_none();
            let text = value.or_else(|| label_string(block)).unwrap_or_default();
            let w = (text_w(&text, FONT) + 2.0 * CTRL_PAD_X).max(130.0);
            sized(
                Kind::Input {
                    text,
                    placeholder,
                    theme,
                },
                w,
                CTRL_H,
                disabled,
            )
        }
        "wf_dropdown" => {
            let text = label_string(block).unwrap_or_default();
            let w = (text_w(&text, FONT) + 2.0 * CTRL_PAD_X + ICON + 8.0).max(130.0);
            sized(Kind::Dropdown { text, theme }, w, CTRL_H, disabled)
        }
        "wf_checkbox" => {
            let label = label_string(block).unwrap_or_default();
            let on = field_bool(block, "checked").unwrap_or(false);
            let w = BOX + 7.0 + text_w(&label, FONT);
            sized(
                Kind::Checkbox { label, on, theme },
                w,
                LINE_H.max(BOX),
                disabled,
            )
        }
        "wf_radio" => {
            let label = label_string(block).unwrap_or_default();
            let on = field_bool(block, "selected").unwrap_or(false);
            let w = DOT + 7.0 + text_w(&label, FONT);
            sized(
                Kind::Radio { label, on, theme },
                w,
                LINE_H.max(DOT),
                disabled,
            )
        }
        "wf_toggle" => {
            let label = label_string(block);
            let on = field_bool(block, "on").unwrap_or(false);
            let lw = label.as_deref().map_or(0.0, |l| 7.0 + text_w(l, FONT));
            sized(
                Kind::Toggle { label, on, theme },
                TRACK_W + lw,
                LINE_H.max(TRACK_H),
                disabled,
            )
        }
        "wf_window" => {
            let title = label_string(block).unwrap_or_default();
            let controls = field_bool(block, "controls").unwrap_or(true);
            let body = child_widgets(doc, block, patterns);
            let (bw, bh) = column_size(&body);
            let ctrl_w = if controls { 56.0 } else { 0.0 };
            let head_w = text_w(&title, TITLE_FONT) + 16.0 + ctrl_w;
            let w = (bw + 2.0 * PAD).max(head_w + 2.0 * PAD).max(WIN_MIN_W);
            let h = TITLEBAR_H + 2.0 * PAD + bh;
            sized(
                Kind::Window {
                    title,
                    controls,
                    body,
                    theme,
                },
                w,
                h,
                disabled,
            )
        }
        "wf_browser" => {
            let url = label_string(block).unwrap_or_default();
            let body = child_widgets(doc, block, patterns);
            let (_, bh) = column_size(&body);
            let (w, h) = browser_size(bh, field_f64(block, "width"), field_f64(block, "height"));
            sized(Kind::Browser { url, body, theme }, w, h, disabled)
        }
        "wf_phone" | "wf_tablet" => {
            let tablet = kind == "wf_tablet";
            let landscape = field_symbol(block, "orientation").as_deref() == Some("landscape");
            let title = label_string(block);
            let body = child_widgets(doc, block, patterns);
            let (_, bh) = column_size(&body);
            let (w, h) = device_size(
                tablet,
                landscape,
                bh,
                field_f64(block, "width"),
                field_f64(block, "height"),
            );
            sized(
                Kind::Device {
                    tablet,
                    title,
                    body,
                    theme,
                },
                w,
                h,
                disabled,
            )
        }
        "wf_panel" => {
            let title = field_utf8(block, "title");
            let body = child_widgets(doc, block, patterns);
            let (bw, bh) = column_size(&body);
            let head_h = if title.is_some() { LINE_H + 4.0 } else { 0.0 };
            let head_w = title.as_deref().map_or(0.0, |t| text_w(t, FONT) + 12.0);
            let w = bw.max(head_w) + 2.0 * PANEL_PAD;
            let h = head_h + bh + 2.0 * PANEL_PAD;
            sized(Kind::Panel { title, body, theme }, w, h, disabled)
        }
        // An empty layout container keeps a small placeholder footprint (two
        // slots) instead of collapsing to 0×0, so the editor can see, select
        // and drop into it. Unconditional — not edit-gated — because the
        // measure paths (`measured_size`, `effective_dims`, the collect pass)
        // have no edit-mode signal, and the drawn box must match the measured
        // one everywhere. A real build just shows the blank space.
        "wf_row" => {
            let items = child_widgets(doc, block, patterns);
            let (w, h) = if items.is_empty() {
                (2.0 * EMPTY_CELL_W + ROW_GAP, EMPTY_CELL_H)
            } else {
                row_size(&items)
            };
            sized(Kind::Row(items), w, h, disabled)
        }
        "wf_column" => {
            let items = child_widgets(doc, block, patterns);
            let (w, h) = if items.is_empty() {
                (EMPTY_CELL_W, 2.0 * EMPTY_CELL_H + GAP)
            } else {
                column_size(&items)
            };
            sized(Kind::Column(items), w, h, disabled)
        }
        "wf_grid" => {
            // Fallback mirrors the schema default (`columns = 2` in
            // wireframe.wcl) — the raw-block walk doesn't see schema defaults.
            let cols = field_i64(block, "columns").unwrap_or(2).max(1) as usize;
            let items = child_widgets(doc, block, patterns);
            let (w, h) = grid_size(&items, cols);
            sized(Kind::Grid { cols, items }, w, h, disabled)
        }
        "wf_node_graph" => build_node_graph(doc, block, disabled),
        _ => sized(Kind::Empty, 0.0, 0.0, false),
    };
    widget.edit = patterns.is_some_and(|p| p.edit_mode());
    widget
}

// ── Node graph ───────────────────────────────────────────────────────

/// Build a `wf_node_graph`: read its `wf_node` / `wf_link` children directly
/// (like `node_table` reads `node_row`s), measure each node, place them with
/// the shared layered solver — a node's explicit `x`/`y` overrides its
/// auto-layout offset — and size the widget to the node bounding box.
fn build_node_graph(doc: Option<&Document>, block: &Block<'_>, disabled: bool) -> Widget {
    let dir = field_symbol(block, "direction")
        .and_then(|s| crate::layered::Direction::from_symbol(&s))
        .unwrap_or(crate::layered::Direction::LeftToRight);

    // Collect nodes (measured) and the link references, in source order.
    let mut nodes: Vec<GNode> = Vec::new();
    let mut id_to_idx: HashMap<String, usize> = HashMap::new();
    let mut pins: Vec<(Option<f64>, Option<f64>)> = Vec::new();
    let mut raw_links: Vec<(String, String, Option<String>)> = Vec::new();
    for child in block.blocks() {
        match child.kind() {
            "wf_node" => {
                let title = label_string(&child).unwrap_or_default();
                let inputs = field_utf8_list(&child, "inputs");
                let outputs = field_utf8_list(&child, "outputs");
                let theme = doc.map(|d| theme_of(d, &child)).unwrap_or_default();
                let (w, h) = node_size(&title, &inputs, &outputs);
                if let Some(id) = field_id(&child, "id") {
                    id_to_idx.insert(id, nodes.len());
                }
                pins.push((field_f64(&child, "x"), field_f64(&child, "y")));
                nodes.push(GNode {
                    title,
                    inputs,
                    outputs,
                    x: 0.0,
                    y: 0.0,
                    w,
                    h,
                    theme,
                });
            }
            "wf_link" => {
                let from = field_utf8(&child, "from")
                    .or_else(|| label_string(&child))
                    .unwrap_or_default();
                let to = field_utf8(&child, "to").unwrap_or_default();
                let label = field_utf8(&child, "label");
                raw_links.push((from, to, label));
            }
            _ => {}
        }
    }

    // Resolve link endpoints to (node, port) indices; drop unresolvable ones.
    let mut links: Vec<GLink> = Vec::new();
    for (from, to, label) in raw_links {
        let (Some((src, src_port)), Some((dst, dst_port))) = (
            resolve_port(&from, &nodes, &id_to_idx, true),
            resolve_port(&to, &nodes, &id_to_idx, false),
        ) else {
            continue;
        };
        links.push(GLink {
            src,
            src_port,
            dst,
            dst_port,
            label,
        });
    }

    if nodes.is_empty() {
        return sized(Kind::NodeGraph { nodes, links }, 1.0, 1.0, disabled);
    }

    // Auto-layout from the link graph (node index doubles as the layout id, so
    // a node needs no user `id` to be positioned), then pin explicit x/y.
    let lnodes: Vec<crate::layered::Node> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| crate::layered::Node {
            id: Some(i.to_string()),
            size: (n.w, n.h),
        })
        .collect();
    let ledges: Vec<(String, String)> = links
        .iter()
        .map(|l| (l.src.to_string(), l.dst.to_string()))
        .collect();
    let offsets = crate::layered::assign_layered_offsets(
        &lnodes,
        &ledges,
        dir,
        GRAPH_LAYER_GAP,
        GRAPH_NODE_GAP,
    );
    for (i, n) in nodes.iter_mut().enumerate() {
        n.x = pins[i].0.unwrap_or(offsets[i].0);
        n.y = pins[i].1.unwrap_or(offsets[i].1);
    }

    // Normalise so the content starts at GRAPH_PAD, and size to the bbox.
    let min_x = nodes.iter().map(|n| n.x).fold(f64::MAX, f64::min);
    let min_y = nodes.iter().map(|n| n.y).fold(f64::MAX, f64::min);
    for n in nodes.iter_mut() {
        n.x += GRAPH_PAD - min_x;
        n.y += GRAPH_PAD - min_y;
    }
    let w = nodes.iter().map(|n| n.x + n.w).fold(0.0, f64::max) + GRAPH_PAD;
    let h = nodes.iter().map(|n| n.y + n.h).fold(0.0, f64::max) + GRAPH_PAD;
    sized(Kind::NodeGraph { nodes, links }, w, h, disabled)
}

/// The measured `(width, height)` of a node box: a title band over a body of
/// `max(inputs, outputs)` port rows, wide enough for the title and both label
/// columns.
fn node_size(title: &str, inputs: &[String], outputs: &[String]) -> (f64, f64) {
    let rows = inputs.len().max(outputs.len());
    let h = NODE_HEADER_H + rows as f64 * NODE_PORT_ROW_H + 2.0 * NODE_BODY_PAD_Y;
    let title_w = text_w(title, FONT) + 2.0 * NODE_PAD_X;
    let max_in = inputs
        .iter()
        .map(|s| text_w(s, PORT_FONT))
        .fold(0.0, f64::max);
    let max_out = outputs
        .iter()
        .map(|s| text_w(s, PORT_FONT))
        .fold(0.0, f64::max);
    let ports_w = NODE_PAD_X + max_in + PORT_COL_GAP + max_out + NODE_PAD_X;
    (title_w.max(ports_w).max(NODE_MIN_W), h)
}

/// Resolve a link endpoint `"node"` / `"node.port"` to `(node_idx, port_idx)`.
/// `is_output` picks the source's output ports vs the destination's inputs. A
/// bare node name (or empty port) targets the first port; a named port matches
/// by label (case-insensitive), falling back to the sole port if there's one.
/// Returns `None` when the node name is unknown or the port can't be resolved.
fn resolve_port(
    reference: &str,
    nodes: &[GNode],
    id_to_idx: &HashMap<String, usize>,
    is_output: bool,
) -> Option<(usize, usize)> {
    let (node_name, port_name) = match reference.split_once('.') {
        Some((n, p)) => (n.trim(), Some(p.trim())),
        None => (reference.trim(), None),
    };
    let idx = *id_to_idx.get(node_name)?;
    let ports = if is_output {
        &nodes[idx].outputs
    } else {
        &nodes[idx].inputs
    };
    let port = match port_name {
        Some(p) if !p.is_empty() => ports
            .iter()
            .position(|x| x.eq_ignore_ascii_case(p))
            .or(if ports.len() == 1 { Some(0) } else { None })?,
        _ => 0,
    };
    Some((idx, port))
}

/// The `(x, y)` of a node's port marker in the graph's local space — on the
/// right edge for an output, the left edge for an input, centred on the port's
/// row. A node with no declared ports on that side anchors at the edge midpoint.
fn port_point(n: &GNode, idx: usize, is_output: bool) -> (f64, f64) {
    let ports = if is_output { &n.outputs } else { &n.inputs };
    let px = if is_output { n.x + n.w } else { n.x };
    let py = if ports.is_empty() {
        n.y + n.h / 2.0
    } else {
        let i = idx.min(ports.len() - 1) as f64;
        n.y + NODE_HEADER_H + NODE_BODY_PAD_Y + (i + 0.5) * NODE_PORT_ROW_H
    };
    (px, py)
}

fn sized(kind: Kind, w: f64, h: f64, disabled: bool) -> Widget {
    Widget {
        kind,
        w,
        h,
        disabled,
        anchor: None,
        edit: false,
    }
}

/// Build every wireframe child of a container, in source order. `doc` is
/// `None` on the measure-only path (size is theme-independent). Data-driven
/// children expand first: a `wdoc_repeater` / `wdoc_instance` / component
/// instance nested in a container is flattened to its generated blocks
/// (carrying their binding scope), so a screen can compose widgets from data
/// inside a `wf_*` frame. Non-wireframe results are dropped, as before.
///
/// With `patterns` (the render path) each child carries its edit-mode shape
/// anchor, so nested widgets are selectable/draggable like top-level ones.
fn child_widgets(
    doc: Option<&Document>,
    block: &Block<'_>,
    patterns: Option<&InlinePatterns>,
) -> Vec<Widget> {
    expand_container_children(block)
        .into_iter()
        .filter(|b| is_wireframe_kind(b.kind()))
        .map(|b| {
            let mut w = build(doc, &b, patterns);
            if let Some(p) = patterns {
                let attrs = shape_anchor_attrs(&b, p);
                if !attrs.is_empty() {
                    w.anchor = Some(attrs);
                }
            }
            w
        })
        .collect()
}

// ── Container sizing ─────────────────────────────────────────────────

/// A browser frame's `(w, h)`: an explicit `width`/`height` pins that axis,
/// otherwise the default, with the height growing past it to fit `content_h`
/// so the content never clips.
fn browser_size(content_h: f64, width: Option<f64>, height: Option<f64>) -> (f64, f64) {
    let needed = BROWSER_TOOLBAR_H + PAD + content_h + PAD;
    (
        width.unwrap_or(BROWSER_W),
        height.unwrap_or(BROWSER_H.max(needed)),
    )
}

/// A phone/tablet frame's `(w, h)`. Portrait is the default; `landscape` swaps
/// the two axes. As with [`browser_size`], an explicit `width`/`height` pins
/// that axis and the height grows to fit `content_h`.
fn device_size(
    tablet: bool,
    landscape: bool,
    content_h: f64,
    width: Option<f64>,
    height: Option<f64>,
) -> (f64, f64) {
    let (base_w, base_h) = if tablet {
        (TABLET_W, TABLET_H)
    } else {
        (PHONE_W, PHONE_H)
    };
    let (default_w, default_h) = if landscape {
        (base_h, base_w)
    } else {
        (base_w, base_h)
    };
    let needed = DEVICE_BEZEL + STATUS_H + PAD + content_h + PAD + HOME_IND_H + DEVICE_BEZEL;
    (
        width.unwrap_or(default_w),
        height.unwrap_or(default_h.max(needed)),
    )
}

fn column_size(items: &[Widget]) -> (f64, f64) {
    if items.is_empty() {
        return (0.0, 0.0);
    }
    let w = items.iter().map(|c| c.w).fold(0.0, f64::max);
    let h = items.iter().map(|c| c.h).sum::<f64>() + GAP * (items.len() - 1) as f64;
    (w, h)
}

fn row_size(items: &[Widget]) -> (f64, f64) {
    if items.is_empty() {
        return (0.0, 0.0);
    }
    let w = items.iter().map(|c| c.w).sum::<f64>() + ROW_GAP * (items.len() - 1) as f64;
    let h = items.iter().map(|c| c.h).fold(0.0, f64::max);
    (w, h)
}

/// A grid's cell geometry: the uniform column width plus each row's
/// `(y offset, height)`, shared by sizing, emission and the edit-mode cell
/// guides so they can't disagree. An empty grid gets the placeholder cells
/// (two rows of empty slots — see the `wf_row` build arm's note on why the
/// placeholder is unconditional).
fn grid_geometry(items: &[Widget], cols: usize) -> (f64, Vec<(f64, f64)>) {
    if items.is_empty() {
        return (
            EMPTY_CELL_W,
            vec![(0.0, EMPTY_CELL_H), (EMPTY_CELL_H + GAP, EMPTY_CELL_H)],
        );
    }
    let col_w = items.iter().map(|c| c.w).fold(0.0, f64::max);
    let mut rows = Vec::new();
    let mut cy = 0.0;
    for chunk in items.chunks(cols) {
        let row_h = chunk.iter().map(|c| c.h).fold(0.0, f64::max);
        rows.push((cy, row_h));
        cy += row_h + GAP;
    }
    (col_w, rows)
}

fn grid_size(items: &[Widget], cols: usize) -> (f64, f64) {
    let (col_w, rows) = grid_geometry(items, cols);
    let (last_y, last_h) = *rows.last().expect("grid geometry always has a row");
    (
        col_w * cols as f64 + GAP * (cols - 1) as f64,
        last_y + last_h,
    )
}

// ── Emission ─────────────────────────────────────────────────────────

/// Emit a widget's SVG at absolute top-left `(x, y)`. Neutral colours come
/// from the resolved theme `roles`; a widget's own `class` (`theme`) overrides
/// box fill / text colour / border.
fn emit(w: &Widget, x: f64, y: f64, roles: &ThemeRoles, out: &mut String) {
    // A nested child's edit-mode anchor: a transform-less group (the content
    // is drawn at absolute offsets already) carrying the `data-wcl-shape`
    // attrs, so the editor can hit-test, select and drag it.
    if let Some(a) = &w.anchor {
        out.push_str("<g");
        out.push_str(a);
        out.push('>');
    }
    if w.disabled {
        out.push_str("<g opacity=\"0.45\">");
    }
    match &w.kind {
        Kind::Empty => {}
        Kind::Label { text, theme } => {
            emit_text(
                out,
                x + 1.0,
                baseline(y, LINE_H),
                "start",
                FONT,
                false,
                fg_of(theme, roles),
                None,
                text,
            );
        }
        Kind::Button { text, theme } => {
            let fill = theme.bg.as_deref().unwrap_or(&roles.overlay);
            rect(out, x, y, w.w, CTRL_H, RADIUS, fill, border(theme, roles));
            // A leading glyph would need the icon sprite (which doesn't survive
            // the PDF embed / `currentColor` baking), so the label is centred —
            // the mock-up reads fine without it.
            emit_text(
                out,
                x + w.w / 2.0,
                baseline(y, CTRL_H),
                "middle",
                FONT,
                false,
                fg_of(theme, roles),
                None,
                text,
            );
        }
        Kind::Input {
            text,
            placeholder,
            theme,
        } => {
            let fill = theme.bg.as_deref().unwrap_or(&roles.bg_inset);
            rect(out, x, y, w.w, CTRL_H, RADIUS, fill, border(theme, roles));
            // Placeholder text is muted + italic; a filled value uses the fg.
            let color = if *placeholder {
                &roles.fg_muted
            } else {
                fg_of(theme, roles)
            };
            emit_text(
                out,
                x + CTRL_PAD_X,
                baseline(y, CTRL_H),
                "start",
                FONT,
                *placeholder,
                color,
                None,
                text,
            );
        }
        Kind::Dropdown { text, theme } => {
            let fill = theme.bg.as_deref().unwrap_or(&roles.bg_inset);
            rect(out, x, y, w.w, CTRL_H, RADIUS, fill, border(theme, roles));
            emit_text(
                out,
                x + CTRL_PAD_X,
                baseline(y, CTRL_H),
                "start",
                FONT,
                false,
                fg_of(theme, roles),
                None,
                text,
            );
            // Down chevron, drawn natively (no icon sprite).
            let cx0 = x + w.w - CTRL_PAD_X - ICON;
            let iy = y + (CTRL_H - ICON) / 2.0;
            polyline(
                out,
                &[
                    (cx0 + 0.2 * ICON, iy + 0.40 * ICON),
                    (cx0 + 0.5 * ICON, iy + 0.64 * ICON),
                    (cx0 + 0.8 * ICON, iy + 0.40 * ICON),
                ],
                &roles.fg_muted,
                1.5,
            );
        }
        Kind::Checkbox { label, on, theme } => {
            let by = y + (w.h - BOX) / 2.0;
            let fill = if *on { &roles.accent } else { &roles.bg_inset };
            rect(out, x, by, BOX, BOX, 3.0, fill, &roles.border);
            if *on {
                // A check mark, drawn natively in the page bg so it reads on
                // the accent fill.
                polyline(
                    out,
                    &[
                        (x + 0.26 * BOX, by + 0.52 * BOX),
                        (x + 0.44 * BOX, by + 0.70 * BOX),
                        (x + 0.74 * BOX, by + 0.32 * BOX),
                    ],
                    &roles.bg,
                    1.8,
                );
            }
            emit_text(
                out,
                x + BOX + 7.0,
                baseline(y, w.h),
                "start",
                FONT,
                false,
                fg_of(theme, roles),
                None,
                label,
            );
        }
        Kind::Radio { label, on, theme } => {
            let cx = x + DOT / 2.0;
            let cy = y + w.h / 2.0;
            circle(out, cx, cy, DOT / 2.0, &roles.bg_inset, &roles.border);
            if *on {
                circle(out, cx, cy, DOT / 2.0 - 3.0, &roles.accent, "none");
            }
            emit_text(
                out,
                x + DOT + 7.0,
                baseline(y, w.h),
                "start",
                FONT,
                false,
                fg_of(theme, roles),
                None,
                label,
            );
        }
        Kind::Toggle { label, on, theme } => {
            let ty = y + (w.h - TRACK_H) / 2.0;
            let fill = if *on { &roles.accent } else { &roles.bg_inset };
            rect(
                out,
                x,
                ty,
                TRACK_W,
                TRACK_H,
                TRACK_H / 2.0,
                fill,
                border(theme, roles),
            );
            let r = (TRACK_H - 2.0) / 2.0;
            let kx = if *on {
                x + TRACK_W - 1.0 - r
            } else {
                x + 1.0 + r
            };
            circle(out, kx, ty + TRACK_H / 2.0, r, &roles.fg, "none");
            if let Some(l) = label {
                emit_text(
                    out,
                    x + TRACK_W + 7.0,
                    baseline(y, w.h),
                    "start",
                    FONT,
                    false,
                    fg_of(theme, roles),
                    None,
                    l,
                );
            }
        }
        Kind::Window {
            title,
            controls,
            body,
            theme,
        } => {
            let bg = theme.bg.as_deref().unwrap_or(&roles.bg_alt);
            rect(out, x, y, w.w, w.h, 6.0, bg, border(theme, roles));
            // Titlebar.
            out.push_str(&format!(
                "<path d=\"M{x:.2} {ty:.2} v{rad} a6 6 0 0 1 6 -6 h{hw:.2} a6 6 0 0 1 6 6 v{rest:.2} h-{tw:.2} z\" fill=\"{tbar}\"/>",
                ty = y + TITLEBAR_H,
                rad = -(TITLEBAR_H - 6.0),
                hw = w.w - 12.0,
                rest = TITLEBAR_H - 6.0,
                tw = w.w,
                tbar = roles.overlay,
            ));
            emit_text(
                out,
                x + PAD,
                baseline(y, TITLEBAR_H),
                "start",
                TITLE_FONT,
                false,
                fg_of(theme, roles),
                None,
                title,
            );
            if *controls {
                emit_window_controls(out, x + w.w, y, roles);
            }
            // Body column.
            emit_column(body, x + PAD, y + TITLEBAR_H + PAD, roles, out);
        }
        Kind::Browser { url, body, theme } => {
            let bg = theme.bg.as_deref().unwrap_or(&roles.bg_alt);
            rect(out, x, y, w.w, w.h, 8.0, bg, border(theme, roles));
            // Toolbar strip with rounded top corners (same path trick as the
            // window titlebar, radius 8).
            out.push_str(&format!(
                "<path d=\"M{x:.2} {ty:.2} v{rad:.2} a8 8 0 0 1 8 -8 h{hw:.2} a8 8 0 0 1 8 8 v{rest:.2} h-{tw:.2} z\" fill=\"{tbar}\"/>",
                ty = y + BROWSER_TOOLBAR_H,
                rad = -(BROWSER_TOOLBAR_H - 8.0),
                hw = w.w - 16.0,
                rest = BROWSER_TOOLBAR_H - 8.0,
                tw = w.w,
                tbar = roles.overlay,
            ));
            // Three traffic-light dots, top-right of the toolbar (rightmost
            // dot pinned to the right inset, the rest stepping left).
            for i in 0..3 {
                let dx = x + w.w - PAD - 5.0 - (2 - i) as f64 * 13.0;
                circle(out, dx, y + 16.0, 4.0, "none", &roles.fg_muted);
            }
            // Address-bar pill below the dots, showing the URL.
            let bar_y = y + 30.0;
            let bar_h = 22.0;
            let fill = theme.bg.as_deref().unwrap_or(&roles.bg_inset);
            rect(
                out,
                x + PAD,
                bar_y,
                w.w - 2.0 * PAD,
                bar_h,
                bar_h / 2.0,
                fill,
                &roles.border,
            );
            emit_text(
                out,
                x + PAD + 12.0,
                baseline(bar_y, bar_h),
                "start",
                FONT,
                false,
                &roles.fg_muted,
                None,
                url,
            );
            // Content area.
            emit_column(body, x + PAD, y + BROWSER_TOOLBAR_H + PAD, roles, out);
        }
        Kind::Device {
            tablet,
            title,
            body,
            theme,
        } => {
            let radius = if *tablet { 22.0 } else { 30.0 };
            let frame = theme.bg.as_deref().unwrap_or(&roles.bg_alt);
            // Outer bezel frame.
            rect(out, x, y, w.w, w.h, radius, frame, border(theme, roles));
            // Inner screen.
            let sx = x + DEVICE_BEZEL;
            let sy = y + DEVICE_BEZEL;
            let sw = w.w - 2.0 * DEVICE_BEZEL;
            let sh = w.h - 2.0 * DEVICE_BEZEL;
            rect(
                out,
                sx,
                sy,
                sw,
                sh,
                (radius - 8.0).max(4.0),
                &roles.bg,
                "none",
            );
            // Status bar: a centred notch (phone) / camera dot (tablet), an
            // optional title on the left, and a battery glyph on the right.
            if *tablet {
                circle(
                    out,
                    x + w.w / 2.0,
                    sy + STATUS_H / 2.0,
                    3.0,
                    "none",
                    &roles.fg_muted,
                );
            } else {
                let notch_w = 90.0_f64.min(sw - 24.0);
                rect(
                    out,
                    x + w.w / 2.0 - notch_w / 2.0,
                    sy + 6.0,
                    notch_w,
                    7.0,
                    3.5,
                    &roles.overlay,
                    "none",
                );
            }
            if let Some(t) = title {
                emit_text(
                    out,
                    sx + PAD,
                    baseline(sy, STATUS_H),
                    "start",
                    FONT,
                    false,
                    &roles.fg_muted,
                    None,
                    t,
                );
            }
            // Battery glyph, right-aligned in the status bar.
            let by = sy + (STATUS_H - 11.0) / 2.0;
            let bx = x + w.w - DEVICE_BEZEL - PAD - 22.0;
            rect(out, bx, by, 20.0, 11.0, 2.5, "none", &roles.fg_muted);
            rect(
                out,
                bx + 2.0,
                by + 2.0,
                12.0,
                7.0,
                1.0,
                &roles.fg_muted,
                "none",
            );
            rect(
                out,
                bx + 20.0,
                by + 3.5,
                2.0,
                4.0,
                1.0,
                &roles.fg_muted,
                "none",
            );
            // Home-indicator pill near the bottom of the screen.
            let pill_w = (sw * 0.34).min(150.0);
            rect(
                out,
                x + w.w / 2.0 - pill_w / 2.0,
                y + w.h - DEVICE_BEZEL - 12.0,
                pill_w,
                5.0,
                2.5,
                &roles.fg_muted,
                "none",
            );
            // Screen content.
            emit_column(body, sx + PAD, sy + STATUS_H + PAD, roles, out);
        }
        Kind::Panel { title, body, theme } => {
            rect(
                out,
                x,
                y,
                w.w,
                w.h,
                RADIUS + 1.0,
                "none",
                border(theme, roles),
            );
            let mut cy = y + PANEL_PAD;
            if let Some(t) = title {
                emit_text(
                    out,
                    x + PANEL_PAD,
                    baseline(cy, LINE_H),
                    "start",
                    FONT,
                    false,
                    &roles.fg_muted,
                    None,
                    t,
                );
                cy += LINE_H + 4.0;
            }
            emit_column(body, x + PANEL_PAD, cy, roles, out);
        }
        Kind::Row(items) => {
            if w.edit {
                emit_row_guides(w, items, x, y, roles, out);
            }
            let mut cx = x;
            for c in items {
                emit(c, cx, y + (w.h - c.h) / 2.0, roles, out);
                cx += c.w + ROW_GAP;
            }
        }
        Kind::Column(items) => {
            if w.edit {
                emit_column_guides(w, items, x, y, roles, out);
            }
            emit_column(items, x, y, roles, out);
        }
        Kind::Grid { cols, items } => {
            if w.edit {
                emit_grid_guides(w, items, *cols, x, y, roles, out);
            }
            let (col_w, rows) = grid_geometry(items, *cols);
            for (r, chunk) in items.chunks(*cols).enumerate() {
                for (i, c) in chunk.iter().enumerate() {
                    emit(c, x + i as f64 * (col_w + GAP), y + rows[r].0, roles, out);
                }
            }
        }
        Kind::NodeGraph { nodes, links } => emit_node_graph(nodes, links, x, y, w, roles, out),
    }
    if w.disabled {
        out.push_str("</g>");
    }
    if w.anchor.is_some() {
        out.push_str("</g>");
    }
}

/// Emit a node graph: route every link under the nodes (so the wires sit
/// behind the boxes), then draw the node boxes with their ports on top.
fn emit_node_graph(
    nodes: &[GNode],
    links: &[GLink],
    ox: f64,
    oy: f64,
    w: &Widget,
    roles: &ThemeRoles,
    out: &mut String,
) {
    // Node boxes as routing obstacles in absolute (emitted) coordinates.
    let obstacles: Vec<crate::routing::Obstacle> = nodes
        .iter()
        .map(|n| crate::routing::Obstacle {
            x: ox + n.x,
            y: oy + n.y,
            w: n.w,
            h: n.h,
        })
        .collect();
    let viewport = (ox + w.w, oy + w.h);
    for l in links {
        let (sx, sy) = port_point(&nodes[l.src], l.src_port, true);
        let (dx, dy) = port_point(&nodes[l.dst], l.dst_port, false);
        let src = (ox + sx, oy + sy);
        let dst = (ox + dx, oy + dy);
        // Route around every node except the two this link connects.
        let obs: Vec<crate::routing::Obstacle> = obstacles
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != l.src && *i != l.dst)
            .map(|(_, o)| *o)
            .collect();
        let pts = crate::routing::route_elbow(
            src,
            crate::routing::Side::East,
            dst,
            crate::routing::Side::West,
            &obs,
            &[],
            viewport,
        )
        .unwrap_or_else(|| vec![src, dst]);
        polyline(out, &pts, &roles.fg_muted, 1.5);
        if let Some(label) = &l.label {
            let mid = pts.len() / 2;
            let (lx, ly) = pts[mid];
            emit_text(
                out,
                lx,
                ly - 4.0,
                "middle",
                PORT_FONT,
                false,
                &roles.fg_muted,
                None,
                label,
            );
        }
    }
    for n in nodes {
        emit_node(out, ox, oy, n, roles);
    }
}

/// Draw one node box: a body rect, a title band + separator, and the input /
/// output port markers with their labels.
fn emit_node(out: &mut String, ox: f64, oy: f64, n: &GNode, roles: &ThemeRoles) {
    let x = ox + n.x;
    let y = oy + n.y;
    let brd = border(&n.theme, roles);
    let body_fill = n.theme.bg.as_deref().unwrap_or(&roles.bg_alt);
    rect(out, x, y, n.w, n.h, RADIUS, body_fill, brd);
    // Title band + separator under it.
    rect(
        out,
        x,
        y,
        n.w,
        NODE_HEADER_H,
        RADIUS,
        &roles.overlay,
        "none",
    );
    let sy = y + NODE_HEADER_H;
    polyline(out, &[(x, sy), (x + n.w, sy)], brd, 1.0);
    emit_text(
        out,
        x + NODE_PAD_X,
        baseline(y, NODE_HEADER_H),
        "start",
        FONT,
        false,
        fg_of(&n.theme, roles),
        None,
        &n.title,
    );
    // Ports: inputs down the left edge, outputs down the right edge.
    let fg = fg_of(&n.theme, roles);
    let row_y = |i: usize| y + NODE_HEADER_H + NODE_BODY_PAD_Y + (i as f64 + 0.5) * NODE_PORT_ROW_H;
    for (i, label) in n.inputs.iter().enumerate() {
        let py = row_y(i);
        circle(out, x, py, NODE_PORT_R, &roles.accent, brd);
        emit_text(
            out,
            x + NODE_PAD_X,
            py + PORT_FONT * 0.34,
            "start",
            PORT_FONT,
            false,
            fg,
            None,
            label,
        );
    }
    for (j, label) in n.outputs.iter().enumerate() {
        let py = row_y(j);
        circle(out, x + n.w, py, NODE_PORT_R, &roles.accent, brd);
        emit_text(
            out,
            x + n.w - NODE_PAD_X,
            py + PORT_FONT * 0.34,
            "end",
            PORT_FONT,
            false,
            fg,
            None,
            label,
        );
    }
}

/// Emit a vertical stack of widgets (window/panel body, `wf_column`).
fn emit_column(items: &[Widget], x: f64, y: f64, roles: &ThemeRoles, out: &mut String) {
    let mut cy = y;
    for c in items {
        emit(c, x, cy, roles, out);
        cy += c.h + GAP;
    }
}

// ── Edit-mode layout guides ──────────────────────────────────────────
// Chrome for the otherwise-invisible layout containers (`wf_row` /
// `wf_column` / `wf_grid`), drawn UNDER the children (which are emitted
// after, so they stay on top for hit-testing and the editor's drill-down
// selection). One `<g data-wf-guide="1">` per container holding: a full-box
// backing rect (`pointer-events="all"` — the whole area click-selects the
// container and accepts drops, not just its children's pixels), a dashed
// boundary, a kind tag on empty containers, and `data-wf-slot` zones the
// editor turns into insert-at-position drops (grid cells; the gaps of a
// row / column). Only emitted on edit-mode builds — published output is
// untouched.

/// Open the guide group: backing rect, dashed boundary, and — when `tag` is
/// non-empty — the kind label tucked into the top-left corner. The caller
/// closes the `</g>`.
fn guide_open(w: &Widget, x: f64, y: f64, tag: &str, roles: &ThemeRoles, out: &mut String) {
    let muted = &roles.fg_muted;
    out.push_str("<g data-wf-guide=\"1\">");
    out.push_str(&format!(
        "<rect x=\"{x:.2}\" y=\"{y:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"{RADIUS}\" \
         fill=\"none\" pointer-events=\"all\"/>",
        w.w, w.h
    ));
    out.push_str(&format!(
        "<rect x=\"{x:.2}\" y=\"{y:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"{RADIUS}\" \
         fill=\"none\" stroke=\"{muted}\" stroke-width=\"1\" stroke-dasharray=\"4 3\" \
         opacity=\"0.5\" pointer-events=\"none\"/>",
        w.w, w.h
    ));
    if !tag.is_empty() {
        out.push_str(&format!(
            "<text x=\"{:.2}\" y=\"{:.2}\" font-family=\"{SANS}\" font-size=\"{GUIDE_FONT}\" \
             fill=\"{muted}\" opacity=\"0.75\" pointer-events=\"none\">{}</text>",
            x + 4.0,
            y + GUIDE_FONT + 3.0,
            escape_html(tag),
        ));
    }
}

/// One `data-wf-slot` drop zone. A visible cell passes `dashed` (grid cells,
/// empty placeholder slots); a bare insertion strip (row/column gaps) stays
/// invisible — either way `pointer-events="all"` makes it hit-testable.
fn guide_slot(out: &mut String, slot: usize, x: f64, y: f64, w: f64, h: f64, dashed: Option<&str>) {
    let stroke = dashed
        .map(|c| {
            format!(" stroke=\"{c}\" stroke-width=\"1\" stroke-dasharray=\"3 3\" opacity=\"0.35\"")
        })
        .unwrap_or_default();
    out.push_str(&format!(
        "<rect data-wf-slot=\"{slot}\" x=\"{x:.2}\" y=\"{y:.2}\" width=\"{w:.2}\" \
         height=\"{h:.2}\" fill=\"none\" pointer-events=\"all\"{stroke}/>",
    ));
}

fn emit_row_guides(
    w: &Widget,
    items: &[Widget],
    x: f64,
    y: f64,
    roles: &ThemeRoles,
    out: &mut String,
) {
    guide_open(
        w,
        x,
        y,
        if items.is_empty() { "row" } else { "" },
        roles,
        out,
    );
    if items.is_empty() {
        guide_slot(out, 0, x, y, EMPTY_CELL_W, w.h, Some(&roles.fg_muted));
        guide_slot(
            out,
            1,
            x + EMPTY_CELL_W + ROW_GAP,
            y,
            EMPTY_CELL_W,
            w.h,
            Some(&roles.fg_muted),
        );
    } else {
        // An insertion strip over each inter-child gap: a drop between child
        // i-1 and i inserts at position i (drops on a child insert after it,
        // and the trailing space appends via the backing rect).
        let mut cx = x;
        for (i, c) in items.iter().enumerate() {
            if i > 0 {
                guide_slot(out, i, cx - ROW_GAP, y, ROW_GAP, w.h, None);
            }
            cx += c.w + ROW_GAP;
        }
    }
    out.push_str("</g>");
}

fn emit_column_guides(
    w: &Widget,
    items: &[Widget],
    x: f64,
    y: f64,
    roles: &ThemeRoles,
    out: &mut String,
) {
    let tag = if items.is_empty() { "column" } else { "" };
    guide_open(w, x, y, tag, roles, out);
    if items.is_empty() {
        guide_slot(out, 0, x, y, w.w, EMPTY_CELL_H, Some(&roles.fg_muted));
        guide_slot(
            out,
            1,
            x,
            y + EMPTY_CELL_H + GAP,
            w.w,
            EMPTY_CELL_H,
            Some(&roles.fg_muted),
        );
    } else {
        let mut cy = y;
        for (i, c) in items.iter().enumerate() {
            if i > 0 {
                guide_slot(out, i, x, cy - GAP, w.w, GAP, None);
            }
            cy += c.h + GAP;
        }
    }
    out.push_str("</g>");
}

fn emit_grid_guides(
    w: &Widget,
    items: &[Widget],
    cols: usize,
    x: f64,
    y: f64,
    roles: &ThemeRoles,
    out: &mut String,
) {
    let tag = if items.is_empty() {
        format!("grid ·{cols}")
    } else {
        String::new()
    };
    guide_open(w, x, y, &tag, roles, out);
    // Every cell of every row — including the trailing empties of a partial
    // last row (their slots run past the child count, so a drop there simply
    // appends) and the placeholder cells of an empty grid.
    let (col_w, rows) = grid_geometry(items, cols);
    for (r, (row_y, row_h)) in rows.iter().enumerate() {
        for c in 0..cols {
            guide_slot(
                out,
                r * cols + c,
                x + c as f64 * (col_w + GAP),
                y + row_y,
                col_w,
                *row_h,
                Some(&roles.fg_muted),
            );
        }
    }
    out.push_str("</g>");
}

/// Titlebar dots + close `✕`, right-aligned at `right_x`.
fn emit_window_controls(out: &mut String, right_x: f64, y: f64, roles: &ThemeRoles) {
    let cy = y + TITLEBAR_H / 2.0;
    let s = 4.0;
    let cx = right_x - PAD - s;
    let muted = &roles.fg_muted;
    out.push_str(&format!(
        "<g stroke=\"{muted}\" stroke-width=\"1.5\" stroke-linecap=\"round\">\
         <line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\"/>\
         <line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\"/></g>",
        cx - s,
        cy - s,
        cx + s,
        cy + s,
        cx - s,
        cy + s,
        cx + s,
        cy - s,
    ));
    // Two outline dots left of the close glyph.
    for i in 0..2 {
        let dx = cx - 4.0 * s - (1 - i) as f64 * 13.0;
        out.push_str(&format!(
            "<circle cx=\"{dx:.2}\" cy=\"{cy:.2}\" r=\"4\" fill=\"none\" stroke=\"{muted}\"/>",
        ));
    }
}

// ── Primitive emitters ───────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn rect(out: &mut String, x: f64, y: f64, w: f64, h: f64, rx: f64, fill: &str, stroke: &str) {
    let stroke_attr = if stroke == "none" {
        String::new()
    } else {
        format!(" stroke=\"{stroke}\" stroke-width=\"1\"")
    };
    out.push_str(&format!(
        "<rect x=\"{x:.2}\" y=\"{y:.2}\" width=\"{w:.2}\" height=\"{h:.2}\" rx=\"{rx}\" fill=\"{fill}\"{stroke_attr}/>",
    ));
}

fn circle(out: &mut String, cx: f64, cy: f64, r: f64, fill: &str, stroke: &str) {
    let stroke_attr = if stroke == "none" {
        String::new()
    } else {
        format!(" stroke=\"{stroke}\" stroke-width=\"1\"")
    };
    out.push_str(&format!(
        "<circle cx=\"{cx:.2}\" cy=\"{cy:.2}\" r=\"{r:.2}\" fill=\"{fill}\"{stroke_attr}/>",
    ));
}

/// A stroked, unfilled open polyline (the chevron / check glyphs).
fn polyline(out: &mut String, points: &[(f64, f64)], stroke: &str, width: f64) {
    let pts = points
        .iter()
        .map(|(x, y)| format!("{x:.2},{y:.2}"))
        .collect::<Vec<_>>()
        .join(" ");
    out.push_str(&format!(
        "<polyline points=\"{pts}\" fill=\"none\" stroke=\"{stroke}\" stroke-width=\"{width}\" \
         stroke-linecap=\"round\" stroke-linejoin=\"round\"/>",
    ));
}

#[allow(clippy::too_many_arguments)]
fn emit_text(
    out: &mut String,
    x: f64,
    y: f64,
    anchor: &str,
    font: f64,
    italic: bool,
    fill: &str,
    opacity: Option<f64>,
    text: &str,
) {
    if text.is_empty() {
        return;
    }
    let style = if italic { " font-style=\"italic\"" } else { "" };
    let op = opacity
        .map(|o| format!(" fill-opacity=\"{o}\""))
        .unwrap_or_default();
    out.push_str(&format!(
        "<text x=\"{x:.2}\" y=\"{y:.2}\" text-anchor=\"{anchor}\" font-family=\"{SANS}\" \
         font-size=\"{font}\" fill=\"{fill}\"{op}{style}>{}</text>",
        escape_html(text),
    ));
}

/// The text baseline for a glyph vertically centred in a box of height `h`
/// whose top is at `top`.
fn baseline(top: f64, h: f64) -> f64 {
    top + h / 2.0 + FONT * 0.34
}

// ── Theme + colour helpers ───────────────────────────────────────────

/// A widget's baked theme from its `class` list (first class that sets each
/// field wins). Empty when no class supplies an override.
fn theme_of(doc: &Document, block: &Block<'_>) -> Theme {
    let classes = field_utf8_list(block, "class");
    Theme {
        bg: class_field(doc, &classes, "background"),
        fg: class_field(doc, &classes, "color"),
        border: class_field(doc, &classes, "border").and_then(|s| border_color(&s)),
    }
}

/// The first referenced `class` block that sets `field`, returned verbatim.
fn class_field(doc: &Document, classes: &[String], field: &str) -> Option<String> {
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

/// The text fill for a widget — its class `color` override, else the theme's
/// `fg` role.
fn fg_of<'a>(theme: &'a Theme, roles: &'a ThemeRoles) -> &'a str {
    theme.fg.as_deref().unwrap_or(&roles.fg)
}

/// A box border colour: the class `border`'s colour if set, else the theme's
/// `border` role.
fn border<'a>(theme: &'a Theme, roles: &'a ThemeRoles) -> &'a str {
    theme.border.as_deref().unwrap_or(&roles.border)
}

/// The colour token from a CSS `border` shorthand (`"1px solid #1f6feb"` →
/// `"#1f6feb"`): the last whitespace-separated token.
fn border_color(s: &str) -> Option<String> {
    s.split_whitespace().last().map(str::to_string)
}

/// A rough average glyph advance (in em) so box sizing fits mock-up text
/// without a font system. Overestimates slightly so boxes never clip.
fn char_em(c: char) -> f64 {
    match c {
        'i' | 'l' | 'j' | 'I' | '.' | ',' | ':' | ';' | '\'' | '|' | '!' | '`' => 0.30,
        'f' | 't' | 'r' | '(' | ')' | '[' | ']' | ' ' => 0.36,
        'm' | 'w' | 'M' | 'W' | '@' => 0.85,
        'A'..='Z' | '0'..='9' => 0.62,
        _ => 0.52,
    }
}

fn text_w(s: &str, font_px: f64) -> f64 {
    s.chars().map(char_em).sum::<f64>() * font_px
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_w_positive_and_monotonic() {
        assert!(text_w("a", FONT) > 0.0);
        assert!(text_w("aa", FONT) > text_w("a", FONT));
        assert!(text_w("", FONT) == 0.0);
    }

    #[test]
    fn border_color_takes_last_token() {
        assert_eq!(
            border_color("1px solid #1f6feb").as_deref(),
            Some("#1f6feb")
        );
        assert_eq!(border_color("red").as_deref(), Some("red"));
    }

    #[test]
    fn device_defaults_and_orientation() {
        // Small content → the default fixed phone frame, portrait.
        assert_eq!(
            device_size(false, false, 40.0, None, None),
            (PHONE_W, PHONE_H)
        );
        // Landscape swaps the two axes.
        assert_eq!(
            device_size(false, true, 40.0, None, None),
            (PHONE_H, PHONE_W)
        );
        // Tablet uses the larger default frame.
        assert_eq!(
            device_size(true, false, 40.0, None, None),
            (TABLET_W, TABLET_H)
        );
    }

    #[test]
    fn device_height_grows_past_default_to_fit_content() {
        // Content taller than the screen makes the frame grow (never clips),
        // while the width stays at the fixed device default.
        let (w, h) = device_size(false, false, 2000.0, None, None);
        assert_eq!(w, PHONE_W);
        assert!(
            h > PHONE_H,
            "phone height should grow past {PHONE_H}, got {h}"
        );
    }

    #[test]
    fn device_explicit_dims_pin_each_axis() {
        // An explicit width/height pins that axis independently.
        assert_eq!(
            device_size(false, false, 40.0, Some(360.0), None),
            (360.0, PHONE_H)
        );
        assert_eq!(
            device_size(false, false, 40.0, None, Some(900.0)),
            (PHONE_W, 900.0)
        );
    }

    #[test]
    fn browser_defaults_and_growth() {
        assert_eq!(browser_size(40.0, None, None), (BROWSER_W, BROWSER_H));
        let (w, h) = browser_size(2000.0, None, None);
        assert_eq!(w, BROWSER_W);
        assert!(
            h > BROWSER_H,
            "browser height should grow past {BROWSER_H}, got {h}"
        );
        // Explicit height pins the axis.
        assert_eq!(browser_size(40.0, None, Some(300.0)), (BROWSER_W, 300.0));
    }

    fn gnode(inputs: &[&str], outputs: &[&str]) -> GNode {
        GNode {
            title: "n".into(),
            inputs: inputs.iter().map(|s| s.to_string()).collect(),
            outputs: outputs.iter().map(|s| s.to_string()).collect(),
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 80.0,
            theme: Theme::default(),
        }
    }

    #[test]
    fn node_size_positive_and_grows_with_content() {
        let (w0, h0) = node_size("A", &[], &[]);
        assert!(w0 >= NODE_MIN_W && h0 > NODE_HEADER_H);
        // A longer title widens the box.
        let (w1, _) = node_size("A considerably longer node title", &[], &[]);
        assert!(w1 > w0);
        // Ports add body height; more rows make it taller still.
        let (_, h1) = node_size("A", &["in".into()], &["out".into()]);
        let (_, h2) = node_size("A", &["a".into(), "b".into()], &["c".into()]);
        assert!(h1 > h0 && h2 > h1);
    }

    #[test]
    fn resolve_port_named_bare_and_unknown() {
        let nodes = vec![
            gnode(&["A", "B"], &["RGB", "Alpha"]),
            gnode(&["Color"], &[]),
        ];
        let map = HashMap::from([("tex".to_string(), 0), ("out".to_string(), 1)]);
        // Named output port, matched case-insensitively.
        assert_eq!(resolve_port("tex.alpha", &nodes, &map, true), Some((0, 1)));
        // Named input port.
        assert_eq!(resolve_port("tex.B", &nodes, &map, false), Some((0, 1)));
        // A bare node name targets its first port.
        assert_eq!(resolve_port("tex", &nodes, &map, true), Some((0, 0)));
        // Single-port node: an unmatched port name falls back to the sole port.
        assert_eq!(
            resolve_port("out.whatever", &nodes, &map, false),
            Some((1, 0))
        );
        // Unknown node, and an unknown port on a multi-port node, both drop.
        assert_eq!(resolve_port("nope.x", &nodes, &map, true), None);
        assert_eq!(resolve_port("tex.zzz", &nodes, &map, true), None);
    }

    #[test]
    fn port_point_sits_on_correct_edge() {
        let mut n = gnode(&["A", "B"], &["Out"]);
        n.x = 10.0;
        n.y = 20.0;
        // Inputs hug the left edge, outputs the right edge, below the header.
        assert_eq!(port_point(&n, 0, false).0, 10.0);
        assert_eq!(port_point(&n, 0, true).0, 110.0);
        assert!(port_point(&n, 0, false).1 > n.y + NODE_HEADER_H);
        // A side with no declared ports anchors at the edge's vertical middle.
        let mut m = gnode(&[], &[]);
        m.h = 80.0;
        assert_eq!(port_point(&m, 0, false).1, 40.0);
    }

    #[test]
    fn grid_dimensions_round_up_rows() {
        let items: Vec<Widget> = (0..5)
            .map(|_| sized(Kind::Empty, 40.0, 20.0, false))
            .collect();
        // 5 items / 2 cols → 3 rows.
        let (w, h) = grid_size(&items, 2);
        assert_eq!(w, 40.0 * 2.0 + GAP);
        assert_eq!(h, 20.0 * 3.0 + GAP * 2.0);
    }
}
