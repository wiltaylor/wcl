//! Pure-Rust PDF backend for wdoc (`wcl wdoc pdf`).
//!
//! Reuses the existing fundamentals lowering to build a small, paint-agnostic
//! [`ir`] block model, lays it out and paginates it ([`layout`]), and paints it
//! to a PDF with [`krilla`] ([`paint`]) — no browser, no external tools. SVG
//! content (diagrams, charts, math) is embedded vector-preserving via
//! krilla-svg in a later phase.
//!
//! At this phase the backend renders prose (headings + paragraphs) across A4 /
//! US-Letter pages with a running header and footer page numbers, writing one
//! PDF per document.

mod collect;
pub(crate) mod ir;
mod layout;
mod page;
mod paint;
mod palette;
mod svg_embed;
mod text;

use std::collections::{BTreeMap, HashSet};
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

/// Render `file` to a PDF in `out_dir`. Returns the number of PDFs written.
///
/// `site_filter`, when set, names the output file (full per-site assembly lands
/// in a later phase); otherwise the source file stem is used.
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

    let pages: Vec<Block> = doc.blocks().filter(|b| b.kind() == "page").collect();
    if pages.is_empty() {
        return Err(PdfError::BadDoc("no `page` blocks to render".into()));
    }

    // Build the inline-pattern engine so bold/italic/code/links resolve as on
    // the HTML path. A single combined document (no per-site split yet) means
    // empty cross-site maps; bare `[text](page)` links resolve against the
    // document's own page names.
    let page_names: HashSet<String> = pages.iter().filter_map(page_label).collect();
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

    // Each page block starts on a fresh physical page; content paginates within.
    let sections: Vec<Vec<ir::BlockNode>> = pages
        .iter()
        .map(|p| collect::collect_page(&doc, p, &patterns, base_dir.as_deref()))
        .collect();

    let geom = Geometry::new(page_size);
    let mut book = text::FontBook::new();
    let palette = palette::Palette::default();
    let embedder = svg_embed::SvgEmbedder::new(&palette);
    let laid = layout::layout(&sections, &mut book, &embedder, &geom);

    let title = site_filter
        .map(str::to_string)
        .or_else(|| file.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "document".to_string());

    let bytes = paint::paint(&laid, &mut book, &geom, &title)
        .map_err(|e| PdfError::Render(format!("{e:?}")))?;

    fs::create_dir_all(out_dir)
        .map_err(|e| PdfError::Io(e, format!("create_dir_all {}", out_dir.display())))?;
    let stem = site_filter
        .map(str::to_string)
        .or_else(|| file.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "document".to_string());
    let out = out_dir.join(format!("{stem}.pdf"));
    fs::write(&out, bytes).map_err(|e| PdfError::Io(e, format!("write {}", out.display())))?;
    Ok(1)
}

/// A page block's label (its name), if any.
fn page_label(page: &Block<'_>) -> Option<String> {
    match page.labels().ok()?.into_iter().next()? {
        Value::Identifier(s) | Value::Utf8(s) | Value::Symbol(s) => Some(s),
        _ => None,
    }
}
