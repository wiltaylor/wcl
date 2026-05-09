use std::fmt::Write;

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::shapes::{Bounds, DiagramEvent, ShapeKind, ShapeNode};

#[derive(Debug, Clone, Copy)]
pub(crate) struct TerminalMetrics {
    pub rows: usize,
    pub cols: usize,
    pub font_size: f64,
    pub cell_width: f64,
    pub cell_height: f64,
    pub padding: f64,
    pub width: f64,
    pub height: f64,
}

impl TerminalMetrics {
    pub(crate) fn from_attrs(
        attrs: &indexmap::IndexMap<String, String>,
        width: Option<f64>,
        height: Option<f64>,
    ) -> Self {
        let rows = attr_usize(attrs, "rows").unwrap_or(24).max(1);
        let cols = attr_usize(attrs, "cols").unwrap_or(80).max(1);
        let font_size = attr_f64(attrs, "font_size").unwrap_or(14.0).max(1.0);
        let line_height = attr_f64(attrs, "line_height").unwrap_or(1.25).max(1.0);
        let padding = attr_f64(attrs, "padding").unwrap_or(12.0).max(0.0);
        let default_cell_width = (font_size * 0.62).max(1.0);
        let default_cell_height = (font_size * line_height).max(1.0);
        let cell_width = attr_f64(attrs, "cell_width")
            .or_else(|| width.map(|w| ((w - padding * 2.0) / cols as f64).max(1.0)))
            .unwrap_or(default_cell_width);
        let cell_height = attr_f64(attrs, "cell_height")
            .or_else(|| height.map(|h| ((h - padding * 2.0) / rows as f64).max(1.0)))
            .unwrap_or(default_cell_height);
        let width = width.unwrap_or(padding * 2.0 + cols as f64 * cell_width);
        let height = height.unwrap_or(padding * 2.0 + rows as f64 * cell_height);
        Self {
            rows,
            cols,
            font_size,
            cell_width,
            cell_height,
            padding,
            width,
            height,
        }
    }

    fn x(&self, col: usize) -> f64 {
        self.padding + col as f64 * self.cell_width
    }

    fn y(&self, row: usize) -> f64 {
        self.padding + row as f64 * self.cell_height
    }

    fn baseline(&self, row: usize) -> f64 {
        self.y(row) + self.cell_height * 0.78
    }
}

#[derive(Debug, Clone, PartialEq)]
struct TermStyle {
    fg: Option<String>,
    bg: Option<String>,
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    double_underline: bool,
    blink: bool,
    inverse: bool,
    hidden: bool,
    strikethrough: bool,
    overline: bool,
    css_class: Option<String>,
}

impl Default for TermStyle {
    fn default() -> Self {
        Self {
            fg: None,
            bg: None,
            bold: false,
            dim: false,
            italic: false,
            underline: false,
            double_underline: false,
            blink: false,
            inverse: false,
            hidden: false,
            strikethrough: false,
            overline: false,
            css_class: None,
        }
    }
}

impl TermStyle {
    fn default_with_class(class_name: &str) -> Self {
        let mut style = Self::default();
        style.css_class = Some(class_name.to_string());
        style
    }
}

#[derive(Debug, Clone)]
struct TermRun {
    row: usize,
    col: usize,
    cells: usize,
    text: String,
    style: TermStyle,
}

#[derive(Debug, Clone)]
struct TerminalMenuItem {
    id: Option<String>,
    label: String,
    target: Option<String>,
    close_targets: Option<Vec<String>>,
    disabled: bool,
}

pub(crate) fn intrinsic_size(attrs: &indexmap::IndexMap<String, String>) -> (f64, f64) {
    let metrics = TerminalMetrics::from_attrs(attrs, None, None);
    (metrics.width, metrics.height)
}

pub(crate) fn render_terminal_svg(node: &ShapeNode, svg: &mut String) {
    let b = node.resolved;
    let metrics = TerminalMetrics::from_attrs(&node.attrs, Some(b.width), Some(b.height));
    let background = node
        .attrs
        .get("background_fill")
        .or_else(|| node.attrs.get("fill"))
        .map(|s| s.as_str())
        .unwrap_or("#0b1020");
    let foreground = node
        .attrs
        .get("foreground_fill")
        .or_else(|| node.attrs.get("color"))
        .map(|s| s.as_str())
        .unwrap_or("#d7e0ff");
    let font_family = node
        .attrs
        .get("font_family")
        .map(|s| s.as_str())
        .unwrap_or("\"JetBrainsMono Nerd Font\", \"Apple Color Emoji\", \"Segoe UI Emoji\", \"Noto Color Emoji\", monospace");
    let rx = node.attrs.get("rx").map(|s| s.as_str()).unwrap_or("8");
    let ry = node.attrs.get("ry").map(|s| s.as_str()).unwrap_or(rx);
    let clip_id = format!(
        "wdoc-terminal-clip-{}",
        node.id
            .as_deref()
            .map(sanitize_id)
            .unwrap_or_else(|| format!("{:x}", hash_bounds(b)))
    );
    let root_attrs = node_attrs(node, b);

    write!(
        svg,
        "<g transform=\"translate({},{})\"{}><defs><clipPath id=\"{}\"><rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" rx=\"{}\" ry=\"{}\"/></clipPath></defs>",
        b.x,
        b.y,
        root_attrs,
        escape_attr(&clip_id),
        b.width,
        b.height,
        escape_attr(rx),
        escape_attr(ry)
    )
    .unwrap();
    write!(
        svg,
        "<rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" rx=\"{}\" ry=\"{}\" fill=\"{}\"/>",
        b.width,
        b.height,
        escape_attr(rx),
        escape_attr(ry),
        escape_attr(background)
    )
    .unwrap();
    write!(
        svg,
        "<g clip-path=\"url(#{})\" font-family=\"{}\" font-size=\"{}\" style=\"white-space:pre\">",
        escape_attr(&clip_id),
        escape_attr(font_family),
        metrics.font_size
    )
    .unwrap();

    let content = node.attrs.get("content").map(|s| s.as_str()).unwrap_or("");
    render_ansi_runs(content, 0, 0, &metrics, foreground, background, svg);
    for child in &node.children {
        render_terminal_child(child, &metrics, foreground, background, svg);
    }

    svg.push_str("</g></g>");
}

fn render_terminal_child(
    child: &ShapeNode,
    metrics: &TerminalMetrics,
    foreground: &str,
    background: &str,
    svg: &mut String,
) {
    match child.kind {
        ShapeKind::TerminalText => {
            let row = attr_usize(&child.attrs, "row").unwrap_or(0);
            let col = attr_usize(&child.attrs, "col").unwrap_or(0);
            let content = child.attrs.get("content").map(|s| s.as_str()).unwrap_or("");
            let fg = child
                .attrs
                .get("foreground_fill")
                .or_else(|| child.attrs.get("fill"))
                .map(|s| s.as_str())
                .unwrap_or(foreground);
            let bg = child
                .attrs
                .get("background_fill")
                .map(|s| s.as_str())
                .unwrap_or(background);
            let bounds = grid_bounds(metrics, row, col, 1, content.width().max(1));
            let attrs = node_attrs(child, bounds);
            write!(svg, "<g{}>", attrs).unwrap();
            render_ansi_runs(content, row, col, metrics, fg, bg, svg);
            svg.push_str("</g>");
        }
        ShapeKind::TerminalBox => render_box(child, metrics, foreground, svg),
        ShapeKind::TerminalRule => render_rule(child, metrics, foreground, svg),
        ShapeKind::TerminalMenubar => render_menubar(child, metrics, foreground, background, svg),
        ShapeKind::TerminalMenu | ShapeKind::TerminalContextMenu => {
            render_menu(child, metrics, foreground, background, svg)
        }
        ShapeKind::TerminalCursor => render_cursor(child, metrics, foreground, svg),
        ShapeKind::TerminalButton => render_button(child, metrics, foreground, background, svg),
        ShapeKind::TerminalTextbox => render_textbox(child, metrics, foreground, background, svg),
        ShapeKind::TerminalCheckbox => render_checkbox(child, metrics, foreground, svg),
        ShapeKind::TerminalRadio => render_radio(child, metrics, foreground, svg),
        ShapeKind::TerminalDropdown => render_dropdown(child, metrics, foreground, background, svg),
        _ => {}
    }
}

fn render_box(child: &ShapeNode, metrics: &TerminalMetrics, foreground: &str, svg: &mut String) {
    let row = attr_usize(&child.attrs, "row").unwrap_or(0);
    let col = attr_usize(&child.attrs, "col").unwrap_or(0);
    let rows = attr_usize(&child.attrs, "rows").unwrap_or(5).max(2);
    let cols = attr_usize(&child.attrs, "cols").unwrap_or(20).max(2);
    let fill = child.attrs.get("background_fill").map(|s| s.as_str());
    let stroke = child
        .attrs
        .get("foreground_fill")
        .or_else(|| child.attrs.get("stroke"))
        .map(|s| s.as_str())
        .unwrap_or(foreground);
    let title = child.attrs.get("title").map(|s| s.as_str()).unwrap_or("");
    let bounds = grid_bounds(metrics, row, col, rows, cols);
    let mut menu_node = node_with_extra_class(child, "wdoc-terminal-menu");
    add_terminal_leave_close_events(&mut menu_node);
    let attrs = node_attrs(&menu_node, bounds);
    write!(svg, "<g{}>", attrs).unwrap();
    if let Some(fill) = fill {
        write_rect(svg, bounds, fill, 0.0);
    }
    for r in 0..rows {
        let text = if r == 0 {
            if !title.is_empty() && cols > 4 {
                let max = cols.saturating_sub(4);
                let title_text = truncate_cells(title, max);
                let used = 2 + title_text.width();
                format!(
                    "┌─{}{}┐",
                    title_text,
                    "─".repeat(cols.saturating_sub(used + 1))
                )
            } else {
                format!("┌{}┐", "─".repeat(cols.saturating_sub(2)))
            }
        } else if r == rows - 1 {
            format!("└{}┘", "─".repeat(cols.saturating_sub(2)))
        } else {
            format!("│{}│", " ".repeat(cols.saturating_sub(2)))
        };
        write_text(
            svg,
            metrics,
            row + r,
            col,
            &text,
            stroke,
            None,
            &TermStyle::default(),
        );
    }
    svg.push_str("</g>");
}

fn render_rule(child: &ShapeNode, metrics: &TerminalMetrics, foreground: &str, svg: &mut String) {
    let row = attr_usize(&child.attrs, "row").unwrap_or(0);
    let col = attr_usize(&child.attrs, "col").unwrap_or(0);
    let cols = attr_usize(&child.attrs, "cols").unwrap_or(20).max(1);
    let vertical = child
        .attrs
        .get("direction")
        .is_some_and(|value| value == "vertical");
    let rows = attr_usize(&child.attrs, "rows").unwrap_or(5).max(1);
    let glyph = child
        .attrs
        .get("glyph")
        .cloned()
        .unwrap_or_else(|| if vertical { "│" } else { "─" }.to_string());
    let fg = child
        .attrs
        .get("foreground_fill")
        .or_else(|| child.attrs.get("fill"))
        .map(|s| s.as_str())
        .unwrap_or(foreground);
    let bounds = grid_bounds(
        metrics,
        row,
        col,
        if vertical { rows } else { 1 },
        if vertical { 1 } else { cols },
    );
    let attrs = node_attrs(child, bounds);
    write!(svg, "<g{}>", attrs).unwrap();
    if vertical {
        for r in 0..rows {
            write_text(
                svg,
                metrics,
                row + r,
                col,
                &glyph,
                fg,
                None,
                &TermStyle::default(),
            );
        }
    } else {
        write_text(
            svg,
            metrics,
            row,
            col,
            &glyph.repeat(cols),
            fg,
            None,
            &TermStyle::default(),
        );
    }
    svg.push_str("</g>");
}

fn render_menu(
    child: &ShapeNode,
    metrics: &TerminalMetrics,
    foreground: &str,
    background: &str,
    svg: &mut String,
) {
    let row = attr_usize(&child.attrs, "row").unwrap_or(0);
    let col = attr_usize(&child.attrs, "col").unwrap_or(0);
    let items = terminal_menu_items(child);
    let cols = attr_usize(&child.attrs, "cols")
        .unwrap_or_else(|| {
            items
                .iter()
                .map(|item| item.label.width() + if item.target.is_some() { 4 } else { 2 })
                .max()
                .unwrap_or(1)
        })
        .max(1);
    let rows = attr_usize(&child.attrs, "rows")
        .unwrap_or(items.len())
        .max(1);
    let default_close_targets = child.attrs.get("close_targets").map(|s| split_items(s));
    let sibling_targets: Vec<String> = items
        .iter()
        .filter_map(|item| item.target.clone())
        .filter(|target| !target.is_empty())
        .collect();
    let fg = child
        .attrs
        .get("foreground_fill")
        .or_else(|| child.attrs.get("fill"))
        .map(|s| s.as_str())
        .unwrap_or(foreground);
    let bg = child
        .attrs
        .get("background_fill")
        .map(|s| s.as_str())
        .unwrap_or(background);
    let hover_fg = child
        .attrs
        .get("hover_foreground_fill")
        .or_else(|| child.attrs.get("selected_foreground_fill"))
        .map(|s| s.as_str())
        .unwrap_or(background);
    let hover_bg = child
        .attrs
        .get("hover_background_fill")
        .or_else(|| child.attrs.get("selected_background_fill"))
        .map(|s| s.as_str())
        .unwrap_or(foreground);
    let bounds = grid_bounds(metrics, row, col, rows, cols);
    let menu_node = node_with_extra_class(child, "wdoc-terminal-menu");
    let attrs = node_attrs(&menu_node, bounds);
    write!(svg, "<g{}>", attrs).unwrap();
    write_rect(svg, bounds, bg, 0.0);
    for (idx, item) in items.into_iter().take(rows).enumerate() {
        let close_targets = item
            .close_targets
            .as_ref()
            .or(default_close_targets.as_ref())
            .unwrap_or(&sibling_targets);
        let item_bounds = grid_bounds(metrics, row + idx, col, 1, cols);
        let item_id = item.id.clone().unwrap_or_else(|| {
            child
                .id
                .as_deref()
                .map(|id| format!("{id}_item_{idx}"))
                .unwrap_or_else(|| format!("terminal_menu_item_{idx}"))
        });
        let item_class = if item.disabled {
            "wdoc-terminal-menu-item wdoc-terminal-menu-item-disabled"
        } else {
            "wdoc-terminal-menu-item"
        };
        let item_node = synthetic_terminal_item_node(
            &item_id,
            item_class,
            item_bounds,
            idx,
            item.disabled,
            item.target.as_deref(),
            close_targets,
        );
        write!(svg, "<g{}>", node_attrs(&item_node, item_bounds)).unwrap();
        write_rect(svg, item_bounds, bg, 0.0);
        write_rect_with_class(svg, item_bounds, hover_bg, "wdoc-terminal-menu-item-bg");
        let label_cols = if item.target.is_some() {
            cols.saturating_sub(3)
        } else {
            cols.saturating_sub(1)
        };
        let label = format!(" {}", truncate_cells(&item.label, label_cols));
        write_text(
            svg,
            metrics,
            row + idx,
            col,
            &format!("{label:<width$}", width = cols),
            fg,
            None,
            &TermStyle::default(),
        );
        write_text(
            svg,
            metrics,
            row + idx,
            col,
            &format!("{label:<width$}", width = cols),
            hover_fg,
            None,
            &TermStyle::default_with_class("wdoc-terminal-menu-item-label-hover"),
        );
        if item.target.is_some() && cols > 0 {
            write_text(
                svg,
                metrics,
                row + idx,
                col + cols - 1,
                ">",
                fg,
                None,
                &TermStyle::default(),
            );
            write_text(
                svg,
                metrics,
                row + idx,
                col + cols - 1,
                ">",
                hover_fg,
                None,
                &TermStyle::default_with_class("wdoc-terminal-menu-item-label-hover"),
            );
        }
        svg.push_str("</g>");
    }
    svg.push_str("</g>");
}

fn render_menubar(
    child: &ShapeNode,
    metrics: &TerminalMetrics,
    foreground: &str,
    background: &str,
    svg: &mut String,
) {
    let row = attr_usize(&child.attrs, "row").unwrap_or(0);
    let col = attr_usize(&child.attrs, "col").unwrap_or(0);
    let items = terminal_menu_items(child);
    let cols = attr_usize(&child.attrs, "cols")
        .unwrap_or_else(|| {
            items
                .iter()
                .map(|item| item.label.width() + 2)
                .sum::<usize>()
                .max(1)
        })
        .max(1);
    let default_close_targets = child.attrs.get("close_targets").map(|s| split_items(s));
    let sibling_targets: Vec<String> = items
        .iter()
        .filter_map(|item| item.target.clone())
        .filter(|target| !target.is_empty())
        .collect();
    let fg = child
        .attrs
        .get("foreground_fill")
        .or_else(|| child.attrs.get("fill"))
        .map(|s| s.as_str())
        .unwrap_or(foreground);
    let bg = child
        .attrs
        .get("background_fill")
        .map(|s| s.as_str())
        .unwrap_or(background);
    let hover_fg = child
        .attrs
        .get("hover_foreground_fill")
        .or_else(|| child.attrs.get("selected_foreground_fill"))
        .map(|s| s.as_str())
        .unwrap_or(background);
    let hover_bg = child
        .attrs
        .get("hover_background_fill")
        .or_else(|| child.attrs.get("selected_background_fill"))
        .map(|s| s.as_str())
        .unwrap_or(foreground);
    let bounds = grid_bounds(metrics, row, col, 1, cols);
    let menu_node = node_with_extra_class(child, "wdoc-terminal-menu");
    let attrs = node_attrs(&menu_node, bounds);
    write!(svg, "<g{}>", attrs).unwrap();
    write_rect(svg, bounds, bg, 0.0);
    let mut current_col = col;
    for (idx, item) in items.into_iter().enumerate() {
        if current_col >= col + cols {
            break;
        }
        let close_targets = item
            .close_targets
            .as_ref()
            .or(default_close_targets.as_ref())
            .unwrap_or(&sibling_targets);
        let item_cols = (item.label.width() + 2).min(col + cols - current_col);
        let item_bounds = grid_bounds(metrics, row, current_col, 1, item_cols);
        let item_id = item.id.clone().unwrap_or_else(|| {
            child
                .id
                .as_deref()
                .map(|id| format!("{id}_item_{idx}"))
                .unwrap_or_else(|| format!("terminal_menubar_item_{idx}"))
        });
        let item_class = if item.disabled {
            "wdoc-terminal-menu-item wdoc-terminal-menu-item-disabled"
        } else {
            "wdoc-terminal-menu-item"
        };
        let item_node = synthetic_terminal_item_node(
            &item_id,
            item_class,
            item_bounds,
            idx,
            item.disabled,
            item.target.as_deref(),
            close_targets,
        );
        write!(svg, "<g{}>", node_attrs(&item_node, item_bounds)).unwrap();
        write_rect(svg, item_bounds, bg, 0.0);
        write_rect_with_class(svg, item_bounds, hover_bg, "wdoc-terminal-menu-item-bg");
        let label = format!(
            " {} ",
            truncate_cells(&item.label, item_cols.saturating_sub(2))
        );
        write_text(
            svg,
            metrics,
            row,
            current_col,
            &format!("{label:<width$}", width = item_cols),
            fg,
            None,
            &TermStyle::default(),
        );
        write_text(
            svg,
            metrics,
            row,
            current_col,
            &format!("{label:<width$}", width = item_cols),
            hover_fg,
            None,
            &TermStyle::default_with_class("wdoc-terminal-menu-item-label-hover"),
        );
        svg.push_str("</g>");
        current_col += item_cols;
    }
    svg.push_str("</g>");
}

fn render_button(
    child: &ShapeNode,
    metrics: &TerminalMetrics,
    foreground: &str,
    background: &str,
    svg: &mut String,
) {
    let row = attr_usize(&child.attrs, "row").unwrap_or(0);
    let col = attr_usize(&child.attrs, "col").unwrap_or(0);
    let label = child
        .attrs
        .get("label")
        .map(|s| s.as_str())
        .unwrap_or("Button");
    let cols = attr_usize(&child.attrs, "cols")
        .unwrap_or_else(|| label.width() + 4)
        .max(1);
    let disabled = attr_bool(&child.attrs, "disabled");
    let variant = child
        .attrs
        .get("variant")
        .map(|s| s.as_str())
        .unwrap_or("primary");
    let accent = child
        .attrs
        .get("accent_fill")
        .map(|s| s.as_str())
        .unwrap_or(match variant {
            "danger" => "#ef4444",
            "secondary" => "#334155",
            _ => "#38bdf8",
        });
    let bg = child
        .attrs
        .get("background_fill")
        .map(|s| s.as_str())
        .unwrap_or(accent);
    let fg = child
        .attrs
        .get("foreground_fill")
        .or_else(|| child.attrs.get("label_fill"))
        .map(|s| s.as_str())
        .unwrap_or(match variant {
            "secondary" => foreground,
            _ => background,
        });
    let hover_bg = child
        .attrs
        .get("hover_background_fill")
        .map(|s| s.as_str())
        .unwrap_or(foreground);
    let bounds = grid_bounds(metrics, row, col, 1, cols);
    let node = terminal_control_node(child, bounds, disabled);
    write!(svg, "<g{}>", node_attrs(&node, bounds)).unwrap();
    write_rect(svg, bounds, bg, 0.0);
    write_rect_with_class(svg, bounds, hover_bg, "wdoc-terminal-control-hover");
    let text = centered_cells(&format!("[ {label} ]"), cols);
    write_text(
        svg,
        metrics,
        row,
        col,
        &text,
        fg,
        Some(cols),
        &TermStyle::default(),
    );
    svg.push_str("</g>");
}

fn render_textbox(
    child: &ShapeNode,
    metrics: &TerminalMetrics,
    foreground: &str,
    _background: &str,
    svg: &mut String,
) {
    let row = attr_usize(&child.attrs, "row").unwrap_or(0);
    let col = attr_usize(&child.attrs, "col").unwrap_or(0);
    let rows = attr_usize(&child.attrs, "rows").unwrap_or(3).max(1);
    let cols = attr_usize(&child.attrs, "cols").unwrap_or(24).max(1);
    let disabled = attr_bool(&child.attrs, "disabled");
    let value = child.attrs.get("value").map(|s| s.as_str()).unwrap_or("");
    let placeholder = child
        .attrs
        .get("placeholder")
        .map(|s| s.as_str())
        .unwrap_or("");
    let text = if value.is_empty() { placeholder } else { value };
    let fg = if value.is_empty() {
        child
            .attrs
            .get("muted_fill")
            .or_else(|| child.attrs.get("placeholder_fill"))
            .map(|s| s.as_str())
            .unwrap_or("#94a3b8")
    } else {
        child
            .attrs
            .get("foreground_fill")
            .map(|s| s.as_str())
            .unwrap_or(foreground)
    };
    let bg = child
        .attrs
        .get("background_fill")
        .map(|s| s.as_str())
        .unwrap_or("#111827");
    let accent = child
        .attrs
        .get("accent_fill")
        .map(|s| s.as_str())
        .unwrap_or("#38bdf8");
    let bounds = grid_bounds(metrics, row, col, rows, cols);
    let node = terminal_control_node(child, bounds, disabled);
    write!(svg, "<g{}>", node_attrs(&node, bounds)).unwrap();
    write_rect(svg, bounds, bg, 0.0);
    for r in 0..rows {
        let prefix = if r == 0 { ">" } else { " " };
        write_text(
            svg,
            metrics,
            row + r,
            col,
            prefix,
            accent,
            None,
            &TermStyle::default(),
        );
    }
    let line_cols = cols.saturating_sub(2);
    for (idx, line) in wrap_terminal_text(text, line_cols)
        .into_iter()
        .take(rows)
        .enumerate()
    {
        write_text(
            svg,
            metrics,
            row + idx,
            col + 2,
            &format!("{:<width$}", line, width = line_cols),
            fg,
            Some(line_cols),
            &TermStyle::default(),
        );
    }
    if let Some(cursor_col) = attr_usize(&child.attrs, "cursor_col") {
        let cursor_col = (col + 2 + cursor_col.min(line_cols)).min(col + cols.saturating_sub(1));
        render_cursor_at(svg, metrics, row, cursor_col, accent, "bar");
    }
    svg.push_str("</g>");
}

fn render_checkbox(
    child: &ShapeNode,
    metrics: &TerminalMetrics,
    foreground: &str,
    svg: &mut String,
) {
    render_choice(child, metrics, foreground, svg, true);
}

fn render_radio(child: &ShapeNode, metrics: &TerminalMetrics, foreground: &str, svg: &mut String) {
    render_choice(child, metrics, foreground, svg, false);
}

fn render_choice(
    child: &ShapeNode,
    metrics: &TerminalMetrics,
    foreground: &str,
    svg: &mut String,
    checkbox: bool,
) {
    let row = attr_usize(&child.attrs, "row").unwrap_or(0);
    let col = attr_usize(&child.attrs, "col").unwrap_or(0);
    let label = child
        .attrs
        .get("label")
        .map(|s| s.as_str())
        .unwrap_or(if checkbox { "Checkbox" } else { "Radio" });
    let checked = attr_bool(&child.attrs, if checkbox { "checked" } else { "selected" });
    let disabled = attr_bool(&child.attrs, "disabled");
    let cols = attr_usize(&child.attrs, "cols")
        .unwrap_or_else(|| label.width() + 4)
        .max(1);
    let fg = child
        .attrs
        .get("foreground_fill")
        .map(|s| s.as_str())
        .unwrap_or(foreground);
    let accent = child
        .attrs
        .get("accent_fill")
        .map(|s| s.as_str())
        .unwrap_or("#38bdf8");
    let muted = child
        .attrs
        .get("muted_fill")
        .map(|s| s.as_str())
        .unwrap_or("#64748b");
    let mark = if checkbox {
        if checked {
            "[x]"
        } else {
            "[ ]"
        }
    } else if checked {
        "(o)"
    } else {
        "( )"
    };
    let bounds = grid_bounds(metrics, row, col, 1, cols);
    let node = terminal_control_node(child, bounds, disabled);
    write!(svg, "<g{}>", node_attrs(&node, bounds)).unwrap();
    write_text(
        svg,
        metrics,
        row,
        col,
        mark,
        if checked { accent } else { muted },
        None,
        &TermStyle::default(),
    );
    let text = format!(" {label}");
    write_text(
        svg,
        metrics,
        row,
        col + mark.width(),
        &truncate_cells(&text, cols.saturating_sub(mark.width())),
        fg,
        None,
        &TermStyle::default(),
    );
    svg.push_str("</g>");
}

fn render_dropdown(
    child: &ShapeNode,
    metrics: &TerminalMetrics,
    foreground: &str,
    background: &str,
    svg: &mut String,
) {
    let row = attr_usize(&child.attrs, "row").unwrap_or(0);
    let col = attr_usize(&child.attrs, "col").unwrap_or(0);
    let items = terminal_menu_items(child);
    let value = child.attrs.get("value").map(|s| s.as_str()).unwrap_or("");
    let placeholder = child
        .attrs
        .get("placeholder")
        .map(|s| s.as_str())
        .unwrap_or("Select");
    let selected_index = attr_usize(&child.attrs, "selected_index");
    let selected = if !value.is_empty() {
        value.to_string()
    } else if let Some(idx) = selected_index {
        items
            .get(idx)
            .map(|item| item.label.clone())
            .unwrap_or_else(|| placeholder.to_string())
    } else {
        placeholder.to_string()
    };
    let cols = attr_usize(&child.attrs, "cols")
        .unwrap_or_else(|| selected.width() + 4)
        .max(4);
    let disabled = attr_bool(&child.attrs, "disabled");
    let fg = child
        .attrs
        .get("foreground_fill")
        .map(|s| s.as_str())
        .unwrap_or(foreground);
    let muted = child
        .attrs
        .get("muted_fill")
        .map(|s| s.as_str())
        .unwrap_or("#94a3b8");
    let bg = child
        .attrs
        .get("background_fill")
        .map(|s| s.as_str())
        .unwrap_or("#111827");
    let hover_bg = child
        .attrs
        .get("hover_background_fill")
        .map(|s| s.as_str())
        .unwrap_or("#38bdf8");
    let bounds = grid_bounds(metrics, row, col, 1, cols);
    let menu_id = child
        .id
        .as_deref()
        .map(|id| format!("{id}_menu"))
        .unwrap_or_else(|| format!("terminal_dropdown_{}_{}_menu", row, col));
    let mut node = terminal_control_node(child, bounds, disabled);
    if !disabled {
        node.events.push(DiagramEvent {
            name: None,
            trigger: "click".to_string(),
            state: "shown".to_string(),
            target: Some(menu_id.clone()),
            button: None,
            mode: Some("toggle".to_string()),
            duration_ms: None,
            prevent_default: None,
            guard_targets: None,
        });
    }
    write!(svg, "<g{}>", node_attrs(&node, bounds)).unwrap();
    write_rect(svg, bounds, bg, 0.0);
    write_rect_with_class(svg, bounds, hover_bg, "wdoc-terminal-control-hover");
    let shown_fill = if value.is_empty() && selected_index.is_none() {
        muted
    } else {
        fg
    };
    let label = truncate_cells(&selected, cols.saturating_sub(4));
    write_text(
        svg,
        metrics,
        row,
        col,
        &format!(" {label:<width$} v", width = cols.saturating_sub(3)),
        shown_fill,
        Some(cols),
        &TermStyle::default(),
    );
    svg.push_str("</g>");
    let mut menu = child.clone();
    menu.kind = ShapeKind::TerminalMenu;
    menu.id = Some(menu_id);
    menu.attrs.insert("row".to_string(), (row + 1).to_string());
    menu.attrs.insert("col".to_string(), col.to_string());
    menu.attrs.insert("cols".to_string(), cols.to_string());
    menu.attrs
        .insert("rows".to_string(), items.len().max(1).to_string());
    menu.attrs
        .entry("background_fill".to_string())
        .or_insert_with(|| bg.to_string());
    menu.attrs
        .entry("hover_background_fill".to_string())
        .or_insert_with(|| hover_bg.to_string());
    let dropdown_class = if attr_bool(&child.attrs, "open") {
        "wdoc-terminal-dropdown-menu wdoc-state-shown"
    } else {
        "wdoc-terminal-dropdown-menu"
    };
    menu.attrs
        .insert("class".to_string(), dropdown_class.to_string());
    menu.attrs
        .insert("_wdoc_runtime".to_string(), "true".to_string());
    render_menu(&menu, metrics, foreground, background, svg);
}

fn render_cursor(child: &ShapeNode, metrics: &TerminalMetrics, foreground: &str, svg: &mut String) {
    let row = attr_usize(&child.attrs, "row").unwrap_or(0);
    let col = attr_usize(&child.attrs, "col").unwrap_or(0);
    let fill = child
        .attrs
        .get("fill")
        .or_else(|| child.attrs.get("foreground_fill"))
        .map(|s| s.as_str())
        .unwrap_or(foreground);
    let mode = child
        .attrs
        .get("mode")
        .map(|s| s.as_str())
        .unwrap_or("block");
    let bounds = cursor_bounds(metrics, row, col, mode);
    let attrs = node_attrs(child, bounds);
    write_cursor_rect(svg, bounds, fill, &attrs);
}

fn render_cursor_at(
    svg: &mut String,
    metrics: &TerminalMetrics,
    row: usize,
    col: usize,
    fill: &str,
    mode: &str,
) {
    let bounds = cursor_bounds(metrics, row, col, mode);
    write_cursor_rect(svg, bounds, fill, "");
}

fn cursor_bounds(metrics: &TerminalMetrics, row: usize, col: usize, mode: &str) -> Bounds {
    let mut bounds = grid_bounds(metrics, row, col, 1, 1);
    match mode {
        "bar" => bounds.width = (metrics.cell_width * 0.16).max(1.0),
        "underline" => {
            bounds.y += metrics.cell_height * 0.82;
            bounds.height = (metrics.cell_height * 0.16).max(1.0);
        }
        _ => {}
    }
    bounds
}

fn write_cursor_rect(svg: &mut String, bounds: Bounds, fill: &str, attrs: &str) {
    write!(
        svg,
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"{}/>",
        bounds.x,
        bounds.y,
        bounds.width,
        bounds.height,
        escape_attr(fill),
        attrs
    )
    .unwrap();
}

fn render_ansi_runs(
    content: &str,
    row_offset: usize,
    col_offset: usize,
    metrics: &TerminalMetrics,
    default_fg: &str,
    default_bg: &str,
    svg: &mut String,
) {
    for run in ansi_runs(
        content,
        metrics.rows.saturating_sub(row_offset),
        metrics.cols.saturating_sub(col_offset),
    ) {
        let fg = effective_fg(&run.style, default_fg, default_bg);
        let bg = effective_bg(&run.style, default_bg, default_fg);
        if bg != default_bg {
            write_rect(
                svg,
                grid_bounds(
                    metrics,
                    row_offset + run.row,
                    col_offset + run.col,
                    1,
                    run.cells,
                ),
                &bg,
                0.0,
            );
        }
        if !run.style.hidden {
            write_text(
                svg,
                metrics,
                row_offset + run.row,
                col_offset + run.col,
                &run.text,
                &fg,
                Some(run.cells),
                &run.style,
            );
        }
    }
}

fn ansi_runs(content: &str, rows: usize, cols: usize) -> Vec<TermRun> {
    let mut runs = Vec::new();
    let mut style = TermStyle::default();
    let mut row = 0usize;
    let mut col = 0usize;
    let mut buf = String::new();
    let mut buf_row = 0usize;
    let mut buf_col = 0usize;
    let mut buf_cells = 0usize;
    let mut chars = content.chars().peekable();
    while let Some(ch) = chars.next() {
        if row >= rows {
            break;
        }
        if ch == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            let mut seq = String::new();
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    if next == 'm' {
                        flush_run(
                            &mut runs,
                            &mut buf,
                            &style,
                            buf_row,
                            buf_col,
                            &mut buf_cells,
                        );
                        apply_sgr(&mut style, &seq);
                    }
                    break;
                }
                seq.push(next);
            }
            continue;
        }
        match ch {
            '\n' => {
                flush_run(
                    &mut runs,
                    &mut buf,
                    &style,
                    buf_row,
                    buf_col,
                    &mut buf_cells,
                );
                row += 1;
                col = 0;
            }
            '\r' => {
                flush_run(
                    &mut runs,
                    &mut buf,
                    &style,
                    buf_row,
                    buf_col,
                    &mut buf_cells,
                );
                col = 0;
            }
            '\x08' => {
                flush_run(
                    &mut runs,
                    &mut buf,
                    &style,
                    buf_row,
                    buf_col,
                    &mut buf_cells,
                );
                col = col.saturating_sub(1);
            }
            '\t' => {
                let next = ((col / 8) + 1) * 8;
                for _ in col..next {
                    push_cell(
                        ' ',
                        &mut runs,
                        &mut buf,
                        &style,
                        &mut buf_row,
                        &mut buf_col,
                        &mut buf_cells,
                        &mut row,
                        &mut col,
                        rows,
                        cols,
                    );
                }
            }
            _ => push_cell(
                ch,
                &mut runs,
                &mut buf,
                &style,
                &mut buf_row,
                &mut buf_col,
                &mut buf_cells,
                &mut row,
                &mut col,
                rows,
                cols,
            ),
        }
    }
    flush_run(
        &mut runs,
        &mut buf,
        &style,
        buf_row,
        buf_col,
        &mut buf_cells,
    );
    runs
}

#[allow(clippy::too_many_arguments)]
fn push_cell(
    ch: char,
    runs: &mut Vec<TermRun>,
    buf: &mut String,
    style: &TermStyle,
    buf_row: &mut usize,
    buf_col: &mut usize,
    buf_cells: &mut usize,
    row: &mut usize,
    col: &mut usize,
    rows: usize,
    cols: usize,
) {
    let width = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
    if *col >= cols || *col + width > cols {
        flush_run(runs, buf, style, *buf_row, *buf_col, buf_cells);
        *row += 1;
        *col = 0;
    }
    if *row >= rows {
        return;
    }
    if buf.is_empty() {
        *buf_row = *row;
        *buf_col = *col;
    }
    buf.push(ch);
    *buf_cells += width;
    *col += width;
}

fn flush_run(
    runs: &mut Vec<TermRun>,
    buf: &mut String,
    style: &TermStyle,
    row: usize,
    col: usize,
    cells: &mut usize,
) {
    if buf.is_empty() {
        return;
    }
    runs.push(TermRun {
        row,
        col,
        cells: *cells,
        text: std::mem::take(buf),
        style: style.clone(),
    });
    *cells = 0;
}

fn apply_sgr(style: &mut TermStyle, seq: &str) {
    let mut params: Vec<i32> = if seq.trim().is_empty() {
        vec![0]
    } else {
        seq.split([';', ':'])
            .map(|part| part.parse::<i32>().unwrap_or(0))
            .collect()
    };
    if params.is_empty() {
        params.push(0);
    }
    let mut i = 0;
    while i < params.len() {
        match params[i] {
            0 => *style = TermStyle::default(),
            1 => style.bold = true,
            2 => style.dim = true,
            3 => style.italic = true,
            4 => style.underline = true,
            5 | 6 => style.blink = true,
            7 => style.inverse = true,
            8 => style.hidden = true,
            9 => style.strikethrough = true,
            21 => {
                style.underline = true;
                style.double_underline = true;
                style.bold = false;
            }
            22 => {
                style.bold = false;
                style.dim = false;
            }
            23 => style.italic = false,
            24 => {
                style.underline = false;
                style.double_underline = false;
            }
            25 => style.blink = false,
            27 => style.inverse = false,
            28 => style.hidden = false,
            29 => style.strikethrough = false,
            53 => style.overline = true,
            55 => style.overline = false,
            30..=37 => style.fg = Some(ansi_16_color((params[i] - 30) as usize, false).to_string()),
            40..=47 => style.bg = Some(ansi_16_color((params[i] - 40) as usize, false).to_string()),
            90..=97 => style.fg = Some(ansi_16_color((params[i] - 90) as usize, true).to_string()),
            100..=107 => {
                style.bg = Some(ansi_16_color((params[i] - 100) as usize, true).to_string())
            }
            38 | 48 => {
                let is_fg = params[i] == 38;
                if let Some((color, consumed)) = parse_extended_color(&params[i + 1..]) {
                    if is_fg {
                        style.fg = Some(color);
                    } else {
                        style.bg = Some(color);
                    }
                    i += consumed;
                }
            }
            39 => style.fg = None,
            49 => style.bg = None,
            _ => {}
        }
        i += 1;
    }
}

fn parse_extended_color(params: &[i32]) -> Option<(String, usize)> {
    match params {
        [5, idx, ..] => Some((ansi_256_color((*idx).clamp(0, 255) as u8), 2)),
        [2, r, g, b, ..] => {
            let r = (*r).clamp(0, 255);
            let g = (*g).clamp(0, 255);
            let b = (*b).clamp(0, 255);
            Some((format!("#{r:02x}{g:02x}{b:02x}"), 4))
        }
        _ => None,
    }
}

fn ansi_16_color(idx: usize, bright: bool) -> &'static str {
    const NORMAL: [&str; 8] = [
        "#000000", "#cd3131", "#0dbc79", "#e5e510", "#2472c8", "#bc3fbc", "#11a8cd", "#e5e5e5",
    ];
    const BRIGHT: [&str; 8] = [
        "#666666", "#f14c4c", "#23d18b", "#f5f543", "#3b8eea", "#d670d6", "#29b8db", "#ffffff",
    ];
    if bright {
        BRIGHT[idx.min(7)]
    } else {
        NORMAL[idx.min(7)]
    }
}

fn ansi_256_color(idx: u8) -> String {
    if idx < 16 {
        let bright = idx >= 8;
        return ansi_16_color((idx % 8) as usize, bright).to_string();
    }
    if idx >= 232 {
        let v = 8 + (idx - 232) as u16 * 10;
        return format!("#{:02x}{:02x}{:02x}", v, v, v);
    }
    let n = idx - 16;
    let r = n / 36;
    let g = (n % 36) / 6;
    let b = n % 6;
    let conv = |v: u8| if v == 0 { 0 } else { 55 + v as u16 * 40 };
    format!("#{:02x}{:02x}{:02x}", conv(r), conv(g), conv(b))
}

fn effective_fg(style: &TermStyle, default_fg: &str, default_bg: &str) -> String {
    if style.inverse {
        style.bg.as_deref().unwrap_or(default_bg).to_string()
    } else {
        style.fg.as_deref().unwrap_or(default_fg).to_string()
    }
}

fn effective_bg(style: &TermStyle, default_bg: &str, default_fg: &str) -> String {
    if style.inverse {
        style.fg.as_deref().unwrap_or(default_fg).to_string()
    } else {
        style.bg.as_deref().unwrap_or(default_bg).to_string()
    }
}

fn write_text(
    svg: &mut String,
    metrics: &TerminalMetrics,
    row: usize,
    col: usize,
    text: &str,
    fill: &str,
    cells: Option<usize>,
    style: &TermStyle,
) {
    if row >= metrics.rows || col >= metrics.cols {
        return;
    }
    let x = metrics.x(col);
    let y = metrics.baseline(row);
    let weight = if style.bold {
        " font-weight=\"700\""
    } else {
        ""
    };
    let font_style = if style.italic {
        " font-style=\"italic\""
    } else {
        ""
    };
    let opacity = if style.dim { " opacity=\"0.72\"" } else { "" };
    let decoration = text_decoration(style);
    let decoration_attr = if decoration.is_empty() {
        String::new()
    } else {
        format!(" text-decoration=\"{}\"", escape_attr(&decoration))
    };
    let class_attr = match (style.blink, style.css_class.as_deref()) {
        (true, Some(class_name)) => {
            format!(" class=\"wdoc-terminal-blink {}\"", escape_attr(class_name))
        }
        (true, None) => " class=\"wdoc-terminal-blink\"".to_string(),
        (false, Some(class_name)) => format!(" class=\"{}\"", escape_attr(class_name)),
        (false, None) => String::new(),
    };
    let length_attr = cells
        .map(|cells| {
            format!(
                " textLength=\"{}\" lengthAdjust=\"spacingAndGlyphs\"",
                cells as f64 * metrics.cell_width
            )
        })
        .unwrap_or_default();
    write!(
        svg,
        "<text x=\"{}\" y=\"{}\" fill=\"{}\"{}{}{}{}{}{}>{}</text>",
        x,
        y,
        escape_attr(fill),
        weight,
        font_style,
        opacity,
        decoration_attr,
        class_attr,
        length_attr,
        escape_text(text)
    )
    .unwrap();
}

fn text_decoration(style: &TermStyle) -> String {
    let mut parts = Vec::new();
    if style.underline || style.double_underline {
        parts.push("underline");
    }
    if style.strikethrough {
        parts.push("line-through");
    }
    if style.overline {
        parts.push("overline");
    }
    parts.join(" ")
}

fn write_rect(svg: &mut String, b: Bounds, fill: &str, rx: f64) {
    write!(
        svg,
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"{}\"/>",
        b.x,
        b.y,
        b.width,
        b.height,
        rx,
        escape_attr(fill)
    )
    .unwrap();
}

fn write_rect_with_class(svg: &mut String, b: Bounds, fill: &str, class_name: &str) {
    write!(
        svg,
        "<rect class=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"/>",
        escape_attr(class_name),
        b.x,
        b.y,
        b.width,
        b.height,
        escape_attr(fill)
    )
    .unwrap();
}

fn grid_bounds(
    metrics: &TerminalMetrics,
    row: usize,
    col: usize,
    rows: usize,
    cols: usize,
) -> Bounds {
    Bounds {
        x: metrics.x(col),
        y: metrics.y(row),
        width: cols as f64 * metrics.cell_width,
        height: rows as f64 * metrics.cell_height,
    }
}

fn synthetic_terminal_item_node(
    id: &str,
    class_name: &str,
    bounds: Bounds,
    source_order: usize,
    disabled: bool,
    open_target: Option<&str>,
    close_targets: &[String],
) -> ShapeNode {
    let mut attrs = indexmap::IndexMap::new();
    attrs.insert("class".to_string(), class_name.to_string());
    attrs.insert(
        "cursor".to_string(),
        if disabled { "default" } else { "pointer" }.to_string(),
    );
    attrs.insert(
        "pointer_events".to_string(),
        if disabled { "none" } else { "all" }.to_string(),
    );
    attrs.insert("_wdoc_runtime".to_string(), "true".to_string());
    let mut events = Vec::new();
    if !disabled {
        events.push(DiagramEvent {
            name: None,
            trigger: "hover".to_string(),
            state: "hovered".to_string(),
            target: None,
            button: None,
            mode: Some("while".to_string()),
            duration_ms: None,
            prevent_default: None,
            guard_targets: None,
        });
        for target in close_targets {
            if Some(target.as_str()) == open_target || target.is_empty() {
                continue;
            }
            events.push(DiagramEvent {
                name: None,
                trigger: "hover".to_string(),
                state: "shown".to_string(),
                target: Some(target.clone()),
                button: None,
                mode: Some("remove".to_string()),
                duration_ms: None,
                prevent_default: None,
                guard_targets: None,
            });
        }
        if let Some(target) = open_target {
            events.push(DiagramEvent {
                name: None,
                trigger: "hover".to_string(),
                state: "shown".to_string(),
                target: Some(target.to_string()),
                button: None,
                mode: Some("add".to_string()),
                duration_ms: None,
                prevent_default: None,
                guard_targets: None,
            });
        }
    }
    ShapeNode {
        kind: ShapeKind::Group,
        id: Some(id.to_string()),
        x: Some(bounds.x),
        y: Some(bounds.y),
        width: Some(bounds.width),
        height: Some(bounds.height),
        top: None,
        bottom: None,
        left: None,
        right: None,
        resolved: bounds,
        attrs,
        events,
        children: Vec::new(),
        align: crate::shapes::Alignment::None,
        gap: 0.0,
        padding: 0.0,
        z_index: 0.0,
        source_order,
    }
}

fn terminal_control_node(child: &ShapeNode, bounds: Bounds, disabled: bool) -> ShapeNode {
    let class_name = if disabled {
        "wdoc-terminal-control wdoc-terminal-control-disabled"
    } else {
        "wdoc-terminal-control"
    };
    let mut node = node_with_extra_class(child, class_name);
    node.x = Some(bounds.x);
    node.y = Some(bounds.y);
    node.width = Some(bounds.width);
    node.height = Some(bounds.height);
    node.resolved = bounds;
    node.attrs.insert(
        "cursor".to_string(),
        if disabled { "default" } else { "pointer" }.to_string(),
    );
    node.attrs.insert(
        "pointer_events".to_string(),
        if disabled { "none" } else { "all" }.to_string(),
    );
    node.attrs
        .insert("_wdoc_runtime".to_string(), "true".to_string());
    if !disabled {
        node.events.push(DiagramEvent {
            name: None,
            trigger: "hover".to_string(),
            state: "hovered".to_string(),
            target: None,
            button: None,
            mode: Some("while".to_string()),
            duration_ms: None,
            prevent_default: None,
            guard_targets: None,
        });
    }
    node.children.clear();
    node
}

fn add_terminal_leave_close_events(node: &mut ShapeNode) {
    let Some(close_targets) = node
        .attrs
        .get("leave_close_targets")
        .map(|s| split_items(s))
    else {
        return;
    };
    if close_targets.is_empty() {
        return;
    }
    let guard_targets = node
        .attrs
        .get("leave_guard_targets")
        .cloned()
        .unwrap_or_else(|| close_targets.join(","));
    for target in close_targets {
        node.events.push(DiagramEvent {
            name: None,
            trigger: "mouse_leave".to_string(),
            state: "shown".to_string(),
            target: Some(target),
            button: None,
            mode: Some("remove".to_string()),
            duration_ms: None,
            prevent_default: None,
            guard_targets: Some(guard_targets.clone()),
        });
    }
}

fn node_with_extra_class(node: &ShapeNode, class_name: &str) -> ShapeNode {
    let mut node = node.clone();
    match node.attrs.get_mut("class") {
        Some(existing) => {
            if !existing.split_whitespace().any(|class| class == class_name) {
                if !existing.trim().is_empty() {
                    existing.push(' ');
                }
                existing.push_str(class_name);
            }
        }
        None => {
            node.attrs
                .insert("class".to_string(), class_name.to_string());
        }
    }
    node
}

fn node_attrs(node: &ShapeNode, b: Bounds) -> String {
    let mut out = String::new();
    for name in [
        "class",
        "style",
        "cursor",
        "pointer_events",
        "opacity",
        "visibility",
        "display",
    ] {
        if let Some(value) = node.attrs.get(name) {
            let attr = name.replace('_', "-");
            write!(out, " {}=\"{}\"", attr, escape_attr(value)).unwrap();
        }
    }
    if let Some(value) = node.attrs.get("visible") {
        let visibility = if value == "false" {
            "hidden"
        } else {
            "visible"
        };
        write!(out, " visibility=\"{}\"", visibility).unwrap();
    }
    let Some(id) = node.id.as_deref() else {
        return out;
    };
    if node.events.is_empty()
        && !node.attrs.contains_key("_wdoc_state_z")
        && node.attrs.get("_wdoc_runtime").map(|v| v == "true") != Some(true)
    {
        return out;
    }
    write!(
        out,
        " data-wdoc-id=\"{}\" data-wdoc-z-base=\"{}\" data-wdoc-x=\"{}\" data-wdoc-y=\"{}\" data-wdoc-width=\"{}\" data-wdoc-height=\"{}\"",
        escape_attr(id),
        node.z_index,
        b.x,
        b.y,
        b.width,
        b.height
    )
    .unwrap();
    if !node.events.is_empty() {
        write!(
            out,
            " data-wdoc-events=\"{}\"",
            escape_attr(&events_data(&node.events))
        )
        .unwrap();
    }
    if let Some(value) = node.attrs.get("_wdoc_state_z") {
        write!(out, " data-wdoc-state-z=\"{}\"", escape_attr(value)).unwrap();
    }
    if let Some(value) = node.attrs.get("_wdoc_state_animation") {
        write!(out, " data-wdoc-state-animation=\"{}\"", escape_attr(value)).unwrap();
    }
    if let Some(value) = node.attrs.get("_wdoc_animations") {
        write!(out, " data-wdoc-animations=\"{}\"", escape_attr(value)).unwrap();
    }
    out
}

fn events_data(events: &[DiagramEvent]) -> String {
    events
        .iter()
        .map(|event| {
            let duration_ms = event.duration_ms.unwrap_or(0).to_string();
            let mut fields = vec![
                event.trigger.as_str(),
                event.state.as_str(),
                event.target.as_deref().unwrap_or("self"),
                event.mode.as_deref().unwrap_or(""),
                event.button.as_deref().unwrap_or("left"),
                duration_ms.as_str(),
                if event
                    .prevent_default
                    .unwrap_or(event.trigger == "right_click")
                {
                    "true"
                } else {
                    "false"
                },
            ];
            if let Some(guard_targets) = event.guard_targets.as_deref() {
                fields.push(guard_targets);
            }
            fields
                .into_iter()
                .map(escape_data)
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn escape_data(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('|', "\\p")
        .replace(';', "\\s")
        .replace(',', "\\c")
        .replace(':', "\\d")
        .replace('~', "\\t")
}

fn terminal_menu_items(node: &ShapeNode) -> Vec<TerminalMenuItem> {
    node.children
        .iter()
        .filter(|child| child.kind == ShapeKind::MenuItem)
        .map(|child| {
            let label = child
                .attrs
                .get("label")
                .map(|label| label.trim())
                .filter(|label| !label.is_empty())
                .unwrap_or("Item")
                .to_string();
            let target = child
                .attrs
                .get("target")
                .map(|target| target.trim())
                .filter(|target| !target.is_empty())
                .map(str::to_string);
            let close_targets = child
                .attrs
                .get("close_targets")
                .map(|value| split_items(value));
            let disabled = child
                .attrs
                .get("disabled")
                .is_some_and(|value| value.eq_ignore_ascii_case("true"));
            TerminalMenuItem {
                id: child.id.clone(),
                label,
                target,
                close_targets,
                disabled,
            }
        })
        .collect()
}

fn split_items(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

fn centered_cells(value: &str, cols: usize) -> String {
    let text = truncate_cells(value, cols);
    let width = text.width();
    if width >= cols {
        return text;
    }
    let left = (cols - width) / 2;
    let right = cols - width - left;
    format!("{}{}{}", " ".repeat(left), text, " ".repeat(right))
}

fn wrap_terminal_text(value: &str, cols: usize) -> Vec<String> {
    if cols == 0 {
        return vec![String::new()];
    }
    let mut out = Vec::new();
    for raw_line in value.lines() {
        let mut line = raw_line.trim();
        if line.is_empty() {
            out.push(String::new());
            continue;
        }
        while !line.is_empty() {
            let chunk = truncate_cells(line, cols);
            let used = chunk.len();
            out.push(chunk);
            line = line.get(used..).unwrap_or("").trim_start();
        }
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

fn truncate_cells(value: &str, max: usize) -> String {
    let mut out = String::new();
    let mut width = 0;
    for ch in value.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
        if width + ch_width > max {
            break;
        }
        out.push(ch);
        width += ch_width;
    }
    out
}

fn attr_usize(attrs: &indexmap::IndexMap<String, String>, key: &str) -> Option<usize> {
    attrs.get(key)?.parse::<usize>().ok()
}

fn attr_bool(attrs: &indexmap::IndexMap<String, String>, key: &str) -> bool {
    attrs
        .get(key)
        .is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

fn attr_f64(attrs: &indexmap::IndexMap<String, String>, key: &str) -> Option<f64> {
    attrs.get(key)?.parse::<f64>().ok()
}

fn sanitize_id(value: &str) -> String {
    let out = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    if out.is_empty() {
        "terminal".to_string()
    } else {
        out
    }
}

fn hash_bounds(b: Bounds) -> u64 {
    ((b.x.to_bits() ^ b.y.to_bits()) ^ (b.width.to_bits() ^ b.height.to_bits())).rotate_left(13)
}

fn escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_attr(s: &str) -> String {
    escape_text(s).replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    fn node(
        kind: ShapeKind,
        id: &str,
        attrs: &[(&str, &str)],
        children: Vec<ShapeNode>,
    ) -> ShapeNode {
        ShapeNode {
            kind,
            id: Some(id.to_string()),
            x: None,
            y: None,
            width: None,
            height: None,
            top: None,
            bottom: None,
            left: None,
            right: None,
            resolved: Bounds::default(),
            attrs: attrs
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect(),
            events: vec![],
            children,
            align: crate::shapes::Alignment::None,
            gap: 0.0,
            padding: 0.0,
            z_index: 0.0,
            source_order: 0,
        }
    }

    #[test]
    fn ansi_parser_supports_sgr_colors_and_styles() {
        let runs = ansi_runs("\x1b[1;3;38;2;1;2;3;48;5;196mHi\x1b[0m!", 2, 10);
        assert_eq!(runs.len(), 2);
        assert!(runs[0].style.bold);
        assert!(runs[0].style.italic);
        assert_eq!(runs[0].style.fg.as_deref(), Some("#010203"));
        assert_eq!(runs[0].style.bg.as_deref(), Some("#ff0000"));
        assert_eq!(runs[1].text, "!");
        assert_eq!(runs[1].style, TermStyle::default());
    }

    #[test]
    fn ansi_parser_handles_tabs_cr_backspace_and_clipping() {
        let runs = ansi_runs("ab\tc\rZ\x08Y\nwide界", 2, 12);
        let text = runs
            .iter()
            .map(|run| run.text.as_str())
            .collect::<Vec<_>>()
            .join("");
        assert!(text.contains("ab"));
        assert!(text.contains("Y"));
        assert!(runs
            .iter()
            .any(|run| run.text.contains('界') && run.cells >= run.text.width()));
    }

    #[test]
    fn terminal_svg_renders_ansi_and_tui_children() {
        let mut attrs = IndexMap::new();
        attrs.insert("rows".to_string(), "6".to_string());
        attrs.insert("cols".to_string(), "24".to_string());
        attrs.insert("content".to_string(), "\x1b[32mok\x1b[0m".to_string());
        attrs.insert("font_size".to_string(), "12".to_string());
        let mut menu_attrs = IndexMap::new();
        menu_attrs.insert("row".to_string(), "2".to_string());
        menu_attrs.insert("col".to_string(), "2".to_string());
        menu_attrs.insert("rows".to_string(), "2".to_string());
        menu_attrs.insert("cols".to_string(), "12".to_string());
        let mut build_attrs = IndexMap::new();
        build_attrs.insert("label".to_string(), "Build".to_string());
        build_attrs.insert("target".to_string(), "build_menu".to_string());
        let mut test_attrs = IndexMap::new();
        test_attrs.insert("label".to_string(), "Test".to_string());
        test_attrs.insert("disabled".to_string(), "true".to_string());
        let child = ShapeNode {
            kind: ShapeKind::TerminalMenu,
            id: Some("menu".to_string()),
            x: None,
            y: None,
            width: None,
            height: None,
            top: None,
            bottom: None,
            left: None,
            right: None,
            resolved: Bounds::default(),
            attrs: menu_attrs,
            events: vec![DiagramEvent {
                name: None,
                trigger: "click".to_string(),
                state: "shown".to_string(),
                target: None,
                button: None,
                mode: Some("toggle".to_string()),
                duration_ms: None,
                prevent_default: None,
                guard_targets: None,
            }],
            children: vec![
                ShapeNode {
                    kind: ShapeKind::MenuItem,
                    id: Some("build_item".to_string()),
                    x: None,
                    y: None,
                    width: None,
                    height: None,
                    top: None,
                    bottom: None,
                    left: None,
                    right: None,
                    resolved: Bounds::default(),
                    attrs: build_attrs,
                    events: vec![],
                    children: vec![],
                    align: crate::shapes::Alignment::None,
                    gap: 0.0,
                    padding: 0.0,
                    z_index: 0.0,
                    source_order: 0,
                },
                ShapeNode {
                    kind: ShapeKind::MenuItem,
                    id: Some("test_item".to_string()),
                    x: None,
                    y: None,
                    width: None,
                    height: None,
                    top: None,
                    bottom: None,
                    left: None,
                    right: None,
                    resolved: Bounds::default(),
                    attrs: test_attrs,
                    events: vec![],
                    children: vec![],
                    align: crate::shapes::Alignment::None,
                    gap: 0.0,
                    padding: 0.0,
                    z_index: 1.0,
                    source_order: 1,
                },
            ],
            align: crate::shapes::Alignment::None,
            gap: 0.0,
            padding: 0.0,
            z_index: 0.0,
            source_order: 0,
        };
        let node = ShapeNode {
            kind: ShapeKind::Terminal,
            id: Some("term".to_string()),
            x: None,
            y: None,
            width: Some(240.0),
            height: Some(120.0),
            top: None,
            bottom: None,
            left: None,
            right: None,
            resolved: Bounds {
                x: 10.0,
                y: 20.0,
                width: 240.0,
                height: 120.0,
            },
            attrs,
            events: vec![],
            children: vec![child],
            align: crate::shapes::Alignment::None,
            gap: 0.0,
            padding: 0.0,
            z_index: 0.0,
            source_order: 0,
        };
        let mut svg = String::new();
        render_terminal_svg(&node, &mut svg);
        assert!(svg.contains("JetBrainsMono Nerd Font"));
        assert!(svg.contains("wdoc-terminal-menu"));
        assert!(svg.contains("fill=\"#0dbc79\""));
        assert!(svg.contains(">ok</text>"));
        assert!(svg.contains("Test"));
        assert!(svg.contains("&gt;</text>"));
        assert!(svg.contains("build_menu"));
        assert!(svg.contains("wdoc-terminal-menu-item-disabled"));
        assert!(svg.contains("data-wdoc-events=\"click|shown|self|toggle|left|0|false\""));
        assert!(svg.contains("wdoc-terminal-menu-item-label-hover"));
    }

    #[test]
    fn terminal_menu_item_blocks_open_targets_and_close_sibling_targets() {
        let metrics = TerminalMetrics::from_attrs(&IndexMap::new(), Some(240.0), Some(120.0));
        let mut attrs = IndexMap::new();
        attrs.insert("row".to_string(), "1".to_string());
        attrs.insert("col".to_string(), "1".to_string());
        attrs.insert("rows".to_string(), "2".to_string());
        attrs.insert("cols".to_string(), "12".to_string());
        let mut build_attrs = IndexMap::new();
        build_attrs.insert("label".to_string(), "Build".to_string());
        build_attrs.insert("target".to_string(), "build_menu".to_string());
        let mut format_attrs = IndexMap::new();
        format_attrs.insert("label".to_string(), "Format".to_string());
        let menu = ShapeNode {
            kind: ShapeKind::TerminalMenu,
            id: Some("run_menu".to_string()),
            x: None,
            y: None,
            width: None,
            height: None,
            top: None,
            bottom: None,
            left: None,
            right: None,
            resolved: Bounds::default(),
            attrs,
            events: vec![],
            children: vec![
                ShapeNode {
                    kind: ShapeKind::MenuItem,
                    id: Some("build_item".to_string()),
                    x: None,
                    y: None,
                    width: None,
                    height: None,
                    top: None,
                    bottom: None,
                    left: None,
                    right: None,
                    resolved: Bounds::default(),
                    attrs: build_attrs,
                    events: vec![],
                    children: vec![],
                    align: crate::shapes::Alignment::None,
                    gap: 0.0,
                    padding: 0.0,
                    z_index: 0.0,
                    source_order: 0,
                },
                ShapeNode {
                    kind: ShapeKind::MenuItem,
                    id: Some("format_item".to_string()),
                    x: None,
                    y: None,
                    width: None,
                    height: None,
                    top: None,
                    bottom: None,
                    left: None,
                    right: None,
                    resolved: Bounds::default(),
                    attrs: format_attrs,
                    events: vec![],
                    children: vec![],
                    align: crate::shapes::Alignment::None,
                    gap: 0.0,
                    padding: 0.0,
                    z_index: 0.0,
                    source_order: 1,
                },
            ],
            align: crate::shapes::Alignment::None,
            gap: 0.0,
            padding: 0.0,
            z_index: 0.0,
            source_order: 0,
        };
        let mut svg = String::new();
        render_menu(&menu, &metrics, "#d7e0ff", "#08111f", &mut svg);

        assert!(svg.contains("Build"));
        assert!(svg.contains("Format"));
        assert!(svg.contains("&gt;</text>"));
        assert!(svg.contains("build_menu|add"));
        assert!(svg.contains("build_menu|remove"));
    }

    #[test]
    fn terminal_form_widgets_render_compact_controls() {
        let mut attrs = IndexMap::new();
        attrs.insert("rows".to_string(), "12".to_string());
        attrs.insert("cols".to_string(), "60".to_string());
        attrs.insert("font_size".to_string(), "12".to_string());

        let dropdown = node(
            ShapeKind::TerminalDropdown,
            "env",
            &[
                ("row", "5"),
                ("col", "2"),
                ("cols", "14"),
                ("value", "prod"),
            ],
            vec![
                node(ShapeKind::MenuItem, "dev", &[("label", "dev")], vec![]),
                node(ShapeKind::MenuItem, "prod", &[("label", "prod")], vec![]),
            ],
        );
        let terminal = ShapeNode {
            kind: ShapeKind::Terminal,
            id: Some("term".to_string()),
            x: None,
            y: None,
            width: Some(600.0),
            height: Some(180.0),
            top: None,
            bottom: None,
            left: None,
            right: None,
            resolved: Bounds {
                x: 0.0,
                y: 0.0,
                width: 600.0,
                height: 180.0,
            },
            attrs,
            events: vec![],
            children: vec![
                node(
                    ShapeKind::TerminalTextbox,
                    "cmd",
                    &[
                        ("row", "1"),
                        ("col", "2"),
                        ("rows", "2"),
                        ("cols", "24"),
                        ("placeholder", "command"),
                    ],
                    vec![],
                ),
                node(
                    ShapeKind::TerminalCheckbox,
                    "dry_run",
                    &[
                        ("row", "4"),
                        ("col", "2"),
                        ("label", "Dry run"),
                        ("checked", "true"),
                    ],
                    vec![],
                ),
                node(
                    ShapeKind::TerminalRadio,
                    "prod_radio",
                    &[
                        ("row", "4"),
                        ("col", "18"),
                        ("label", "Prod"),
                        ("selected", "true"),
                    ],
                    vec![],
                ),
                dropdown,
                node(
                    ShapeKind::TerminalButton,
                    "deploy",
                    &[
                        ("row", "9"),
                        ("col", "2"),
                        ("cols", "14"),
                        ("label", "Deploy"),
                    ],
                    vec![],
                ),
            ],
            align: crate::shapes::Alignment::None,
            gap: 0.0,
            padding: 0.0,
            z_index: 0.0,
            source_order: 0,
        };

        let mut svg = String::new();
        render_terminal_svg(&terminal, &mut svg);
        assert!(svg.contains("command"));
        assert!(svg.contains("[x]"));
        assert!(svg.contains("(o)"));
        assert!(svg.contains("prod"));
        assert!(svg.contains("[ Deploy ]"));
        assert!(svg.contains("wdoc-terminal-control"));
        assert!(svg.contains("click|shown|env_menu|toggle"));
        assert!(svg.contains("wdoc-terminal-dropdown-menu"));
        assert!(svg.contains("wdoc-terminal-menu"));
    }
}
