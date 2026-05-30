//! Pure-Rust PDF backend for wdoc (`wcl wdoc pdf`).
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
use wcl_lang::{Block, Document, Environment, Value, disk_loader};

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
    pub width: f32,
    pub height: f32,
    pub margin_x: f32,
    pub margin_top: f32,
    pub margin_bottom: f32,
}

impl Geometry {
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

    pub(crate) fn content_left(&self) -> f32 {
        self.margin_x
    }
    pub(crate) fn content_top(&self) -> f32 {
        self.margin_top
    }
    pub(crate) fn content_width(&self) -> f32 {
        self.width - 2.0 * self.margin_x
    }
    pub(crate) fn content_height(&self) -> f32 {
        self.height - self.margin_top - self.margin_bottom
    }
}

/// Errors from PDF generation. Mirrors `build::BuildError`'s shape so the CLI
/// maps them to the same exit codes.
pub enum PdfError {
    Io(std::io::Error, String),
    Parse(Report),
    Schema(usize),
    BadDoc(String),
    Render(String),
}

impl PdfError {
    pub fn report(&self) {
        match self {
            Self::Io(e, ctx) => eprintln!("{ctx}: {e}"),
            Self::Parse(r) => eprintln!("{r:?}"),
            Self::Schema(n) => eprintln!("{n} schema violation{}", if *n == 1 { "" } else { "s" }),
            Self::BadDoc(msg) => eprintln!("{msg}"),
            Self::Render(msg) => eprintln!("pdf render failed: {msg}"),
        }
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
        &Environment::new(),
        loader,
    )
    .map_err(|e| PdfError::Parse(Report::new(e)))?;

    let errs = doc.schema_errors();
    if !errs.is_empty() {
        let n = errs.len();
        let src = NamedSource::new(name.clone(), user_src.clone());
        for e in &errs {
            let report = Report::new(e.clone()).with_source_code(src.clone());
            eprintln!("{report:?}");
        }
        return Err(PdfError::Schema(n));
    }

    let site_blocks: Vec<Block> = doc.blocks().filter(|b| b.kind() == "site").collect();
    let all_pages: Vec<Block> = doc.blocks().filter(|b| b.kind() == "page").collect();
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
        .collect();
    let embedder = svg_embed::SvgEmbedder::new(
        &palette,
        &class_css,
        patterns.icons(),
        patterns.images(),
        patterns.tilesets(),
    );

    fs::create_dir_all(out_dir)
        .map_err(|e| PdfError::Io(e, format!("create_dir_all {}", out_dir.display())))?;
    let stem = file.file_stem().map_or_else(
        || "document".to_string(),
        |s| s.to_string_lossy().into_owned(),
    );

    let mut written = 0;
    for spec in build_set {
        // Wireframe (`wf_*`) elements in this site's pages bake from its UI
        // theme. The shared `patterns` is updated per site (interior mutability;
        // the embedder borrows it immutably for the whole run).
        patterns.set_ui_theme(crate::render::resolve_ui_theme(spec.block.as_ref()));
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

        let (laid, section_starts) = layout::layout(&sections, &mut book, &embedder, &geom);

        // Map each page name to its physical page index (shifted past the cover
        // page when present) so internal `[text](page)` links become PDF jumps.
        let offset = usize::from(explicit_title.is_some());
        let mut dests: HashMap<String, usize> = HashMap::new();
        for (i, page) in ordered.iter().enumerate() {
            if let (Some(name), Some(&start)) = (page_label(page), section_starts.get(i)) {
                dests.insert(name, start + offset);
            }
        }

        let bytes = paint::paint(
            &laid,
            &mut book,
            &geom,
            &title,
            explicit_title.is_some(),
            &dests,
        )
        .map_err(|e| PdfError::Render(format!("{e:?}")))?;

        // `<site>.pdf` per named site; `<stem>.pdf` for an unnamed/default site.
        let file_stem = spec.name.clone().unwrap_or_else(|| stem.clone());
        let out = out_dir.join(format!("{file_stem}.pdf"));
        fs::write(&out, bytes).map_err(|e| PdfError::Io(e, format!("write {}", out.display())))?;
        written += 1;
    }
    Ok(written)
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

/// A page block's label (its name), if any.
fn page_label(page: &Block<'_>) -> Option<String> {
    match page.labels().ok()?.into_iter().next()? {
        Value::Identifier(s) | Value::Utf8(s) | Value::Symbol(s) => Some(s),
        _ => None,
    }
}
