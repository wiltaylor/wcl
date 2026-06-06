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

use wcl_lang::{Block, Document};

use crate::render::{
    RenderCtx, ThemeRoles, escape_html, field_bool, field_f64, field_i64, field_symbol, field_utf8,
    field_utf8_list, label_string, resolve_rect_box, resolve_roles,
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
    let w = build(Some(ctx.doc), block);
    let mut body = String::new();
    emit(&w, 0.0, 0.0, &roles, &mut body);
    format!("<g transform=\"translate({x:.2} {y:.2})\">{body}</g>")
}

/// The measured `(width, height)` of a wireframe widget — `doc`-free, because
/// a widget's size depends only on its text content + structure, not its theme.
/// Used by the diagram layout solvers (`effective_dims`) and the collect pass.
pub(crate) fn measured_size(block: &Block<'_>) -> (f64, f64) {
    let w = build(None, block);
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
    /// An unknown / unsupported child — rendered as nothing (zero size).
    Empty,
}

// ── Build (read fields + measure, bottom-up) ─────────────────────────

fn build(doc: Option<&Document>, block: &Block<'_>) -> Widget {
    let disabled = field_bool(block, "disabled").unwrap_or(false);
    // Theme feeds emission, not size — the measure-only path passes `None`.
    let theme = doc.map(|d| theme_of(d, block)).unwrap_or_default();
    let kind = block.kind();
    match kind {
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
            let body = child_widgets(doc, block);
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
            let body = child_widgets(doc, block);
            let (_, bh) = column_size(&body);
            let (w, h) = browser_size(bh, field_f64(block, "width"), field_f64(block, "height"));
            sized(Kind::Browser { url, body, theme }, w, h, disabled)
        }
        "wf_phone" | "wf_tablet" => {
            let tablet = kind == "wf_tablet";
            let landscape = field_symbol(block, "orientation").as_deref() == Some("landscape");
            let title = label_string(block);
            let body = child_widgets(doc, block);
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
            let body = child_widgets(doc, block);
            let (bw, bh) = column_size(&body);
            let head_h = if title.is_some() { LINE_H + 4.0 } else { 0.0 };
            let head_w = title.as_deref().map_or(0.0, |t| text_w(t, FONT) + 12.0);
            let w = bw.max(head_w) + 2.0 * PANEL_PAD;
            let h = head_h + bh + 2.0 * PANEL_PAD;
            sized(Kind::Panel { title, body, theme }, w, h, disabled)
        }
        "wf_row" => {
            let items = child_widgets(doc, block);
            let (w, h) = row_size(&items);
            sized(Kind::Row(items), w, h, disabled)
        }
        "wf_column" => {
            let items = child_widgets(doc, block);
            let (w, h) = column_size(&items);
            sized(Kind::Column(items), w, h, disabled)
        }
        "wf_grid" => {
            let cols = field_i64(block, "columns").unwrap_or(1).max(1) as usize;
            let items = child_widgets(doc, block);
            let (w, h) = grid_size(&items, cols);
            sized(Kind::Grid { cols, items }, w, h, disabled)
        }
        _ => sized(Kind::Empty, 0.0, 0.0, false),
    }
}

fn sized(kind: Kind, w: f64, h: f64, disabled: bool) -> Widget {
    Widget {
        kind,
        w,
        h,
        disabled,
    }
}

/// Build every wireframe child of a container, in source order. `doc` is
/// `None` on the measure-only path (size is theme-independent).
fn child_widgets(doc: Option<&Document>, block: &Block<'_>) -> Vec<Widget> {
    block
        .blocks()
        .filter(|b| is_wireframe_kind(b.kind()))
        .map(|b| build(doc, &b))
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

fn grid_size(items: &[Widget], cols: usize) -> (f64, f64) {
    if items.is_empty() {
        return (0.0, 0.0);
    }
    let col_w = items.iter().map(|c| c.w).fold(0.0, f64::max);
    let rows = items.len().div_ceil(cols);
    let row_h: f64 = (0..rows)
        .map(|r| {
            items[r * cols..((r + 1) * cols).min(items.len())]
                .iter()
                .map(|c| c.h)
                .fold(0.0, f64::max)
        })
        .sum::<f64>()
        + GAP * (rows.saturating_sub(1)) as f64;
    let w = col_w * cols as f64 + GAP * (cols - 1) as f64;
    (w, row_h)
}

// ── Emission ─────────────────────────────────────────────────────────

/// Emit a widget's SVG at absolute top-left `(x, y)`. Neutral colours come
/// from the resolved theme `roles`; a widget's own `class` (`theme`) overrides
/// box fill / text colour / border.
fn emit(w: &Widget, x: f64, y: f64, roles: &ThemeRoles, out: &mut String) {
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
            // Three traffic-light dots, top-left of the toolbar.
            for i in 0..3 {
                let dx = x + PAD + 5.0 + i as f64 * 13.0;
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
            let mut cx = x;
            for c in items {
                emit(c, cx, y + (w.h - c.h) / 2.0, roles, out);
                cx += c.w + ROW_GAP;
            }
        }
        Kind::Column(items) => emit_column(items, x, y, roles, out),
        Kind::Grid { cols, items } => {
            let col_w = items.iter().map(|c| c.w).fold(0.0, f64::max);
            let mut cy = y;
            for chunk in items.chunks(*cols) {
                let row_h = chunk.iter().map(|c| c.h).fold(0.0, f64::max);
                for (i, c) in chunk.iter().enumerate() {
                    emit(c, x + i as f64 * (col_w + GAP), cy, roles, out);
                }
                cy += row_h + GAP;
            }
        }
    }
    if w.disabled {
        out.push_str("</g>");
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
