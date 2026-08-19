//! Inline-text pattern engine for spans.
//!
//! Each `@block("inline_pattern")` declared in the document carries a
//! regex string and a `to_span` function. At build time we enumerate
//! them, compile each regex once, and apply the engine to every
//! `span` block's text: tokenize left-to-right, take the
//! earliest-starting match across all patterns, hand the captures
//! to the user's `to_span`, and render whatever `InlineSpan` variants
//! come back. Each match's text fields are re-tokenized so a Bold
//! match can contain an Italic span inside.
//!
//! Built-in patterns ship in `wdoc.wcl` (bold / italic / code /
//! link); user `.wcl` files can declare more.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;

use regex::Regex;
use wcl_lang::{Document, FnValue, Value, VariantPayload};

use crate::blocks::diagram::tileset::TilesetRegistry;
use crate::blocks::file::FileRegistry;
use crate::blocks::icons::IconRegistry;
use crate::blocks::image::ImageRegistry;
use crate::blocks::video::VideoRegistry;
use crate::html::render_styles;
use crate::pdf::ir::{FontFamily, InlineRun, TextStyle};
use crate::render::escape_html;

/// Maximum recursion depth when re-tokenizing a match's text
/// fields. Keeps a self-referential pattern from blowing the
/// stack.
const MAX_DEPTH: usize = 8;

/// Which output backend a render pass targets. Carried on
/// [`InlinePatterns`] so the block-visibility predicate
/// ([`crate::visibility::block_visible`]) can honour the `backends=[…]`
/// axis of `@only` / `@except`, and so [`crate::native`] can refuse a
/// native block on a target it doesn't cover.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Backend {
    /// The HTML site renderer.
    Html,
    /// The PDF renderer.
    Pdf,
    /// The Markdown renderer.
    Markdown,
}

impl Backend {
    /// The symbol an author writes in a `backends=[…]` axis
    /// (`:html` / `:pdf` / `:markdown`).
    pub(crate) fn symbol(self) -> &'static str {
        match self {
            Backend::Html => "html",
            Backend::Pdf => "pdf",
            Backend::Markdown => "markdown",
        }
    }
}

/// One site's inline-markup rules, plus the link context needed to
/// resolve `[text](target)` against that site's pages.
pub(crate) struct InlinePatterns {
    /// The patterns, in declaration order — the first match at a
    /// position wins.
    compiled: Vec<CompiledPattern>,
    /// Named structured styles, rendered once per site rather than resolving
    /// and re-rendering their blocks for every page template invocation.
    styles: BTreeMap<String, crate::html::RenderedCss>,
    /// Names of every `page` in the current site, used by `render_link`
    /// to recognise bare `[text](page_name)` references and rewrite them
    /// to `page_name.html`.
    page_names: HashSet<String>,
    /// The current site's name (`None` for a single unnamed / synthetic
    /// site) and its output prefix this build (`""` at the root, else
    /// `"<name>/"`), used to resolve `[text](site:page)` cross-site links.
    current_site: Option<String>,
    /// URL prefix of the current site (`""` at the root, else
    /// `"<name>/"`).
    current_prefix: String,
    /// Every declared site → its page-name set, and → its URL prefix in
    /// the full layout (`""` for the root site, else `"<name>/"`). A
    /// `site:page` link validates against these and builds a relative
    /// href from `current_prefix` to the target.
    site_pages: BTreeMap<String, HashSet<String>>,
    /// Every site → its URL prefix, for building cross-site hrefs.
    site_prefix: BTreeMap<String, String>,
    /// Bare hrefs that didn't match a known page during rendering.
    /// Build collects these after the page loop and turns them
    /// into a `BuildError::BadLink`.
    link_errors: RefCell<Vec<String>>,
    /// Icon registry, resolving the built-in `:name:` pattern's
    /// `InlineSpan::Icon` against the declared `iconset`s.
    icons: IconRegistry,
    /// Tileset registry. Not used by inline text — carried here so the
    /// SVG render path reaches it (via `tilesets()`) without threading a
    /// new param through `render_block`, mirroring how `icons` rides
    /// along.
    tilesets: TilesetRegistry,
    /// Image registry. Carried alongside `tilesets` so both the page
    /// (`<img>`) and diagram (`<image>`) render paths reach it via
    /// `images()`; populated lazily as `image` blocks are rendered.
    images: ImageRegistry,
    /// Video registry. Carried alongside `images` so the page `video`
    /// render path reaches it via `videos()`; populated lazily as `video`
    /// blocks are rendered, copying any local file / poster into `_wdoc/`.
    videos: VideoRegistry,
    /// File registry. Carried alongside `images` so the `file` block render
    /// path reaches it via `files()`; populated lazily as `file` blocks are
    /// rendered, copying any local file into its `dir`.
    files: FileRegistry,
    /// The current site's output directory (set per build by `set_output_dir`).
    /// Rides here so the `markdown_source` block reaches it — its Markdown
    /// lowering writes any nested diagram SVGs under `<output_dir>/_wdoc/`,
    /// exactly like the Markdown backend's page emitter.
    output_dir: RefCell<std::path::PathBuf>,
    /// The current site's resolved UI/application theme — what `wf_*`
    /// wireframe elements bake their colours from (separate from the
    /// document theme). Set per site by the build (`set_ui_theme`), it
    /// rides here so the wireframe renderer reaches the right site's theme
    /// without threading a new param through `render_block`, like `icons`.
    /// `RefCell` because the PDF path shares one `InlinePatterns` across
    /// sites (the embedder borrows it immutably for the whole run) and
    /// updates the theme per site.
    ui_theme: RefCell<crate::render::UiTheme>,
    /// The backend this pass targets (fixed at construction) plus the current
    /// site's name and template kind (`:webpage` / `:book` / `:presentation`),
    /// read by the block-visibility predicate to honour `@only` / `@except`.
    /// The site/template fields are `RefCell` and updated per site via
    /// `set_site_context` — mirroring `ui_theme`, so the single PDF
    /// `InlinePatterns` (shared, immutably borrowed across sites) still tracks
    /// which site it is currently emitting.
    backend: Backend,
    /// Site currently being emitted, for the visibility predicate.
    /// `RefCell` because the PDF renderer shares one `InlinePatterns`
    /// immutably across sites.
    vis_site: RefCell<Option<String>>,
    /// Template currently being emitted, for the visibility predicate.
    vis_template: RefCell<Option<String>>,
}

/// One inline-markup rule: the regex that recognises it and the WCL
/// function that turns a match into a span value.
struct CompiledPattern {
    /// Pattern that recognises the markup.
    regex: Regex,
    /// Skip matches touching a word character on either side (the
    /// `boundary = true` block field) — intraword `_` stays literal.
    boundary: bool,
    /// Function mapping the captures to a span value.
    to_span: FnValue,
}

/// The earliest pattern match at or after a scan position, as returned by
/// [`InlinePatterns::find_next`]: the match byte range, the index of the
/// winning compiled pattern, and its capture-group strings.
struct Match {
    /// Byte offset where the match begins.
    start: usize,
    /// Byte offset one past its end.
    end: usize,
    /// Index of the winning pattern.
    pat_idx: usize,
    /// Capture-group strings, group 1 first.
    caps: Vec<String>,
}

/// One token produced by [`InlinePatterns::tokenize`]: either a run of
/// literal text, or a matched inline-pattern span (a `Value::Variant`) with
/// the depth at which it was found, so each backend can recurse into the
/// span's text fields at the right level.
enum InlineToken<'a> {
    /// A run of text no pattern matched.
    Literal(&'a str),
    /// A matched span value, with the depth it was found at so a backend
    /// recurses into its text fields at the right level.
    Span(&'a Value, usize),
}

impl InlinePatterns {
    /// Enumerate every `@block("inline_pattern")` at the document
    /// root, compile its regex, and capture its `to_span` function.
    /// Patterns whose regex fails to compile or whose `to_span`
    /// isn't a function are silently skipped — schema validation
    /// flags those separately.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn load(
        doc: &Document,
        page_names: HashSet<String>,
        current_site: Option<String>,
        current_prefix: String,
        site_pages: BTreeMap<String, HashSet<String>>,
        site_prefix: BTreeMap<String, String>,
        icons: IconRegistry,
        tilesets: TilesetRegistry,
        images: ImageRegistry,
        videos: VideoRegistry,
        files: FileRegistry,
        backend: Backend,
    ) -> Self {
        let mut compiled = Vec::new();
        for block in doc.blocks() {
            if block.kind() != "inline_pattern" {
                continue;
            }
            let Some(pattern_field) = block.field("pattern") else {
                continue;
            };
            let Ok(Value::Utf8(pattern_src) | Value::Ascii(pattern_src)) = pattern_field.value()
            else {
                continue;
            };
            let Ok(regex) = Regex::new(pattern_src) else {
                continue;
            };
            let Some(to_span_field) = block.field("to_span") else {
                continue;
            };
            let Ok(Value::Function(fv)) = to_span_field.value() else {
                continue;
            };
            let boundary = block
                .field("boundary")
                .and_then(|f| f.value().ok())
                .is_some_and(|v| matches!(v, Value::Bool(true)));
            compiled.push(CompiledPattern {
                regex,
                boundary,
                to_span: fv.clone(),
            });
        }
        InlinePatterns {
            compiled,
            styles: render_styles(doc),
            page_names,
            current_site,
            current_prefix,
            site_pages,
            site_prefix,
            link_errors: RefCell::new(Vec::new()),
            icons,
            tilesets,
            images,
            videos,
            files,
            output_dir: RefCell::new(std::path::PathBuf::new()),
            ui_theme: RefCell::new(crate::render::UiTheme::default()),
            backend,
            vis_site: RefCell::new(None),
            vis_template: RefCell::new(None),
        }
    }

    /// CSS for a named structured style referenced by an `Html::Style`.
    pub(crate) fn style(&self, name: &str) -> Option<&str> {
        self.styles.get(name).map(|style| style.text.as_str())
    }

    /// Set the current site's name and template kind for block-visibility
    /// filtering (the build calls this per site before rendering its pages).
    /// Interior mutability so it works on the shared, immutably-borrowed PDF
    /// `InlinePatterns`, like `set_ui_theme`.
    pub(crate) fn set_site_context(&self, site: Option<String>, template: Option<String>) {
        *self.vis_site.borrow_mut() = site;
        *self.vis_template.borrow_mut() = template;
    }

    /// The backend this render pass targets — the current value of the
    /// `@only`/`@except` `backends=` axis.
    pub(crate) fn backend(&self) -> Backend {
        self.backend
    }

    /// The current site's name, for the `@only`/`@except` `sites=` axis
    /// (`None` for the synthetic / unnamed site).
    pub(crate) fn vis_site(&self) -> Option<String> {
        self.vis_site.borrow().clone()
    }

    /// The current site's template kind, for the `templates=` axis
    /// (`None` when the site declares no `default_template`).
    pub(crate) fn vis_template(&self) -> Option<String> {
        self.vis_template.borrow().clone()
    }

    /// Set the current site's resolved UI/application theme (the build calls
    /// this per site before rendering its pages). Interior mutability so it
    /// works on the shared, immutably-borrowed PDF `InlinePatterns`.
    pub(crate) fn set_ui_theme(&self, ui_theme: crate::render::UiTheme) {
        *self.ui_theme.borrow_mut() = ui_theme;
    }

    /// The current site's UI/application theme — the base a wireframe element
    /// bakes from (before any per-element `theme`/`accent`/`mode` override).
    pub(crate) fn ui_theme(&self) -> crate::render::UiTheme {
        self.ui_theme.borrow().clone()
    }

    /// The document's icon registry, threaded into the SVG render path
    /// (diagram `icon` blocks) via `render_diagram`.
    pub(crate) fn icons(&self) -> &IconRegistry {
        &self.icons
    }

    /// The document's tileset registry, threaded into the SVG render
    /// path (diagram `tilemap` blocks) via `render_diagram`.
    pub(crate) fn tilesets(&self) -> &TilesetRegistry {
        &self.tilesets
    }

    /// The document's image registry, threaded into both the page
    /// (`<img>`) and diagram (`<image>`) render paths.
    pub(crate) fn images(&self) -> &ImageRegistry {
        &self.images
    }

    /// The document's video registry, threaded into the page `video`
    /// render path. Holds every referenced local video file / poster so
    /// the build can copy them into `_wdoc/`.
    pub(crate) fn videos(&self) -> &VideoRegistry {
        &self.videos
    }

    /// The document's file registry, threaded into the `file` block render
    /// path (HTML + Markdown). Holds every referenced local file so the
    /// build can copy it into its `dir`.
    pub(crate) fn files(&self) -> &FileRegistry {
        &self.files
    }

    /// Record the current site's output directory so the `markdown_source`
    /// block can write the diagram SVGs its Markdown lowering produces into
    /// `<output_dir>/_wdoc/` (the same place the rest of the build's assets go).
    pub(crate) fn set_output_dir(&self, dir: std::path::PathBuf) {
        *self.output_dir.borrow_mut() = dir;
    }

    /// The current site's output directory (see [`set_output_dir`]).
    pub(crate) fn output_dir(&self) -> std::path::PathBuf {
        self.output_dir.borrow().clone()
    }

    /// Drain accumulated unknown-page link errors. Build calls
    /// this after every page has rendered and turns a non-empty
    /// result into a `BuildError::BadLink`.
    pub(crate) fn take_link_errors(&self) -> Vec<String> {
        self.link_errors.borrow_mut().drain(..).collect()
    }

    /// Tokenize `text` and emit HTML: literal text gets html-escaped,
    /// matched regions become whatever the pattern's `to_span`
    /// function returns. Text fields inside emitted InlineSpan
    /// variants are themselves passed back through `render` (with
    /// `depth + 1`) so patterns can nest. Beyond `MAX_DEPTH` levels,
    /// text is emitted as a literal — guards against pathological
    /// self-referential patterns.
    pub(crate) fn render(&self, doc: &Document, text: &str) -> String {
        self.render_inner(doc, text, 0)
    }

    /// Render inline markup to HTML, bounded by `depth` so a pattern
    /// that re-emits its own syntax cannot recurse forever.
    fn render_inner(&self, doc: &Document, text: &str, depth: usize) -> String {
        let mut out = String::new();
        self.tokenize(doc, text, depth, |tok| match tok {
            InlineToken::Literal(s) => out.push_str(&escape_html(s)),
            // A user fn that returned something unexpected leaves the
            // matched text as a Literal token, so it's still shown.
            InlineToken::Span(span, d) => out.push_str(&self.render_variant(doc, span, d)),
        });
        out
    }

    /// Shared left-to-right inline scan driving all three backends
    /// ([`render_inner`](Self::render_inner) / [`runs_inner`](Self::runs_inner)
    /// / [`markdown_inner`](Self::markdown_inner)). Walks `text`, handing each
    /// run of literal text and each matched pattern span to `emit`; the
    /// backend decides how to materialise them. Beyond `MAX_DEPTH` levels (or
    /// with no patterns) the whole input is emitted as a single literal,
    /// guarding against pathological self-referential patterns. A pattern
    /// whose `to_span` fn doesn't return a `list` (or matches empty) falls
    /// back to emitting the matched text literally — the zero-length guard
    /// also stops a pattern like `()` looping forever.
    fn tokenize(
        &self,
        doc: &Document,
        text: &str,
        depth: usize,
        mut emit: impl FnMut(InlineToken),
    ) {
        if depth >= MAX_DEPTH || self.compiled.is_empty() {
            emit(InlineToken::Literal(text));
            return;
        }
        let mut pos = 0usize;
        while pos < text.len() {
            let Some(m) = self.find_next(text, pos) else {
                emit(InlineToken::Literal(&text[pos..]));
                break;
            };
            if m.start > pos {
                emit(InlineToken::Literal(&text[pos..m.start]));
            }
            let pattern = &self.compiled[m.pat_idx];
            let args: Vec<Value> = m.caps.into_iter().map(Value::Utf8).collect();
            match doc.call_value(&pattern.to_span, &[Value::List(std::sync::Arc::new(args))]) {
                Ok(Value::List(spans)) => {
                    for span in std::sync::Arc::unwrap_or_clone(spans) {
                        emit(InlineToken::Span(&span, depth));
                    }
                }
                _ => emit(InlineToken::Literal(&text[m.start..m.end])),
            }
            pos = m.end;
            if m.end == m.start {
                if let Some(next_ch) = text[pos..].chars().next() {
                    let next_pos = pos + next_ch.len_utf8();
                    emit(InlineToken::Literal(&text[pos..next_pos]));
                    pos = next_pos;
                } else {
                    break;
                }
            }
        }
    }

    /// Scan every pattern starting at `pos`, return the earliest-
    /// starting match. Ties broken by compiled order — root-authored
    /// blocks enumerate before imported ones (`Document::blocks`), so
    /// a user pattern with the same syntax at the same position
    /// overrides the `wdoc.wcl` built-in.
    fn find_next(&self, text: &str, pos: usize) -> Option<Match> {
        // True when the byte position is flanked by a word character —
        // boundary-gated patterns must not start/end against one.
        let word_before = |at: usize| {
            text[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
        };
        let word_after = |at: usize| {
            text[at..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
        };
        let mut best: Option<Match> = None;
        for (i, pat) in self.compiled.iter().enumerate() {
            // Boundary-gated patterns rescan past rejected occurrences
            // (`_mode_` inside `safe_mode_password`), one char at a time,
            // until a properly flanked match (or none) is found.
            let mut search = pos;
            let caps = loop {
                let Some(caps) = pat.regex.captures_at(text, search) else {
                    break None;
                };
                let Some(m) = caps.get(0) else { break None };
                if pat.boundary && (word_before(m.start()) || word_after(m.end())) {
                    let step = text[m.start()..]
                        .chars()
                        .next()
                        .map(char::len_utf8)
                        .unwrap_or(1);
                    search = m.start() + step;
                    continue;
                }
                break Some(caps);
            };
            let Some(caps) = caps else {
                continue;
            };
            let m = caps.get(0)?;
            // Captures are guaranteed to be ≥ pos because of
            // `captures_at`'s contract.
            let start = m.start();
            let end = m.end();
            if best.as_ref().map(|b| start < b.start).unwrap_or(true) {
                let groups: Vec<String> = (0..caps.len())
                    .map(|g| {
                        caps.get(g)
                            .map(|m| m.as_str().to_string())
                            .unwrap_or_default()
                    })
                    .collect();
                best = Some(Match {
                    start,
                    end,
                    pat_idx: i,
                    caps: groups,
                });
            }
        }
        best
    }

    /// Structured twin of [`render`](Self::render) for the PDF backend:
    /// tokenize `text` and return a tree of [`InlineRun`]s instead of HTML.
    /// Bold / italic / code patterns lower to a `Plain` span carrying a
    /// `bold` / `italic` / `code` class, which maps to a [`TextStyle`]; links
    /// become [`InlineRun::Link`]. Icons and inline math fall back to literal
    /// text here — they become SVG runs in the SVG phase.
    pub(crate) fn render_runs(&self, doc: &Document, text: &str) -> Vec<InlineRun> {
        let mut out = Vec::new();
        self.runs_inner(doc, text, 0, TextStyle::body(), &mut out);
        out
    }

    /// Render inline markup to PDF runs, with the same depth bound.
    fn runs_inner(
        &self,
        doc: &Document,
        text: &str,
        depth: usize,
        style: TextStyle,
        out: &mut Vec<InlineRun>,
    ) {
        self.tokenize(doc, text, depth, |tok| match tok {
            InlineToken::Literal(s) => push_run(out, s, style),
            InlineToken::Span(span, d) => self.runs_variant(doc, span, d, style, out),
        });
    }

    /// Render one matched span value as PDF runs.
    fn runs_variant(
        &self,
        doc: &Document,
        value: &Value,
        depth: usize,
        style: TextStyle,
        out: &mut Vec<InlineRun>,
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
        match variant.as_str() {
            "Plain" => {
                let text = map_utf8(map, "text").unwrap_or_default();
                let classes = class_list(map);
                let st = apply_classes(style, &classes);
                // A code span is verbatim: emit one run without re-tokenizing,
                // so an embedded `_`/`*` pair isn't reinterpreted as emphasis
                // (mirrors the HTML / Markdown backends).
                if classes.iter().any(|c| c == "code") {
                    push_run(out, &text, st);
                } else {
                    self.runs_inner(doc, &text, depth + 1, st, out);
                }
            }
            "Link" => {
                let text = map_utf8(map, "text").unwrap_or_default();
                let href = self.resolve_href(&map_utf8(map, "href").unwrap_or_default());
                let st = apply_classes(style, &class_list(map));
                let mut runs = Vec::new();
                self.runs_inner(doc, &text, depth + 1, st, &mut runs);
                out.push(InlineRun::Link { runs, href });
            }
            // Icons / inline math become inline SVG objects (overlaid in the
            // text flow by the PDF layout pass). An unresolved icon falls back
            // to its literal `:name:` text.
            "Icon" => {
                let name = map_utf8(map, "name").unwrap_or_default();
                match self.icons.standalone(&name) {
                    Some(svg) => out.push(InlineRun::Object { svg }),
                    None => push_run(out, &format!(":{name}:"), style),
                }
            }
            "Math" => out.push(InlineRun::Object {
                svg: crate::blocks::math::render_inline_math(map),
            }),
            _ => {}
        }
    }

    /// Render one matched span value as HTML, dispatching on its variant.
    fn render_variant(&self, doc: &Document, value: &Value, depth: usize) -> String {
        let Value::Variant {
            variant, payload, ..
        } = value
        else {
            return String::new();
        };
        let VariantPayload::Record(map) = payload else {
            return String::new();
        };
        match variant.as_str() {
            "Plain" => self.render_plain(doc, map, depth),
            "Link" => self.render_link(doc, map, depth),
            "Icon" => self.render_icon(map),
            "Math" => crate::blocks::math::render_inline_math(map),
            _ => String::new(),
        }
    }

    /// Render an `InlineSpan::Icon`. Resolves the name against the icon
    /// registry; a miss (unknown name, or no declared iconsets) emits the
    /// literal `:name:` so a chance regex match in prose is harmless.
    fn render_icon(&self, map: &BTreeMap<String, Value>) -> String {
        let name = map_utf8(map, "name").unwrap_or_default();
        let classes = class_list(map);
        self.icons
            .resolve_inline(&name, &classes)
            .unwrap_or_else(|| escape_html(&format!(":{name}:")))
    }

    /// Render a plain styled span as HTML.
    fn render_plain(&self, doc: &Document, map: &BTreeMap<String, Value>, depth: usize) -> String {
        let text = map_utf8(map, "text").unwrap_or_default();
        // A code span is verbatim: emit its contents html-escaped without
        // re-tokenizing, so an embedded `_`/`*` pair (e.g. `reading_long_format`)
        // isn't reinterpreted as emphasis (mirrors the Markdown backend).
        let inner = if class_list(map).iter().any(|c| c == "code") {
            escape_html(&text)
        } else {
            self.render_inner(doc, &text, depth + 1)
        };
        let class_attr = class_attr(map);
        let mut out = format!("<span{class_attr}>");
        out.push_str(&inner);
        out.push_str("</span>");
        out
    }

    /// Render a link span, resolving a bare target against the current
    /// site's pages and recording it when nothing matches.
    fn render_link(&self, doc: &Document, map: &BTreeMap<String, Value>, depth: usize) -> String {
        let text = map_utf8(map, "text").unwrap_or_default();
        let href = map_utf8(map, "href").unwrap_or_default();
        let resolved = self.resolve_href(&href);
        let inner = self.render_inner(doc, &text, depth + 1);
        let class_attr = class_attr(map);
        let mut out = String::new();
        write!(
            out,
            "<a{class_attr} href=\"{}\">{inner}</a>",
            escape_html(&resolved)
        )
        .expect("write to String");
        out
    }

    /// Rewrite `href` for the rendered `<a href="...">`. External
    /// URLs (anything with a scheme, anchor-only, or path-relative
    /// prefix) pass through unchanged. A bare token (or `name#frag`)
    /// that matches a known page is rewritten to `<page>.html` with
    /// the fragment preserved. A bare token that doesn't match any
    /// page is recorded as a link error so build can fail.
    pub(crate) fn resolve_href(&self, href: &str) -> String {
        if is_external_href(href) {
            return href.to_string();
        }
        let (target, fragment) = match href.find('#') {
            Some(i) => (&href[..i], &href[i..]),
            None => (href, ""),
        };
        // `site:page` — a cross-site link. `page` names are identifiers
        // (no `:`), and `mailto:` / `http://` are already handled above,
        // so a `:` here unambiguously names another site.
        if let Some((site, page)) = target.split_once(':') {
            match self.site_pages.get(site) {
                Some(pages) if pages.contains(page) => {
                    if Some(site) == self.current_site.as_deref() {
                        return format!("{page}.html{fragment}");
                    }
                    // Walk up out of the current site's subdirectory (if
                    // any), then down into the target site's prefix.
                    let up = self.current_prefix.matches('/').count();
                    let prefix = self.site_prefix.get(site).map_or("", String::as_str);
                    return format!("{}{prefix}{page}.html{fragment}", "../".repeat(up));
                }
                Some(_) => self
                    .link_errors
                    .borrow_mut()
                    .push(format!("link to unknown page '{page}' in site '{site}'")),
                None => self
                    .link_errors
                    .borrow_mut()
                    .push(format!("link to unknown site '{site}'")),
            }
            return href.to_string();
        }
        if self.page_names.contains(target) {
            return format!("{target}.html{fragment}");
        }
        self.link_errors
            .borrow_mut()
            .push(format!("link to unknown page '{target}'"));
        href.to_string()
    }

    /// Markdown twin of [`render`](Self::render) for the Markdown backend:
    /// tokenize `text` and emit GitHub-flavoured Markdown rather than HTML.
    /// Bold / italic / code patterns become `**…**` / `_…_` / `` `…` ``;
    /// links become `[text](href)` with internal page hrefs pointed at the
    /// `.md` sibling; icons fall back to their literal `:name:`; inline
    /// equations stay as raw LaTeX in `$…$` (or `$$…$$` for display style),
    /// matching the block-equation policy of the Markdown target.
    pub(crate) fn render_markdown(&self, doc: &Document, text: &str) -> String {
        self.markdown_inner(doc, text, 0)
    }

    /// Render inline markup to Markdown, with the same depth bound.
    fn markdown_inner(&self, doc: &Document, text: &str, depth: usize) -> String {
        let mut out = String::new();
        self.tokenize(doc, text, depth, |tok| match tok {
            InlineToken::Literal(s) => out.push_str(&escape_md(s)),
            InlineToken::Span(span, d) => out.push_str(&self.markdown_variant(doc, span, d)),
        });
        out
    }

    /// Render one matched span value as Markdown.
    fn markdown_variant(&self, doc: &Document, value: &Value, depth: usize) -> String {
        let Value::Variant {
            variant, payload, ..
        } = value
        else {
            return String::new();
        };
        let VariantPayload::Record(map) = payload else {
            return String::new();
        };
        match variant.as_str() {
            "Plain" => {
                let text = map_utf8(map, "text").unwrap_or_default();
                let classes = class_list(map);
                // A code span is literal: emit it verbatim (no re-tokenizing,
                // no emphasis nesting — Markdown can't style inside code).
                if classes.iter().any(|c| c == "code") {
                    return md_code_span(&text);
                }
                let inner = self.markdown_inner(doc, &text, depth + 1);
                wrap_emphasis(&inner, &classes)
            }
            "Link" => {
                let text = map_utf8(map, "text").unwrap_or_default();
                let raw = map_utf8(map, "href").unwrap_or_default();
                let href = self.markdown_href(&raw);
                let inner = self.markdown_inner(doc, &text, depth + 1);
                format!("[{inner}]({href})")
            }
            "Icon" => format!(":{}:", map_utf8(map, "name").unwrap_or_default()),
            "Math" => {
                let latex = map_utf8(map, "latex").unwrap_or_default();
                let display = matches!(map.get("display"), Some(Value::Bool(true)));
                if display {
                    format!("$${latex}$$")
                } else {
                    format!("${latex}$")
                }
            }
            _ => String::new(),
        }
    }

    /// Resolve a link `href` for Markdown output. Reuses [`resolve_href`]
    /// (so a bad internal page link still records a build error), but points
    /// an internal page link at its `.md` sibling instead of `.html`;
    /// external / anchor / path-relative hrefs pass through unchanged.
    fn markdown_href(&self, href: &str) -> String {
        let resolved = self.resolve_href(href);
        if is_external_href(href) {
            return resolved;
        }
        let (path, frag) = match resolved.find('#') {
            Some(i) => (&resolved[..i], &resolved[i..]),
            None => (resolved.as_str(), ""),
        };
        match path.strip_suffix(".html") {
            Some(stem) => format!("{stem}.md{frag}"),
            None => resolved,
        }
    }
}

/// Push a non-empty styled text run.
fn push_run(out: &mut Vec<InlineRun>, text: &str, style: TextStyle) {
    if !text.is_empty() {
        out.push(InlineRun::Text {
            text: text.to_string(),
            style,
        });
    }
}

/// Fold the built-in inline-emphasis classes (`bold` / `italic` / `code`) into
/// a [`TextStyle`]. Other classes carry no PDF styling at this phase.
fn apply_classes(mut style: TextStyle, classes: &[String]) -> TextStyle {
    for c in classes {
        match c.as_str() {
            "bold" => style.bold = true,
            "italic" => style.italic = true,
            "code" => style.family = FontFamily::Mono,
            _ => {}
        }
    }
    style
}

/// Escape the Markdown-significant punctuation in a literal text run so
/// prose doesn't accidentally format. Intentionally conservative: `_` is
/// left alone (CommonMark treats intraword `_` as literal, and snake_case
/// identifiers are everywhere in this domain), so only the characters that
/// open emphasis / code / links from word boundaries are escaped.
fn escape_md(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if matches!(ch, '\\' | '`' | '*' | '[' | ']') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Wrap `inner` in the Markdown emphasis markers implied by `classes`
/// (`bold` → `**`, `italic` → `_`, both → `***`). No emphasis class ⇒
/// `inner` returned unchanged.
fn wrap_emphasis(inner: &str, classes: &[String]) -> String {
    let bold = classes.iter().any(|c| c == "bold");
    let italic = classes.iter().any(|c| c == "italic");
    let marker = match (bold, italic) {
        (true, true) => "***",
        (true, false) => "**",
        (false, true) => "_",
        (false, false) => return inner.to_string(),
    };
    format!("{marker}{inner}{marker}")
}

/// Emit `code` as a Markdown inline code span, widening the backtick fence
/// past the longest backtick run in the content and padding with a space
/// when it begins or ends with a backtick (CommonMark code-span rules).
fn md_code_span(code: &str) -> String {
    let mut longest = 0usize;
    let mut run = 0usize;
    for ch in code.chars() {
        if ch == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    let fence = "`".repeat(longest + 1);
    let pad = if code.starts_with('`') || code.ends_with('`') {
        " "
    } else {
        ""
    };
    format!("{fence}{pad}{code}{pad}{fence}")
}

/// Whether an href leaves the site — a scheme, a protocol-relative
/// prefix, or a mail link.
fn is_external_href(href: &str) -> bool {
    if href.starts_with('#') || href.starts_with('/') {
        return true;
    }
    if href.starts_with("./") || href.starts_with("../") {
        return true;
    }
    if href.contains("://") {
        return true;
    }
    for scheme in ["mailto:", "tel:", "data:", "javascript:"] {
        if href.starts_with(scheme) {
            return true;
        }
    }
    false
}

/// Read a `utf8` entry out of a span record.
fn map_utf8(map: &BTreeMap<String, Value>, name: &str) -> Option<String> {
    match map.get(name)? {
        Value::Utf8(s) | Value::Ascii(s) => Some(s.clone()),
        _ => None,
    }
}

/// Build the `class="…"` attribute from a span record's classes.
fn class_attr(map: &BTreeMap<String, Value>) -> String {
    let names = class_list(map);
    if names.is_empty() {
        return String::new();
    }
    format!(" class=\"{}\"", escape_html(&names.join(" ")))
}

/// Extract a span's `class: list<utf8>?` field as a plain list.
fn class_list(map: &BTreeMap<String, Value>) -> Vec<String> {
    let Some(Value::List(items)) = map.get("class") else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|v| match v {
            Value::Utf8(s) | Value::Ascii(s) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use wcl_lang::{Environment, disk_loader};

    /// Open a fixture through the embedded wdoc registry, so the built-in
    /// bold/italic/code/… patterns compile exactly as they do in a real
    /// build; `extra` appends user declarations after the stdlib import.
    fn open_wdoc(extra: &str) -> Document {
        let src = format!("import <wdoc.wcl>\n{extra}");
        let loader = crate::schema_registry().loader(disk_loader());
        Document::open_at_with_loader(&src, "inline-test.wcl", None, &Environment::new(), loader)
            .expect("open inline fixture")
    }

    /// Wire an `InlinePatterns` over `doc` with an empty site context —
    /// the engine under test only needs the compiled pattern table (and
    /// `pages` for the href-resolution tests).
    fn patterns_for(doc: &Document, pages: &[&str]) -> InlinePatterns {
        InlinePatterns::load(
            doc,
            pages.iter().map(|s| s.to_string()).collect(),
            None,
            String::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            IconRegistry::load(doc),
            // `BuildError` carries no Debug impl, so unwrap by hand;
            // fixtures declare no tilesets, so this can't fail.
            TilesetRegistry::load(doc, None)
                .unwrap_or_else(|_| panic!("tileset registry over an empty fixture")),
            ImageRegistry::new(None),
            VideoRegistry::new(None),
            FileRegistry::new(None),
            Backend::Html,
        )
    }

    fn render_builtin(text: &str) -> String {
        let doc = open_wdoc("");
        let pats = patterns_for(&doc, &[]);
        pats.render(&doc, text)
    }

    #[test]
    fn empty_input_renders_empty() {
        assert_eq!(render_builtin(""), "");
    }

    #[test]
    fn no_patterns_emits_whole_text_as_escaped_literal() {
        // A document that never imports the stdlib compiles zero patterns;
        // the engine must still escape (would-be delimiters stay literal).
        let doc = Document::open("", "empty.wcl").expect("open empty doc");
        let pats = patterns_for(&doc, &[]);
        assert_eq!(pats.render(&doc, "**a** <x>"), "**a** &lt;x&gt;");
    }

    #[test]
    fn matches_at_start_end_and_back_to_back_lose_no_text() {
        // Adjacent matches at both string edges: the scan must not skip or
        // duplicate the boundary bytes between them.
        assert_eq!(
            render_builtin("**a**_b_ and **z**"),
            "<span class=\"bold\">a</span><span class=\"italic\">b</span> \
             and <span class=\"bold\">z</span>"
        );
    }

    #[test]
    fn unclosed_delimiter_is_literal_text() {
        // No closing `**` / backtick ⇒ the regex simply never matches; the
        // opener must survive as prose rather than swallowing to EOF.
        assert_eq!(render_builtin("**never closed"), "**never closed");
        assert_eq!(render_builtin("start `still open"), "start `still open");
    }

    #[test]
    fn earliest_start_wins_and_match_text_is_retokenized() {
        // Italic opens at byte 0, bold at byte 3: the earlier start wins the
        // outer span, and its captured text re-runs the engine so the bold
        // nests inside.
        assert_eq!(
            render_builtin("_i **b** j_"),
            "<span class=\"italic\">i <span class=\"bold\">b</span> j</span>"
        );
    }

    #[test]
    fn same_position_tie_goes_to_root_authored_override() {
        // Ties break by compiled order, and `Document::blocks` enumerates
        // the root document before its imports — so a user pattern with the
        // built-in bold syntax beats the stdlib one at the same position.
        let doc = open_wdoc(
            r#"
inline_pattern user_bold {
  pattern = "\\*\\*([^*\n]+)\\*\\*"
  to_span = fn(g: list<utf8>) -> list<InlineSpan>
    [InlineSpan::Plain { text: at(g, 1), class: ["user-bold"] }]
}
"#,
        );
        let pats = patterns_for(&doc, &[]);
        assert_eq!(
            pats.render(&doc, "**x**"),
            "<span class=\"user-bold\">x</span>"
        );
    }

    #[test]
    fn two_user_patterns_tie_break_in_declaration_order() {
        let doc = open_wdoc(
            r#"
inline_pattern tag_first {
  pattern = "@(\\w+)"
  to_span = fn(g: list<utf8>) -> list<InlineSpan>
    [InlineSpan::Plain { text: at(g, 1), class: ["first"] }]
}
inline_pattern tag_second {
  pattern = "@(\\w+)"
  to_span = fn(g: list<utf8>) -> list<InlineSpan>
    [InlineSpan::Plain { text: at(g, 1), class: ["second"] }]
}
"#,
        );
        let pats = patterns_for(&doc, &[]);
        assert_eq!(
            pats.render(&doc, "@here"),
            "<span class=\"first\">here</span>"
        );
    }

    #[test]
    fn code_span_is_verbatim_not_retokenized() {
        // `_b_` inside a code span must stay literal — render_plain skips
        // re-tokenizing when the `code` class is present.
        assert_eq!(
            render_builtin("`a _b_ c`"),
            "<span class=\"code\">a _b_ c</span>"
        );
    }

    #[test]
    fn multibyte_text_around_and_inside_matches_does_not_panic() {
        // Multi-byte chars hugging both delimiters and inside the capture:
        // every span boundary the scan slices at must be a char boundary.
        assert_eq!(
            render_builtin("é🎉**wörld—ünïcode**🎉é"),
            "é🎉<span class=\"bold\">wörld—ünïcode</span>🎉é"
        );
    }

    #[test]
    fn zero_length_match_advances_one_char_and_terminates() {
        // `x*` matches empty at every position — the zero-length guard must
        // step forward by a whole char (not a byte, or `é` would split and
        // panic) and the scan must still terminate.
        let doc = open_wdoc(
            r#"
inline_pattern empty_ok {
  pattern = "x*"
  to_span = fn(g: list<utf8>) -> list<InlineSpan>
    [InlineSpan::Plain { text: at(g, 0), class: ["z"] }]
}
"#,
        );
        let pats = patterns_for(&doc, &[]);
        assert_eq!(
            pats.render(&doc, "éé"),
            "<span class=\"z\"></span>é<span class=\"z\"></span>é"
        );
    }

    #[test]
    fn retokenizing_stops_at_max_depth() {
        // Each level of this pattern strips one `>` from the front, so the
        // recursion is driven purely by re-tokenization. With more `>`s than
        // MAX_DEPTH, the guard must emit the remainder as a literal instead
        // of recursing forever.
        let doc = open_wdoc(
            r#"
inline_pattern gt {
  pattern = ">([^\n]*)"
  to_span = fn(g: list<utf8>) -> list<InlineSpan>
    [InlineSpan::Plain { text: at(g, 1), class: ["q"] }]
}
"#,
        );
        let pats = patterns_for(&doc, &[]);
        let out = pats.render(&doc, &">".repeat(MAX_DEPTH + 2));
        assert_eq!(out.matches("class=\"q\"").count(), MAX_DEPTH);
        // The two leftover `>`s surface literally (html-escaped).
        assert!(out.contains("&gt;&gt;"), "unexpected output: {out}");
    }

    #[test]
    fn literals_are_html_escaped_inside_and_outside_matches() {
        assert_eq!(
            render_builtin("a<b & **c<d**"),
            "a&lt;b &amp; <span class=\"bold\">c&lt;d</span>"
        );
    }

    #[test]
    fn resolve_href_rewrites_known_pages_and_records_bad_links() {
        let doc = open_wdoc("");
        let pats = patterns_for(&doc, &["intro"]);
        assert_eq!(pats.resolve_href("intro"), "intro.html");
        // Fragment survives the rewrite; external / anchor hrefs pass through.
        assert_eq!(pats.resolve_href("intro#sec"), "intro.html#sec");
        assert_eq!(pats.resolve_href("https://e.com/x"), "https://e.com/x");
        assert_eq!(pats.resolve_href("#top"), "#top");
        assert!(pats.take_link_errors().is_empty());
        // An unknown bare token passes through but is recorded so the build
        // can fail after the page loop.
        assert_eq!(pats.resolve_href("missing"), "missing");
        let errs = pats.take_link_errors();
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("missing"), "unexpected error: {errs:?}");
    }

    #[test]
    fn markdown_twin_escapes_literals_and_keeps_code_verbatim() {
        let doc = open_wdoc("");
        let pats = patterns_for(&doc, &[]);
        // Literal `[` `]` `*` are escaped so prose can't accidentally
        // format; a code span's content is emitted raw (no escaping,
        // no emphasis nesting).
        assert_eq!(
            pats.render_markdown(&doc, "see [x] *y* **b** `c*d`"),
            "see \\[x\\] \\*y\\* **b** `c*d`"
        );
    }

    #[test]
    fn pdf_runs_twin_keeps_code_verbatim_and_maps_styles() {
        let doc = open_wdoc("");
        let pats = patterns_for(&doc, &[]);
        // The code span must come through as a single Mono run — an inner
        // `_…_` pair re-interpreted as italic would split it.
        let runs = pats.render_runs(&doc, "a **b** `c _d_`");
        let flat: Vec<(&str, bool, bool)> = runs
            .iter()
            .map(|r| match r {
                InlineRun::Text { text, style } => (
                    text.as_str(),
                    style.bold,
                    matches!(style.family, FontFamily::Mono),
                ),
                other => panic!("unexpected run: {other:?}"),
            })
            .collect();
        assert_eq!(
            flat,
            vec![
                ("a ", false, false),
                ("b", true, false),
                (" ", false, false),
                ("c _d_", false, true),
            ]
        );
    }

    #[test]
    fn md_code_span_follows_commonmark_fence_rules() {
        // Fence widens past the longest interior backtick run; a leading or
        // trailing backtick forces the space padding.
        assert_eq!(md_code_span("a"), "`a`");
        assert_eq!(md_code_span("a`b"), "``a`b``");
        assert_eq!(md_code_span("`a"), "`` `a ``");
        assert_eq!(md_code_span("a``b"), "```a``b```");
    }
}
