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

#[derive(Debug, Clone)]
struct TermRun {
    row: usize,
    col: usize,
    cells: usize,
    text: String,
    style: TermStyle,
}

pub(crate) fn intrinsic_size(attrs: &indexmap::IndexMap<String, String>) -> (f64, f64) {
    let metrics = TerminalMetrics::from_attrs(attrs, None, None);
    let chrome_height = terminal_chrome_height(attrs);
    (metrics.width, metrics.height + chrome_height)
}

pub(crate) fn render_terminal_svg(node: &ShapeNode, svg: &mut String) {
    let b = node.resolved;
    let chrome_height = terminal_chrome_height(&node.attrs).min(b.height.max(0.0));
    let body_height = (b.height - chrome_height).max(1.0);
    let metrics = TerminalMetrics::from_attrs(&node.attrs, Some(b.width), Some(body_height));
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
    let id_suffix = node
        .id
        .as_deref()
        .map(sanitize_id)
        .unwrap_or_else(|| format!("{:x}", hash_bounds(b)));
    let window_clip_id = format!("wdoc-terminal-window-clip-{id_suffix}");
    let body_clip_id = format!("wdoc-terminal-clip-{id_suffix}");
    let has_chrome = chrome_height > 0.0;
    let title = node
        .attrs
        .get("title")
        .map(|s| s.as_str())
        .unwrap_or("Terminal");
    let chrome_fill = node
        .attrs
        .get("chrome_fill")
        .map(|s| s.as_str())
        .unwrap_or("#111827");
    let chrome_foreground = node
        .attrs
        .get("chrome_foreground_fill")
        .map(|s| s.as_str())
        .unwrap_or("#cbd5e1");
    let chrome_border = node
        .attrs
        .get("chrome_border_fill")
        .or_else(|| node.attrs.get("stroke"))
        .map(|s| s.as_str())
        .unwrap_or("#334155");
    let title_font_size = (metrics.font_size * 0.92).min((chrome_height * 0.48).max(8.0));
    let title_y = chrome_height / 2.0 + title_font_size * 0.35;
    let root_attrs = node_attrs(node, b);

    write!(
        svg,
        "<g transform=\"translate({},{})\"{}><defs><clipPath id=\"{}\"><rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" rx=\"{}\" ry=\"{}\"/></clipPath><clipPath id=\"{}\"><rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\"/></clipPath></defs>",
        b.x,
        b.y,
        root_attrs,
        escape_attr(&window_clip_id),
        b.width,
        b.height,
        escape_attr(rx),
        escape_attr(ry),
        escape_attr(&body_clip_id),
        b.width,
        body_height
    )
    .unwrap();

    if has_chrome {
        write!(
            svg,
            "<g clip-path=\"url(#{})\"><rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"{}\"/><rect x=\"0\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"/><text x=\"14\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" fill=\"{}\" style=\"white-space:pre\" pointer-events=\"none\">{}</text></g><rect x=\"0.5\" y=\"0.5\" width=\"{}\" height=\"{}\" rx=\"{}\" ry=\"{}\" fill=\"none\" stroke=\"{}\"/>",
            escape_attr(&window_clip_id),
            b.width,
            chrome_height,
            escape_attr(chrome_fill),
            chrome_height,
            b.width,
            body_height,
            escape_attr(background),
            title_y,
            escape_attr(font_family),
            title_font_size,
            escape_attr(chrome_foreground),
            escape_text(title),
            (b.width - 1.0).max(0.0),
            (b.height - 1.0).max(0.0),
            escape_attr(rx),
            escape_attr(ry),
            escape_attr(chrome_border)
        )
        .unwrap();
    } else {
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
    }
    write!(
        svg,
        "<g clip-path=\"url(#{})\"><g transform=\"translate(0,{})\"><g clip-path=\"url(#{})\" font-family=\"{}\" font-size=\"{}\" style=\"white-space:pre\">",
        escape_attr(&window_clip_id),
        chrome_height,
        escape_attr(&body_clip_id),
        escape_attr(font_family),
        metrics.font_size
    )
    .unwrap();

    let content = node.attrs.get("content").map(|s| s.as_str()).unwrap_or("");
    render_ansi_runs(content, 0, 0, &metrics, foreground, background, svg);
    render_terminal_children(&node.children, &metrics, foreground, background, svg);

    svg.push_str("</g></g></g></g>");
}

fn terminal_chrome_height(attrs: &indexmap::IndexMap<String, String>) -> f64 {
    if attrs.get("chrome").map(|s| s.as_str()) == Some("none") {
        return 0.0;
    }
    attr_f64(attrs, "chrome_height").unwrap_or(28.0).max(0.0)
}

fn render_terminal_children(
    children: &[ShapeNode],
    metrics: &TerminalMetrics,
    foreground: &str,
    background: &str,
    svg: &mut String,
) {
    let mut children: Vec<_> = children.iter().enumerate().collect();
    children.sort_by(|(a_idx, a), (b_idx, b)| {
        a.z_index
            .total_cmp(&b.z_index)
            .then_with(|| a.source_order.cmp(&b.source_order))
            .then_with(|| a_idx.cmp(b_idx))
    });
    for (_, child) in children {
        render_terminal_child(child, metrics, foreground, background, svg);
    }
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
            let cells = attr_usize(&child.attrs, "cols").unwrap_or_else(|| content.width().max(1));
            let content = if child.attrs.contains_key("cols") {
                align_cells(
                    &truncate_cells(content, cells),
                    cells,
                    child
                        .attrs
                        .get("text_align")
                        .or_else(|| child.attrs.get("align"))
                        .map(String::as_str)
                        .unwrap_or("left"),
                )
            } else {
                content.to_string()
            };
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
            let bounds = grid_bounds(metrics, row, col, 1, cells.max(1));
            let attrs = node_attrs(child, bounds);
            write!(svg, "<g{}>", attrs).unwrap();
            render_ansi_runs(&content, row, col, metrics, fg, bg, svg);
            svg.push_str("</g>");
        }
        ShapeKind::TerminalBox => render_box(child, metrics, foreground, svg),
        ShapeKind::TerminalRule => render_rule(child, metrics, foreground, svg),
        ShapeKind::Group => render_terminal_group(child, metrics, foreground, background, svg),
        ShapeKind::TerminalCursor => render_cursor(child, metrics, foreground, svg),
        ShapeKind::TerminalSurface => render_surface(child, metrics, background, svg),
        _ => render_terminal_container_or_children(child, metrics, foreground, background, svg),
    }
}

fn render_terminal_container_or_children(
    child: &ShapeNode,
    metrics: &TerminalMetrics,
    foreground: &str,
    background: &str,
    svg: &mut String,
) {
    render_terminal_children(&child.children, metrics, foreground, background, svg);
}

fn render_terminal_group(
    child: &ShapeNode,
    metrics: &TerminalMetrics,
    foreground: &str,
    background: &str,
    svg: &mut String,
) {
    if let (Some(row), Some(col)) = (
        attr_usize(&child.attrs, "row"),
        attr_usize(&child.attrs, "col"),
    ) {
        let rows = attr_usize(&child.attrs, "rows").unwrap_or(1).max(1);
        let cols = attr_usize(&child.attrs, "cols").unwrap_or(1).max(1);
        let bounds = grid_bounds(metrics, row, col, rows, cols);
        write!(svg, "<g{}>", node_attrs(child, bounds)).unwrap();
        render_terminal_children(&child.children, metrics, foreground, background, svg);
        svg.push_str("</g>");
        return;
    }
    render_terminal_children(&child.children, metrics, foreground, background, svg);
}

fn render_surface(
    child: &ShapeNode,
    metrics: &TerminalMetrics,
    background: &str,
    svg: &mut String,
) {
    let row = attr_usize(&child.attrs, "row").unwrap_or(0);
    let col = attr_usize(&child.attrs, "col").unwrap_or(0);
    let rows = attr_usize(&child.attrs, "rows").unwrap_or(1).max(1);
    let cols = attr_usize(&child.attrs, "cols").unwrap_or(1).max(1);
    let fill = child
        .attrs
        .get("background_fill")
        .or_else(|| child.attrs.get("fill"))
        .map(|s| s.as_str())
        .unwrap_or(background);
    let rx = attr_f64(&child.attrs, "rx").unwrap_or(0.0);
    let bounds = grid_bounds(metrics, row, col, rows, cols);
    write!(svg, "<g{}>", node_attrs(child, bounds)).unwrap();
    write_rect(svg, bounds, fill, rx);
    if let Some(hover_fill) = child
        .attrs
        .get("hover_background_fill")
        .or_else(|| child.attrs.get("hover_fill"))
    {
        let hover_class = child
            .attrs
            .get("hover_class")
            .map(String::as_str)
            .unwrap_or("wdoc-terminal-control-hover");
        write_rect_with_class(svg, bounds, hover_fill, hover_class);
    }
    svg.push_str("</g>");
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
    let attrs = node_attrs(child, bounds);
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
    if node
        .attrs
        .get("_wdoc_terminal_grid_group")
        .is_some_and(|value| value == "true")
    {
        write!(out, " data-wdoc-terminal-grid-group=\"true\"").unwrap();
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

fn align_cells(value: &str, cells: usize, align: &str) -> String {
    let width = value.width();
    if width >= cells {
        return value.to_string();
    }
    let remaining = cells - width;
    match align {
        "center" | "middle" => {
            let left = remaining / 2;
            let right = remaining - left;
            format!("{}{}{}", " ".repeat(left), value, " ".repeat(right))
        }
        "right" | "end" => format!("{}{}", " ".repeat(remaining), value),
        _ => value.to_string(),
    }
}

fn attr_usize(attrs: &indexmap::IndexMap<String, String>, key: &str) -> Option<usize> {
    attrs.get(key)?.parse::<usize>().ok()
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
    fn terminal_svg_renders_window_chrome_by_default() {
        let mut attrs = IndexMap::new();
        attrs.insert("rows".to_string(), "6".to_string());
        attrs.insert("cols".to_string(), "24".to_string());
        attrs.insert("content".to_string(), "ok".to_string());
        attrs.insert("title".to_string(), "wdoc serve".to_string());
        attrs.insert("chrome_fill".to_string(), "#101827".to_string());
        attrs.insert("chrome_foreground_fill".to_string(), "#e2e8f0".to_string());
        attrs.insert("chrome_border_fill".to_string(), "#475569".to_string());
        attrs.insert("font_size".to_string(), "12".to_string());

        let terminal = ShapeNode {
            kind: ShapeKind::Terminal,
            id: Some("term".to_string()),
            x: None,
            y: None,
            width: Some(240.0),
            height: Some(148.0),
            top: None,
            bottom: None,
            left: None,
            right: None,
            resolved: Bounds {
                x: 10.0,
                y: 20.0,
                width: 240.0,
                height: 148.0,
            },
            attrs,
            events: vec![],
            children: vec![],
            align: crate::shapes::Alignment::None,
            gap: 0.0,
            padding: 0.0,
            z_index: 0.0,
            source_order: 0,
        };

        let mut svg = String::new();
        render_terminal_svg(&terminal, &mut svg);
        assert!(svg.contains("wdoc-terminal-window-clip-term"));
        assert!(svg.contains("fill=\"#101827\""));
        assert!(svg.contains("fill=\"#e2e8f0\""));
        assert!(svg.contains("stroke=\"#475569\""));
        assert!(svg.contains(">wdoc serve</text>"));
        assert!(svg.contains("translate(0,28)"));
        assert!(svg.contains(">ok</text>"));
    }

    #[test]
    fn terminal_chrome_can_be_disabled() {
        let mut attrs = IndexMap::new();
        attrs.insert("rows".to_string(), "6".to_string());
        attrs.insert("cols".to_string(), "24".to_string());
        attrs.insert("content".to_string(), "ok".to_string());
        attrs.insert("chrome".to_string(), "none".to_string());

        let terminal = ShapeNode {
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
            children: vec![],
            align: crate::shapes::Alignment::None,
            gap: 0.0,
            padding: 0.0,
            z_index: 0.0,
            source_order: 0,
        };

        let mut svg = String::new();
        render_terminal_svg(&terminal, &mut svg);
        assert!(!svg.contains(">Terminal</text>"));
        assert!(svg.contains("translate(0,0)"));
        assert!(svg.contains(">ok</text>"));
    }

    #[test]
    fn terminal_intrinsic_size_includes_default_chrome() {
        let mut attrs = IndexMap::new();
        attrs.insert("rows".to_string(), "2".to_string());
        attrs.insert("cols".to_string(), "10".to_string());
        attrs.insert("font_size".to_string(), "10".to_string());
        attrs.insert("cell_width".to_string(), "6".to_string());
        attrs.insert("cell_height".to_string(), "12".to_string());
        attrs.insert("padding".to_string(), "2".to_string());

        let (_, height) = intrinsic_size(&attrs);
        assert_eq!(height, 56.0);

        attrs.insert("chrome".to_string(), "none".to_string());
        let (_, plain_height) = intrinsic_size(&attrs);
        assert_eq!(plain_height, 28.0);
    }

    #[test]
    fn terminal_svg_renders_ansi_and_tui_children() {
        let mut attrs = IndexMap::new();
        attrs.insert("rows".to_string(), "6".to_string());
        attrs.insert("cols".to_string(), "24".to_string());
        attrs.insert("content".to_string(), "\x1b[32mok\x1b[0m".to_string());
        attrs.insert("font_size".to_string(), "12".to_string());
        let child = node(
            ShapeKind::TerminalText,
            "status",
            &[
                ("row", "2"),
                ("col", "2"),
                ("content", "status"),
                ("foreground_fill", "#38bdf8"),
            ],
            vec![],
        );
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
        assert!(svg.contains("fill=\"#0dbc79\""));
        assert!(svg.contains(">ok</text>"));
        assert!(svg.contains(">status</text>"));
        assert!(svg.contains("fill=\"#38bdf8\""));
    }

    #[test]
    fn terminal_surface_and_composite_children_render_compact_controls() {
        let mut attrs = IndexMap::new();
        attrs.insert("rows".to_string(), "12".to_string());
        attrs.insert("cols".to_string(), "60".to_string());
        attrs.insert("font_size".to_string(), "12".to_string());

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
                    ShapeKind::TerminalSurface,
                    "cmd_surface",
                    &[
                        ("row", "1"),
                        ("col", "2"),
                        ("rows", "2"),
                        ("cols", "24"),
                        ("background_fill", "#111827"),
                        ("class", "wdoc-terminal-control"),
                    ],
                    vec![],
                ),
                node(
                    ShapeKind::TerminalText,
                    "cmd_prompt",
                    &[
                        ("row", "1"),
                        ("col", "4"),
                        ("cols", "22"),
                        ("content", "command"),
                    ],
                    vec![],
                ),
                node(
                    ShapeKind::TerminalText,
                    "dry_run",
                    &[("row", "4"), ("col", "2"), ("content", "[x] Dry run")],
                    vec![],
                ),
                node(
                    ShapeKind::TerminalText,
                    "prod_radio",
                    &[("row", "4"), ("col", "18"), ("content", "(o) Prod")],
                    vec![],
                ),
                node(
                    ShapeKind::TerminalSurface,
                    "env_surface",
                    &[
                        ("row", "5"),
                        ("col", "2"),
                        ("cols", "14"),
                        ("background_fill", "#111827"),
                        ("hover_background_fill", "#38bdf8"),
                        ("class", "wdoc-terminal-control"),
                    ],
                    vec![],
                ),
                node(
                    ShapeKind::TerminalText,
                    "env_label",
                    &[
                        ("row", "5"),
                        ("col", "2"),
                        ("cols", "14"),
                        ("content", " prod v"),
                    ],
                    vec![],
                ),
                node(
                    ShapeKind::TerminalText,
                    "deploy",
                    &[
                        ("row", "9"),
                        ("col", "2"),
                        ("cols", "14"),
                        ("content", "[ Deploy ]"),
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
    }

    #[test]
    fn terminal_children_render_by_z_index_then_source_order() {
        let mut attrs = IndexMap::new();
        attrs.insert("rows".to_string(), "4".to_string());
        attrs.insert("cols".to_string(), "20".to_string());

        let mut high = node(
            ShapeKind::TerminalText,
            "high",
            &[("row", "1"), ("col", "1"), ("content", "HIGH")],
            vec![],
        );
        high.z_index = 10.0;
        high.source_order = 0;
        let mut low = node(
            ShapeKind::TerminalText,
            "low",
            &[("row", "1"), ("col", "1"), ("content", "LOW")],
            vec![],
        );
        low.z_index = -1.0;
        low.source_order = 1;

        let terminal = ShapeNode {
            kind: ShapeKind::Terminal,
            id: Some("term".to_string()),
            x: None,
            y: None,
            width: Some(220.0),
            height: Some(80.0),
            top: None,
            bottom: None,
            left: None,
            right: None,
            resolved: Bounds {
                x: 0.0,
                y: 0.0,
                width: 220.0,
                height: 80.0,
            },
            attrs,
            events: vec![],
            children: vec![high, low],
            align: crate::shapes::Alignment::None,
            gap: 0.0,
            padding: 0.0,
            z_index: 0.0,
            source_order: 0,
        };

        let mut svg = String::new();
        render_terminal_svg(&terminal, &mut svg);
        let low_pos = svg.find(">LOW</text>").expect("low text should render");
        let high_pos = svg.find(">HIGH</text>").expect("high text should render");
        assert!(low_pos < high_pos);
    }
}
