//! Pure-Rust PDF backend for wdoc (`wcl wdoc build --type pdf`).
//!
//! Reuses the existing fundamentals lowering to build a small, paint-agnostic
//! [`ir`] block model, lays it out and paginates it ([`layout`]), and paints it
//! to a PDF with [`krilla`](::krilla) ([`paint`]) — no browser, no external
//! tools. Diagrams, charts, equations and icons are embedded vector-preserving
//! via krilla-svg ([`svg_embed`]).
//!
//! Renders prose, headings, styled inline text, links, lists, tables, code
//! blocks, callouts and SVG content across A4 / US-Letter pages with a running
//! header and footer page numbers. Output is one PDF per `site` (a `book`
//! site's pages flow in TOC order); a document with no `site` renders to a
//! single PDF named from the source file.

mod collect;
mod content;
pub(crate) mod ir;
mod layout;
mod page;
mod paint;
mod palette;
mod svg_embed;
mod text;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::Path;

use miette::{NamedSource, Report};
use wcl_lang::{Block, Document, Value, disk_loader};

use crate::build::schema_registry;
use crate::icons::IconRegistry;
use crate::image::ImageRegistry;
use crate::inline::InlinePatterns;
use crate::tileset::TilesetRegistry;

/// Physical page size. A4 is the default; US Letter is selectable via the CLI.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PageSize {
    /// ISO A4, 210 × 297 mm.
    A4,
    /// US Letter, 8.5 × 11 in.
    Letter,
}

impl PageSize {
    /// Page dimensions in PDF points (1/72 in).
    fn dimensions(self) -> (f32, f32) {
        match self {
            // 210 mm × 297 mm at 72 pt/in.
            PageSize::A4 => (595.276, 841.890),
            PageSize::Letter => (612.0, 792.0),
        }
    }
}

/// Page geometry derived from a [`PageSize`]: the physical box, the margins,
/// and the content box prose flows into. All values in PDF points.
pub(crate) struct Geometry {
    /// Page width in points.
    pub width: f32,
    /// Page height in points.
    pub height: f32,
    /// Left and right margin in points.
    pub margin_x: f32,
    /// Top margin in points.
    pub margin_top: f32,
    /// Bottom margin in points.
    pub margin_bottom: f32,
}

impl Geometry {
    /// Page geometry for the requested paper size.
    fn new(size: PageSize) -> Self {
        let (width, height) = size.dimensions();
        Self {
            width,
            height,
            margin_x: 72.0,
            margin_top: 72.0,
            margin_bottom: 72.0,
        }
    }

    /// X of the content area's left edge.
    pub(crate) fn content_left(&self) -> f32 {
        self.margin_x
    }
    /// Y of the content area's top edge.
    pub(crate) fn content_top(&self) -> f32 {
        self.margin_top
    }
    /// Width available for content.
    pub(crate) fn content_width(&self) -> f32 {
        self.width - 2.0 * self.margin_x
    }
    /// Height available for content.
    pub(crate) fn content_height(&self) -> f32 {
        self.height - self.margin_top - self.margin_bottom
    }
}

/// Errors from PDF generation. Mirrors `build::BuildError`'s shape so the CLI
/// maps them to the same exit codes.
pub enum PdfError {
    /// A filesystem operation failed; the `String` names the target.
    Io(std::io::Error, String),
    /// The entry document did not parse.
    Parse(Report),
    /// The document violated its schema; carries the violation count.
    Schema(usize),
    /// A block expression failed to evaluate during rendering. Carries a
    /// pre-built miette report with the source snippet attached.
    Eval(Report),
    /// The document is structurally unsuitable for PDF output.
    BadDoc(String),
    /// The PDF writer failed while producing output.
    Render(String),
}

impl PdfError {
    /// Render this failure to stderr, with the source snippet where the
    /// variant carries one.
    pub fn report(&self) {
        match self {
            Self::Io(e, ctx) => eprintln!("{ctx}: {e}"),
            Self::Parse(r) => eprintln!("{r:?}"),
            Self::Schema(n) => eprintln!("{n} schema violation{}", if *n == 1 { "" } else { "s" }),
            Self::Eval(r) => eprintln!("{r:?}"),
            Self::BadDoc(msg) => eprintln!("{msg}"),
            Self::Render(msg) => eprintln!("pdf render failed: {msg}"),
        }
    }

    /// Wrap a render-time evaluation failure into a `PdfError::Eval`,
    /// attaching the source file the error was raised against so the miette
    /// report renders the snippet against the correct text (a cross-file
    /// span won't line up with the root document's source).
    pub(crate) fn eval(err: wcl_lang::EvalError, src: NamedSource<String>) -> Self {
        let report = Report::new(err).with_source_code(src);
        Self::Eval(report)
    }
}

/// Render `file` to one PDF per `site` in `out_dir`. Returns the number of
/// PDFs written. `site_filter` restricts rendering to a single named site.
pub fn pdf(
    file: &Path,
    out_dir: &Path,
    site_filter: Option<&str>,
    page_size: PageSize,
) -> Result<usize, PdfError> {
    let user_src = fs::read_to_string(file)
        .map_err(|e| PdfError::Io(e, format!("read {}", file.display())))?;
    let name = file.display().to_string();

    let base_dir = file.parent().map(std::path::Path::to_path_buf);
    let loader = schema_registry().loader(disk_loader());
    let doc = Document::open_at_with_loader(
        &user_src,
        &name,
        base_dir.clone(),
        &crate::build::wdoc_environment(),
        loader,
    )
    .map_err(|e| PdfError::Parse(Report::new(e)))?;

    let errs = crate::build::schema_errors(&doc);
    if !errs.is_empty() {
        let n = errs.len();
        let src = NamedSource::new(name.clone(), user_src.clone());
        for e in &errs {
            let report = Report::new(e.clone()).with_source_code(src.clone());
            eprintln!("{report:?}");
        }
        return Err(PdfError::Schema(n));
    }

    // The schema-level rendering contract (see `contract_errors`) — fail
    // like a schema violation.
    let reserved = crate::build::contract_errors(&doc);
    if !reserved.is_empty() {
        let n = reserved.len();
        let src = NamedSource::new(name.clone(), user_src.clone());
        for r in reserved {
            eprintln!("{:?}", r.with_source_code(src.clone()));
        }
        return Err(PdfError::Schema(n));
    }

    let site_blocks: Vec<Block> = doc.blocks().filter(|b| b.kind() == "site").collect();
    let all_pages = crate::build::collect_pages(&doc).map_err(|e| match e {
        crate::build::BuildError::BadPage(m) => PdfError::BadDoc(m),
        _ => PdfError::BadDoc("could not collect pages".into()),
    })?;
    if all_pages.is_empty() {
        return Err(PdfError::BadDoc("no `page` blocks to render".into()));
    }
    let specs = crate::build::collect_site_specs(&site_blocks, &all_pages)
        .map_err(|_| PdfError::BadDoc("could not group pages into sites".into()))?;

    // Which sites to render: a `--site` filter, else every declared site.
    let build_set: Vec<&crate::build::SiteSpec> = match site_filter {
        Some(want) => {
            let chosen: Vec<_> = specs
                .iter()
                .filter(|s| s.name.as_deref() == Some(want))
                .collect();
            if chosen.is_empty() {
                return Err(PdfError::BadDoc(format!("unknown site \"{want}\"")));
            }
            chosen
        }
        None => specs.iter().collect(),
    };

    // The inline-pattern engine (shared across sites — internal page links
    // aren't annotated yet, so per-site cross-link context isn't needed).
    let page_names: HashSet<String> = all_pages.iter().filter_map(page_label).collect();
    let icons = IconRegistry::load(&doc);
    let tilesets = match TilesetRegistry::load(&doc, base_dir.as_deref()) {
        Ok(t) => t,
        Err(crate::build::BuildError::Tileset(m)) => return Err(PdfError::BadDoc(m)),
        Err(_) => return Err(PdfError::BadDoc("tileset load failed".into())),
    };
    let images = ImageRegistry::new(base_dir.clone());
    let videos = crate::video::VideoRegistry::new(base_dir.clone());
    let files = crate::file::FileRegistry::new(base_dir.clone());
    let patterns = InlinePatterns::load(
        &doc,
        page_names,
        None,
        String::new(),
        BTreeMap::new(),
        BTreeMap::new(),
        icons,
        tilesets,
        images,
        videos,
        files,
        crate::inline::Backend::Pdf,
    );

    let geom = Geometry::new(page_size);
    let mut book = text::FontBook::new();
    let palette = palette::Palette::default();
    // The document's own `class` rules, so custom-coloured diagram shapes pick
    // up their fills inside embedded SVG (usvg applies them via `style_sheet`).
    let class_css: String = doc
        .blocks()
        .filter(|b| b.kind() == "class")
        .filter_map(|b| crate::render::render_class(&b))
        .map(|rule| rule.text)
        .collect();
    // Diagram card boxes match the themed web `.wdoc-card` (bg-alt fill,
    // border stroke). PDF is always-light, so resolve the site theme's light
    // palette; an unthemed doc keeps the neutral white/grey default.
    let (card_fill, card_stroke) = match site_blocks.first() {
        Some(site) => {
            let theme =
                crate::render::field_symbol(site, "theme").unwrap_or_else(|| "forge".to_string());
            let accent =
                crate::render::field_symbol(site, "accent").unwrap_or_else(|| "blue".to_string());
            let roles = crate::render::resolve_roles(&doc, &theme, &accent, "light");
            (roles.bg_alt, roles.border)
        }
        None => ("#ffffff".to_string(), "#cccccc".to_string()),
    };
    let embedder = svg_embed::SvgEmbedder::new(
        &palette,
        &class_css,
        patterns.icons(),
        patterns.images(),
        patterns.tilesets(),
        card_fill,
        card_stroke,
    );

    fs::create_dir_all(out_dir)
        .map_err(|e| PdfError::Io(e, format!("create_dir_all {}", out_dir.display())))?;
    let stem = file.file_stem().map_or_else(
        || "document".to_string(),
        |s| s.to_string_lossy().into_owned(),
    );

    // Clear any routing error / render warnings / embed error stranded by
    // an earlier pass so stale messages can't leak into this one.
    let _ = crate::render::take_route_error();
    let _ = crate::render::take_render_warnings();
    let _ = svg_embed::take_embed_error();
    let (result, eval_err) = crate::render::scoped_eval_errors(|| -> Result<usize, PdfError> {
        let mut written = 0;
        for spec in build_set {
            // Wireframe (`wf_*`) elements in this site's pages bake from its UI
            // theme. The shared `patterns` is updated per site (interior mutability;
            // the embedder borrows it immutably for the whole run).
            patterns.set_ui_theme(crate::render::resolve_ui_theme(spec.block.as_ref()));
            // Site name + template kind for `@only`/`@except` block visibility.
            let default_template = spec
                .block
                .as_ref()
                .and_then(|b| crate::render::field_symbol(b, "default_template"));
            patterns.set_site_context(spec.name.clone(), default_template);
            // One physical page per page block, ordered by the site TOC (start page
            // first, then TOC chapters, then any remaining pages in source order).
            let ordered = order_site_pages(spec);
            let sections: Vec<Vec<ir::BlockNode>> = ordered
                .iter()
                .map(|p| collect::collect_page(&doc, p, &patterns, base_dir.as_deref()))
                .collect();

            // A site with an explicit `title` gets a cover page.
            let explicit_title = spec
                .block
                .as_ref()
                .and_then(|b| crate::render::field_utf8(b, "title"));
            let title = explicit_title
                .clone()
                .or_else(|| spec.name.clone())
                .unwrap_or_else(|| stem.clone());
            // Internal links / outline destinations are shifted past the optional
            // cover page (the cover is a physical page but not in the laid pages).
            let offset = usize::from(explicit_title.is_some());

            // The site's table of contents (the book sidebar's data). When present,
            // the PDF gets a printed "Contents" page at the front and a reader
            // outline; absent, the output is unchanged.
            let toc_nodes = spec
                .block
                .as_ref()
                .map(crate::render::read_toc)
                .unwrap_or_default();

            let (laid, dests, outline) = if toc_nodes.is_empty() {
                let (laid, section_starts) = layout::layout(&sections, &mut book, &embedder, &geom);
                let mut dests: HashMap<String, usize> = HashMap::new();
                for (i, page) in ordered.iter().enumerate() {
                    if let (Some(name), Some(&start)) = (page_label(page), section_starts.get(i)) {
                        dests.insert(name, start + offset);
                    }
                }
                (laid, dests, None)
            } else {
                // Pass 1: lay out the content alone to learn each page's position
                // among the content pages (0-based, cover excluded).
                let (content_pages, content_starts) =
                    layout::layout(&sections, &mut book, &embedder, &geom);
                let mut content_index: HashMap<String, usize> = HashMap::new();
                for (i, page) in ordered.iter().enumerate() {
                    if let (Some(name), Some(&start)) = (page_label(page), content_starts.get(i)) {
                        content_index.insert(name, start);
                    }
                }

                // Flatten the toc into (depth, title, page) in source order.
                let mut flat: Vec<(u8, String, Option<String>)> = Vec::new();
                flatten_toc_entries(&toc_nodes, 0, &mut flat);

                // The printed numbers depend on the contents page count `T`, which
                // depends only on the entry count (each entry is one fixed-slot
                // line), not the number values — so build to learn `T`, then rebuild
                // with the real numbers. Iterate to a fixpoint (cap as a guard).
                let mut t = 1usize;
                let mut toc_pages: Vec<layout::LaidOutPage> = Vec::new();
                for _ in 0..4 {
                    let toc_section = build_toc_section(&flat, &content_index, t);
                    let (laid, _) = layout::layout(&[toc_section], &mut book, &embedder, &geom);
                    let nt = laid.len().max(1);
                    let done = nt == t;
                    toc_pages = laid;
                    t = nt;
                    if done {
                        break;
                    }
                }
                let t = toc_pages.len();

                // Final page index = `T` contents pages ahead, then the cover.
                let mut dests: HashMap<String, usize> = HashMap::new();
                for (name, &start) in &content_index {
                    dests.insert(name.clone(), start + t + offset);
                }
                let outline = build_outline_tree(&toc_nodes, &dests);

                let mut laid = toc_pages;
                laid.extend(content_pages);
                (laid, dests, Some(outline))
            };

            let bytes = paint::paint(
                &laid,
                &mut book,
                &geom,
                &title,
                explicit_title.is_some(),
                &dests,
                outline,
            )
            .map_err(|e| PdfError::Render(format!("{e:?}")))?;

            // `<site>.pdf` per named site; `<stem>.pdf` for an unnamed/default site.
            let file_stem = spec.name.clone().unwrap_or_else(|| stem.clone());
            let out = out_dir.join(format!("{file_stem}.pdf"));
            fs::write(&out, bytes)
                .map_err(|e| PdfError::Io(e, format!("write {}", out.display())))?;
            written += 1;
        }
        Ok(written)
    });
    if let Some((e, src)) = eval_err {
        return Err(PdfError::eval(e, src));
    }
    // An unroutable diagram edge surfaces after the eval check, mirroring
    // the HTML build.
    if let Some(msg) = crate::render::take_route_error() {
        return Err(PdfError::Render(msg));
    }
    // An SVG that failed to embed means a diagram is missing from the PDF
    // — data loss, not a degraded render — so it fails the build too.
    if let Some(msg) = svg_embed::take_embed_error() {
        return Err(PdfError::Render(msg));
    }
    result
}

/// Order a site's pages for a continuous PDF: the `start` page first, then
/// pages named by the site's `toc` (depth-first), then any remaining pages in
/// source order.
fn order_site_pages<'a>(spec: &'a crate::build::SiteSpec<'a>) -> Vec<&'a Block<'a>> {
    let mut ordered: Vec<&Block> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let push = |p: &'a Block<'a>, seen: &mut HashSet<String>, ordered: &mut Vec<&'a Block<'a>>| {
        if let Some(n) = page_label(p) {
            if seen.insert(n) {
                ordered.push(p);
            }
        } else {
            ordered.push(p);
        }
    };

    if let Some(start) = spec
        .pages
        .iter()
        .find(|p| crate::render::field_bool(p, "start") == Some(true))
    {
        push(start, &mut seen, &mut ordered);
    }
    if let Some(site) = &spec.block {
        let mut toc_names = Vec::new();
        flatten_toc(&crate::render::read_toc(site), &mut toc_names);
        for name in toc_names {
            if !seen.contains(&name)
                && let Some(p) = spec
                    .pages
                    .iter()
                    .find(|p| page_label(p).as_deref() == Some(&name))
            {
                push(p, &mut seen, &mut ordered);
            }
        }
    }
    for p in &spec.pages {
        push(p, &mut seen, &mut ordered);
    }
    ordered
}

/// Flatten a TOC tree into page names, depth-first in source order.
fn flatten_toc(nodes: &[crate::render::TocNode], out: &mut Vec<String>) {
    for node in nodes {
        if let Some(page) = &node.page {
            out.push(page.clone());
        }
        flatten_toc(&node.children, out);
    }
}

/// Flatten the toc into `(depth, title, page)` rows, depth-first in source
/// order, for the printed contents page.
fn flatten_toc_entries(
    nodes: &[crate::render::TocNode],
    depth: u8,
    out: &mut Vec<(u8, String, Option<String>)>,
) {
    for node in nodes {
        out.push((depth, node.title.clone(), node.page.clone()));
        flatten_toc_entries(&node.children, depth + 1, out);
    }
}

/// Build the contents section: a "Contents" heading plus a [`ir::BlockNode::Toc`]
/// whose entries carry their resolved page number. `t` is the contents page
/// count, so a chapter's printed (footer-matching) number is `start + t + 1`.
fn build_toc_section(
    flat: &[(u8, String, Option<String>)],
    content_index: &HashMap<String, usize>,
    t: usize,
) -> Vec<ir::BlockNode> {
    let entries = flat
        .iter()
        .map(|(depth, title, page)| {
            let number = page
                .as_ref()
                .and_then(|p| content_index.get(p))
                .map(|&start| (start + t + 1).to_string())
                .unwrap_or_default();
            // Keep the link only when the page exists in this site.
            let page = page
                .as_ref()
                .filter(|p| content_index.contains_key(p.as_str()))
                .cloned();
            ir::TocLine {
                depth: *depth,
                title: title.clone(),
                page,
                number,
            }
        })
        .collect();
    vec![
        ir::BlockNode::Heading {
            level: 1,
            runs: vec![ir::InlineRun::Text {
                text: "Contents".to_string(),
                style: ir::TextStyle::heading(),
            }],
        },
        ir::BlockNode::Toc { entries },
    ]
}

/// Build the PDF outline (reader bookmarks) from the toc, resolving each node's
/// destination through `dests`. Top-level (and any parent) nodes open expanded.
fn build_outline_tree(
    nodes: &[crate::render::TocNode],
    dests: &HashMap<String, usize>,
) -> krilla::outline::Outline {
    let mut outline = krilla::outline::Outline::new();
    for node in nodes {
        if let Some(child) = build_outline_node(node, dests) {
            outline.push_child(child);
        }
    }
    outline
}

/// One outline node and its children. A node points at its own page, or — for a
/// grouping heading with no page — its first descendant page; a node whose whole
/// subtree resolves to no page is dropped (krilla requires a destination).
fn build_outline_node(
    node: &crate::render::TocNode,
    dests: &HashMap<String, usize>,
) -> Option<krilla::outline::OutlineNode> {
    use krilla::destination::XyzDestination;
    use krilla::geom::Point;
    use krilla::outline::OutlineNode;

    let page_idx = first_dest(node, dests)?;
    let mut on = OutlineNode::new(
        node.title.clone(),
        XyzDestination::new(page_idx, Point::from_xy(0.0, 0.0)),
    );
    let mut any_child = false;
    for child in &node.children {
        if let Some(c) = build_outline_node(child, dests) {
            on.push_child(c);
            any_child = true;
        }
    }
    Some(if any_child { on.with_open(true) } else { on })
}

/// The first resolvable destination at or below `node` (self first, then
/// children depth-first).
fn first_dest(node: &crate::render::TocNode, dests: &HashMap<String, usize>) -> Option<usize> {
    if let Some(p) = &node.page
        && let Some(&i) = dests.get(p)
    {
        return Some(i);
    }
    node.children.iter().find_map(|c| first_dest(c, dests))
}

/// A page block's label (its name), if any.
fn page_label(page: &Block<'_>) -> Option<String> {
    match page.labels().ok()?.into_iter().next()? {
        Value::Identifier(s) | Value::Utf8(s) | Value::Symbol(s) => Some(s),
        _ => None,
    }
}
