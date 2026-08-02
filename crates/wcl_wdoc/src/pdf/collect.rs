//! Collect a page's blocks into the PDF [`ir`](super::ir).
//!
//! This is the PDF twin of the HTML render dispatch: rather than calling
//! `render_html_variant` (which emits HTML strings), it reuses the shared
//! lowering ([`lower_block`](crate::render::lower_block), which runs a block's
//! WCL `lower` and hands back what it produced) and walks that into
//! block/inline IR nodes. A block lowering to the **semantic content IR**
//! is read by [`content`](super::content), which matches the union
//! exhaustively; one still lowering to `HtmlFundamental` is walked by
//! [`walk_block_variant`] here. Inline text runs through the shared
//! inline-pattern engine via
//! [`InlinePatterns::render_runs`](crate::inline::InlinePatterns::render_runs),
//! so `**bold**` / `_italic_` / `` `code` `` / `[links](page)` resolve exactly
//! as on the HTML path. Non-prose fundamentals are skipped at this phase and
//! rejoin with the SVG/table work.

use std::path::Path;

use wcl_lang::{Block, Document, Value};

use crate::inline::InlinePatterns;
use crate::kinds;
use crate::render::{
    Lowered, MAX_LOWER_DEPTH, as_record_variant, cell_text, expand_component_children,
    expand_repeater_children, field_f64, field_symbol, field_utf8, gather_inline_text,
    heading_level, instance_target_def, lower_block, map_list, map_utf8, map_utf8_list,
    render_diagram,
};

use super::content;
use super::ir::{BlockNode, CardSpec, CodeSpan, FontFamily, InlineRun, ListLine, TextStyle};
use super::svg_embed;

/// Collect every child block of `page` into a flat list of flow nodes.
pub(crate) fn collect_page(
    doc: &Document,
    page: &Block<'_>,
    patterns: &InlinePatterns,
    base_dir: Option<&Path>,
) -> Vec<BlockNode> {
    let mut out = Vec::new();
    for child in page.blocks() {
        collect_block(doc, &child, patterns, base_dir, &mut out);
    }
    out
}

fn collect_block(
    doc: &Document,
    block: &Block<'_>,
    patterns: &InlinePatterns,
    base_dir: Option<&Path>,
    out: &mut Vec<BlockNode>,
) {
    // The structural kinds every backend shares — visibility filtering,
    // `notes` / `frontmatter`, `partial` deposits, and cycle-guarded
    // `collect` gathering — dispatch through the common walker.
    let structural = crate::render::walk_structural(doc, block, patterns, &mut |b| {
        collect_block(doc, b, patterns, base_dir, out);
        Ok::<(), std::convert::Infallible>(())
    });
    if structural.is_some() {
        return;
    }
    // A native block this backend doesn't implement is a build error, not a
    // silent nothing — waived per instance with `@except(backends = [:pdf])`.
    if crate::native::refuse_uncovered(block, patterns, crate::inline::Backend::Pdf) {
        return;
    }
    let kind = block.kind();
    // An `edit_object` is an editor affordance: the button exists only in the
    // `wcl editor` preview's edit mode, which is an HTML build. A PDF renders
    // nothing for it — stated here rather than left to fall through, so the
    // kind is honestly covered on this target.
    if kind == kinds::EDIT_OBJECT {
        return;
    }
    // A presentation fragment is a step-reveal wrapper — in a static PDF its
    // children simply render in place.
    if kind == kinds::FRAGMENT {
        for child in block.blocks() {
            collect_block(doc, &child, patterns, base_dir, out);
        }
        return;
    }
    // A `column` is a CSS grid: side-by-side layout is the one thing a PDF
    // page flow can't reproduce. The content is not the layout, though, so
    // the children render stacked in place — the same degradation `region`
    // and `fragment` take, and the reason `column` is native here at all
    // (until this arm existed it silently dropped its children).
    if kind == kinds::COLUMN {
        for child in block.blocks() {
            collect_block(doc, &child, patterns, base_dir, out);
        }
        return;
    }
    // A named `region` slots into an HTML template; a PDF has no template,
    // so its children render in place.
    if kind == kinds::REGION {
        for child in block.blocks() {
            collect_block(doc, &child, patterns, base_dir, out);
        }
        return;
    }
    // An `edit_field` binds its children to a data-object field for the
    // editor's Design mode — a transparent wrapper here.
    if kind == kinds::EDIT_FIELD {
        for child in block.blocks() {
            collect_block(doc, &child, patterns, base_dir, out);
        }
        return;
    }
    // Diagrams (and charts/tilemaps within them) already render to a complete
    // SVG in Rust. If the diagram has `card` shapes (foreignObject boxes), the
    // SVG draws everything but the card content, and each card body is collected
    // for native overlay painting; otherwise embed the SVG as-is.
    if kind == kinds::DIAGRAM {
        let svg = render_diagram(doc, block, patterns, base_dir);
        out.push(collect_diagram(doc, block, svg, patterns, base_dir));
        return;
    }
    // Page-level SVG blocks lowered from WCL (`lower_svg`): a complete
    // `<svg>` like `diagram`, with no `card` shapes — plain vector embed.
    if kind == kinds::SEQUENCE_DIAGRAM || kind == kinds::STATE_DIAGRAM {
        let svg = crate::render::render_lowered_svg_block(doc, block, kind, patterns);
        out.push(collect_diagram(doc, block, svg, patterns, base_dir));
        return;
    }
    // Lists are fundamental HTML blocks (no usable `lower`); read their nested
    // `li` structure directly into flattened, marked lines.
    if kind == kinds::LIST {
        let mut lines = Vec::new();
        let ordered = field_symbol(block, "style").as_deref() == Some("numbered");
        collect_li_group(doc, block, ordered, 0, "", patterns, &mut lines);
        out.push(BlockNode::List { lines });
        return;
    }
    if kind == kinds::TABLE {
        out.push(collect_table(doc, block, patterns));
        return;
    }
    // Terminals already render to a complete (static-snapshot) SVG in Rust.
    // The PDF entry bakes the window background + default colours into the
    // bare `<svg>` (no `<div>` / injected CSS to lean on when embedded).
    if kind == kinds::TERMINAL {
        out.push(BlockNode::Svg {
            svg: crate::terminal::render_terminal_pdf(doc, block, base_dir),
        });
        return;
    }
    // A page-level raster image: load the source bytes for embedding.
    if kind == kinds::IMAGE {
        if let Some(node) = collect_image(block, base_dir) {
            out.push(node);
        }
        return;
    }
    // A page-level video: in static PDF it can't play, so show the poster
    // thumbnail and — for an online video only — a link to it.
    if kind == kinds::VIDEO {
        collect_video(block, base_dir, out);
        return;
    }
    // Generators expand exactly as on the HTML / Markdown paths (the
    // shared helpers in `render/expand.rs`), so data-generated content
    // reaches the PDF too. A repeater stamps its body once per element
    // of `each`; a `wdoc_instance` renders the component named by its
    // `component` value.
    if kind == kinds::REPEATER {
        if block.binding_scope_depth() <= MAX_LOWER_DEPTH {
            for child in expand_repeater_children(block) {
                collect_block(doc, &child, patterns, base_dir, out);
            }
        }
        return;
    }
    if kind == kinds::INSTANCE {
        if block.binding_scope_depth() <= MAX_LOWER_DEPTH
            && let Some(def) = instance_target_def(block)
        {
            collect_component(doc, block, &def, patterns, base_dir, out);
        }
        return;
    }
    // A bare `wdoc_content` outside a component has no effect (the
    // substitution happens in `collect_component`).
    if kind == kinds::CONTENT {
        return;
    }
    // A `demo` in static PDF can't show a dual light/dark preview (no
    // theming) or syntax-highlighted source seam, so it collapses to one
    // render of its children in place.
    if kind == kinds::DEMO {
        for child in block.blocks() {
            collect_block(doc, &child, patterns, base_dir, out);
        }
        return;
    }
    // A user-defined `wdoc_component` instance: expand its declarative
    // body with the instance's slots bound.
    if let Some(def) = doc.kind_declarer(kind) {
        collect_component(doc, block, &def, patterns, base_dir, out);
        return;
    }
    let Some(values) = lower_block(doc, block, kind) else {
        return;
    };
    for value in &values {
        match value {
            // A content node is read from the one declaration every backend
            // shares — see `pdf::content`.
            Lowered::Content(node) => content::collect_content(doc, node, patterns, base_dir, out),
            Lowered::Html(value) => walk_block_variant(doc, value, 0, patterns, base_dir, out),
        }
    }
}

/// Expand a `wdoc_component` instance into the PDF IR: walk the
/// definition's body with the instance's slots bound, substituting the
/// instance's own children for a top-level `wdoc_content` placeholder
/// (the common layout-slot case). Mirrors the Markdown emitter.
fn collect_component(
    doc: &Document,
    instance: &Block<'_>,
    def: &Block<'_>,
    patterns: &InlinePatterns,
    base_dir: Option<&Path>,
    out: &mut Vec<BlockNode>,
) {
    if instance.binding_scope_depth() > MAX_LOWER_DEPTH {
        return;
    }
    for child in expand_component_children(instance, def) {
        if child.kind() == kinds::CONTENT {
            for ic in instance.blocks() {
                collect_block(doc, &ic, patterns, base_dir, out);
            }
        } else {
            collect_block(doc, &child, patterns, base_dir, out);
        }
    }
}

/// Turn one fundamental value into block-level IR, pushing onto `out`.
/// `depth` bounds the custom-variant recursion below, the same way the
/// HTML renderer bounds its own.
fn walk_block_variant(
    doc: &Document,
    value: &Value,
    depth: usize,
    patterns: &InlinePatterns,
    base_dir: Option<&Path>,
    out: &mut Vec<BlockNode>,
) {
    if depth > MAX_LOWER_DEPTH {
        return;
    }
    let Some((kind, map)) = as_record_variant(value) else {
        return;
    };
    match kind.as_str() {
        "paragraph" => {
            let text = map_utf8_list(map, "spans").join("");
            let classes = map_utf8_list(map, "class");
            if let Some(level) = heading_level(&classes) {
                // Headings render as a single styled run (inline emphasis in a
                // heading is rare and would fight the heading style).
                out.push(BlockNode::Heading {
                    level,
                    runs: vec![InlineRun::Text {
                        text,
                        style: TextStyle::heading(),
                    }],
                });
            } else {
                out.push(BlockNode::Paragraph {
                    runs: patterns.render_runs(doc, &text),
                });
            }
        }
        "element" => {
            let tag = map_utf8(map, "tag").unwrap_or_default();
            let children = map_list(map, "children");
            match tag.as_str() {
                "p" | "span" | "div" => {
                    let runs = collect_runs(doc, children, patterns);
                    if !runs.is_empty() {
                        out.push(BlockNode::Paragraph { runs });
                    }
                }
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    let level = tag.as_bytes()[1] - b'0';
                    out.push(BlockNode::Heading {
                        level,
                        runs: vec![InlineRun::Text {
                            text: gather_inline_text(children),
                            style: TextStyle::heading(),
                        }],
                    });
                }
                // Unknown wrapper: descend, treating children as blocks.
                _ => {
                    for c in children {
                        walk_block_variant(doc, c, depth + 1, patterns, base_dir, out);
                    }
                }
            }
        }
        // A bare inline run with no paragraph wrapper.
        "inline" => {
            let text = map_utf8(map, "text").unwrap_or_default();
            let runs = patterns.render_runs(doc, &text);
            if !runs.is_empty() {
                out.push(BlockNode::Paragraph { runs });
            }
        }
        // A block equation lowers to an `HtmlFundamental::Math` carrying a
        // self-contained `<svg>` (RaTeX) — embed it.
        "math" => out.push(BlockNode::Svg {
            svg: crate::math::render_math_fundamental(map),
        }),
        // A code block lowers to an `HtmlFundamental::Highlighted` — re-run
        // syntect to get coloured token runs for native drawing.
        "highlighted" => {
            let source = map_utf8(map, "source").unwrap_or_default();
            let language = map_utf8(map, "language").unwrap_or_default();
            let lines = crate::highlight::highlight_spans(&source, &language)
                .into_iter()
                .map(|spans| {
                    spans
                        .into_iter()
                        .map(|(text, color)| CodeSpan { text, color })
                        .collect()
                })
                .collect();
            out.push(BlockNode::Code { lines });
        }
        // An unhandled member of the HTML element vocabulary is not a
        // custom variant, so it must not be expanded as one — it simply
        // has no PDF reading, as it always had none.
        _ if crate::render::is_html_fundamental(value) => {}
        // A custom variant — expand it through its kind's own `lower` and
        // walk what that produced (content or another fundamental). This
        // recursion used to live only in the HTML renderer, which is why a
        // user block whose lowering returned another custom variant
        // rendered in the book and nowhere else.
        other => {
            for v in crate::render::expand_custom_variant(doc, map, other) {
                match crate::render::recursed_content(&v) {
                    Some(node) => content::collect_content(doc, &node, patterns, base_dir, out),
                    None => walk_block_variant(doc, &v, depth + 1, patterns, base_dir, out),
                }
            }
        }
    }
}

/// Flatten inline-bearing children into styled runs through the pattern engine.
fn collect_runs(doc: &Document, children: &[Value], patterns: &InlinePatterns) -> Vec<InlineRun> {
    let mut runs = Vec::new();
    for c in children {
        let Some((kind, map)) = as_record_variant(c) else {
            continue;
        };
        match kind.as_str() {
            "inline" => {
                let text = map_utf8(map, "text").unwrap_or_default();
                runs.extend(patterns.render_runs(doc, &text));
            }
            "paragraph" => {
                let text = map_utf8_list(map, "spans").join("");
                runs.extend(patterns.render_runs(doc, &text));
            }
            "element" => {
                runs.extend(collect_runs(doc, map_list(map, "children"), patterns));
            }
            _ => {}
        }
    }
    runs
}

/// Build a diagram node. When the rendered SVG holds `card` foreignObjects, pair
/// each (in render order) with its source `card` block, collect the card's
/// title + body into PDF blocks, and carry them as overlays; otherwise a plain
/// `Svg` node. Falls back to `Svg` if the card↔box counts disagree.
fn collect_diagram(
    doc: &Document,
    block: &Block<'_>,
    svg: String,
    patterns: &InlinePatterns,
    base_dir: Option<&Path>,
) -> BlockNode {
    if !svg.contains("<foreignObject") {
        return BlockNode::Svg { svg };
    }
    let rects = svg_embed::card_rects(&svg);
    let mut card_blocks: Vec<Block<'_>> = Vec::new();
    collect_card_blocks(block, &mut card_blocks);
    let viewbox = svg_embed::parse_viewbox(&svg);
    if rects.is_empty() || rects.len() != card_blocks.len() || viewbox.is_none() {
        return BlockNode::Svg { svg };
    }
    let cards = card_blocks
        .iter()
        .zip(rects)
        .map(|(card, rect)| {
            let mut body = Vec::new();
            if let Some(title) = field_utf8(card, "title") {
                body.push(BlockNode::Paragraph {
                    runs: vec![InlineRun::Text {
                        text: title,
                        style: TextStyle {
                            family: FontFamily::Serif,
                            bold: true,
                            italic: false,
                        },
                    }],
                });
            }
            for child in card.blocks() {
                collect_block(doc, &child, patterns, base_dir, &mut body);
            }
            CardSpec { rect, body }
        })
        .collect();
    BlockNode::Diagram {
        svg,
        viewbox: viewbox.expect("checked above"),
        cards,
    }
}

/// Collect every `card` shape within a diagram, depth-first in source order
/// (matching the foreignObject render order). Containers and the `timeline`
/// shape are descended into; a card's own body is not (its children are content,
/// not further diagram cards).
fn collect_card_blocks<'a>(block: &Block<'a>, out: &mut Vec<Block<'a>>) {
    for child in block.blocks() {
        if child.kind() == "card" {
            out.push(child);
        } else {
            collect_card_blocks(&child, out);
        }
    }
}

/// Collect a `table` block (either the computed-`rows` form or the native
/// pipe-row form) into header + body run-cells.
fn collect_table(doc: &Document, block: &Block<'_>, patterns: &InlinePatterns) -> BlockNode {
    let cell = |v: &Value| -> Vec<InlineRun> { patterns.render_runs(doc, &cell_text(v)) };

    // Computed-rows form.
    if let Some(Value::List(body)) = crate::render::computed_field(block, "rows") {
        let header: Vec<Vec<InlineRun>> = match crate::render::computed_field(block, "header") {
            Some(Value::List(cells)) => cells.iter().map(&cell).map(bold_cell).collect(),
            _ => Vec::new(),
        };
        let rows = body
            .iter()
            .map(|r| match r {
                Value::List(cells) => cells.iter().map(&cell).collect(),
                other => vec![cell(other)],
            })
            .collect();
        return BlockNode::Table { header, rows };
    }

    // Native pipe-row form: the first row is the header.
    let mut all: Vec<Vec<Vec<InlineRun>>> = Vec::new();
    for table in block.tables() {
        for row in table.rows() {
            if let Ok(values) = row.values() {
                all.push(values.iter().map(&cell).collect());
            }
        }
    }
    let header = if all.is_empty() {
        Vec::new()
    } else {
        all.remove(0).into_iter().map(bold_cell).collect()
    };
    BlockNode::Table { header, rows: all }
}

/// Load a page-level `image` block's source file for raster embedding. Skips
/// remote (`http(s):`) and `data:` sources and unreadable paths.
fn collect_image(block: &Block<'_>, base_dir: Option<&Path>) -> Option<BlockNode> {
    let source = match block.labels().ok()?.into_iter().next()? {
        Value::Utf8(s) | Value::Ascii(s) => s,
        _ => return None,
    };
    image_node(
        &source,
        base_dir,
        field_f64(block, "width").map(|v| v as f32),
        field_f64(block, "height").map(|v| v as f32),
    )
}

/// Load an image source for raster embedding, at an optional display size.
/// Skips remote (`http(s):`) and `data:` sources — there is no network at
/// build time. Shared with the content IR's `Image` / `Video` nodes.
pub(super) fn image_node(
    source: &str,
    base_dir: Option<&Path>,
    disp_w: Option<f32>,
    disp_h: Option<f32>,
) -> Option<BlockNode> {
    if source.starts_with("http://")
        || source.starts_with("https://")
        || source.starts_with("data:")
    {
        return None;
    }
    let path = match base_dir {
        Some(dir) => dir.join(source),
        None => Path::new(source).to_path_buf(),
    };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            // HTML/Markdown fail later when the asset copy errors, but the
            // PDF never copies — without this the image just vanishes.
            crate::render::record_render_warning(format!(
                "image \"{source}\": cannot read {} ({e}) — it is missing from the PDF",
                path.display()
            ));
            return None;
        }
    };
    Some(BlockNode::Image {
        bytes,
        disp_w,
        disp_h,
    })
}

/// Collect a page `video` into static PDF nodes: the poster thumbnail (when
/// it's an embeddable local image) plus, for an online video, a link to it.
/// A local video gets only its poster — a `file:`-style path is useless in a
/// distributed PDF — so it gets no link.
fn collect_video(block: &Block<'_>, base_dir: Option<&Path>, out: &mut Vec<BlockNode>) {
    // Poster: only a local file can be embedded (there's no network at build
    // time), so a remote poster / YouTube auto-thumbnail is skipped here.
    if let Some(poster) = field_utf8(block, "poster")
        && !poster.starts_with("http://")
        && !poster.starts_with("https://")
        && !poster.starts_with("data:")
    {
        let path = match base_dir {
            Some(dir) => dir.join(&poster),
            None => Path::new(&poster).to_path_buf(),
        };
        if let Ok(bytes) = std::fs::read(path) {
            out.push(BlockNode::Image {
                bytes,
                disp_w: field_f64(block, "width").map(|v| v as f32),
                disp_h: field_f64(block, "height").map(|v| v as f32),
            });
        }
    }

    let Some(source) = block.labels().ok().and_then(|l| l.into_iter().next()) else {
        return;
    };
    let source = match source {
        Value::Utf8(s) | Value::Ascii(s) => s,
        _ => return,
    };
    if let Some(url) = crate::video::online_url(&source) {
        let label = field_utf8(block, "title").unwrap_or_else(|| url.clone());
        out.push(BlockNode::Paragraph {
            runs: vec![InlineRun::Link {
                runs: vec![InlineRun::Text {
                    text: label,
                    style: TextStyle::body(),
                }],
                href: url,
            }],
        });
    }
}

/// Force every run in a (header) cell bold.
fn bold_cell(runs: Vec<InlineRun>) -> Vec<InlineRun> {
    runs.into_iter().map(bold_run).collect()
}

pub(super) fn bold_run(run: InlineRun) -> InlineRun {
    match run {
        InlineRun::Text { text, mut style } => {
            style.bold = true;
            InlineRun::Text { text, style }
        }
        InlineRun::Link { runs, href } => InlineRun::Link {
            runs: runs.into_iter().map(bold_run).collect(),
            href,
        },
        // An inline object (icon/equation) has no text style to bold.
        InlineRun::Object { svg } => InlineRun::Object { svg },
    }
}

/// Flatten the `li` children of `parent` (a `list` or an `li`) into marked
/// lines. `ordered` selects numbered vs bullet markers; `prefix` is the parent
/// item's number path (`"1.2"`); `depth` drives indentation. A bare `li` nested
/// under an `li` forms an implicit sublist in the parent's style; a nested
/// `list` block carries its own style.
fn collect_li_group(
    doc: &Document,
    parent: &Block<'_>,
    ordered: bool,
    depth: u8,
    prefix: &str,
    patterns: &InlinePatterns,
    lines: &mut Vec<ListLine>,
) {
    let mut i = 0u32;
    for li in parent.blocks().filter(|b| b.kind() == "li") {
        i += 1;
        let num = if prefix.is_empty() {
            i.to_string()
        } else {
            format!("{prefix}.{i}")
        };
        let marker = if ordered {
            format!("{num}.")
        } else {
            bullet(depth).to_string()
        };
        let text = li_text(&li);
        lines.push(ListLine {
            depth,
            marker,
            runs: patterns.render_runs(doc, &text),
        });

        // Implicit sublist: bare `li`s directly under this `li`.
        if li.blocks().any(|b| b.kind() == "li") {
            let sub_prefix = if ordered { num.as_str() } else { "" };
            collect_li_group(doc, &li, ordered, depth + 1, sub_prefix, patterns, lines);
        }
        // Explicit nested `list` blocks with their own style.
        for sub in li.blocks().filter(|b| b.kind() == kinds::LIST) {
            let sub_ordered = field_symbol(&sub, "style").as_deref() == Some("numbered");
            let sub_prefix = if sub_ordered && ordered {
                num.as_str()
            } else {
                ""
            };
            collect_li_group(
                doc,
                &sub,
                sub_ordered,
                depth + 1,
                sub_prefix,
                patterns,
                lines,
            );
        }
    }
}

pub(super) fn bullet(depth: u8) -> &'static str {
    match depth % 3 {
        0 => "•",
        1 => "◦",
        _ => "▪",
    }
}

/// The inline text of an `li` (its `@inline(0)` label slot).
fn li_text(li: &Block<'_>) -> String {
    match li.labels().ok().and_then(|l| l.into_iter().next()) {
        Some(Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) | Value::Symbol(s)) => s,
        _ => String::new(),
    }
}
