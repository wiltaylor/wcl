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

use crate::file::FileRegistry;
use crate::icons::IconRegistry;
use crate::image::ImageRegistry;
use crate::pdf::ir::{FontFamily, InlineRun, TextStyle};
use crate::render::escape_html;
use crate::tileset::TilesetRegistry;
use crate::video::VideoRegistry;

/// Maximum recursion depth when re-tokenizing a match's text
/// fields. Keeps a self-referential pattern from blowing the
/// stack.
const MAX_DEPTH: usize = 8;

/// Which output backend a render pass targets. Carried on
/// [`InlinePatterns`] so the block-visibility predicate
/// ([`crate::visibility::block_visible`]) can honour the `backends=[…]`
/// axis of `@only` / `@except`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Backend {
    Html,
    Pdf,
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

pub(crate) struct InlinePatterns {
    compiled: Vec<CompiledPattern>,
    /// Names of every `page` in the current site, used by `render_link`
    /// to recognise bare `[text](page_name)` references and rewrite them
    /// to `page_name.html`.
    page_names: HashSet<String>,
    /// The current site's name (`None` for a single unnamed / synthetic
    /// site) and its output prefix this build (`""` at the root, else
    /// `"<name>/"`), used to resolve `[text](site:page)` cross-site links.
    current_site: Option<String>,
    current_prefix: String,
    /// Every declared site → its page-name set, and → its URL prefix in
    /// the full layout (`""` for the root site, else `"<name>/"`). A
    /// `site:page` link validates against these and builds a relative
    /// href from `current_prefix` to the target.
    site_pages: BTreeMap<String, HashSet<String>>,
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
    /// Skill-target link layout. `Some(start_page)` puts internal page links
    /// into the skill folder shape (`SKILL.md` for the start page,
    /// `references/<name>.md` otherwise). `skill_current_ref` is `true` while
    /// rendering a reference page (one level deep), so its links get a `../`
    /// prefix. Both are interior-mutable, set per page by the skill backend.
    skill_start: RefCell<Option<String>>,
    skill_current_ref: RefCell<bool>,
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
    vis_site: RefCell<Option<String>>,
    vis_template: RefCell<Option<String>>,
}

struct CompiledPattern {
    regex: Regex,
    to_span: FnValue,
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
            compiled.push(CompiledPattern {
                regex,
                to_span: fv.clone(),
            });
        }
        InlinePatterns {
            compiled,
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
            skill_start: RefCell::new(None),
            skill_current_ref: RefCell::new(false),
            ui_theme: RefCell::new(crate::render::UiTheme::default()),
            backend,
            vis_site: RefCell::new(None),
            vis_template: RefCell::new(None),
        }
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

    /// Enable the skill-folder link layout: internal page links resolve to
    /// `SKILL.md` (for `start_page`) or `references/<name>.md`. Called once
    /// per skill site by the skill backend.
    pub(crate) fn set_skill_layout(&self, start_page: String) {
        *self.skill_start.borrow_mut() = Some(start_page);
    }

    /// Mark whether the page now rendering is a reference page (one level
    /// deep under `references/`), so its internal links get a `../` prefix.
    /// Called per page by the skill backend.
    pub(crate) fn set_skill_current_reference(&self, is_reference: bool) {
        *self.skill_current_ref.borrow_mut() = is_reference;
    }

    /// Relative prefix from the current page's output location back to the
    /// build root, for root-relative asset URLs (images, diagram SVGs,
    /// shipped files). `"../"` while a skill reference page (one level deep
    /// under `references/`) renders, else `""`.
    pub(crate) fn asset_prefix(&self) -> &'static str {
        if *self.skill_current_ref.borrow() {
            "../"
        } else {
            ""
        }
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

    fn render_inner(&self, doc: &Document, text: &str, depth: usize) -> String {
        if depth >= MAX_DEPTH || self.compiled.is_empty() {
            return escape_html(text);
        }
        let mut out = String::new();
        let mut pos = 0usize;
        while pos < text.len() {
            let Some((start, end, pat_idx, caps)) = self.find_next(text, pos) else {
                out.push_str(&escape_html(&text[pos..]));
                break;
            };
            if start > pos {
                out.push_str(&escape_html(&text[pos..start]));
            }
            let pattern = &self.compiled[pat_idx];
            let args: Vec<Value> = caps.into_iter().map(Value::Utf8).collect();
            match doc.call_value(&pattern.to_span, &[Value::List(args)]) {
                Ok(Value::List(spans)) => {
                    for span in spans {
                        out.push_str(&self.render_variant(doc, &span, depth));
                    }
                }
                _ => {
                    // If the user fn returned something unexpected,
                    // emit the matched text literally so the user can
                    // still see what was there.
                    out.push_str(&escape_html(&text[start..end]));
                }
            }
            pos = end;
            // Guard against zero-length matches — without this, a
            // pattern like `()` would loop forever.
            if end == start {
                if let Some(next_ch) = text[pos..].chars().next() {
                    let next_pos = pos + next_ch.len_utf8();
                    out.push_str(&escape_html(&text[pos..next_pos]));
                    pos = next_pos;
                } else {
                    break;
                }
            }
        }
        out
    }

    /// Scan every pattern starting at `pos`, return the earliest-
    /// starting match. Ties broken by pattern declaration order so
    /// the built-ins (declared first in `wdoc.wcl`) win over a
    /// user override with the same syntax at the same position.
    fn find_next(&self, text: &str, pos: usize) -> Option<(usize, usize, usize, Vec<String>)> {
        let mut best: Option<(usize, usize, usize, Vec<String>)> = None;
        for (i, pat) in self.compiled.iter().enumerate() {
            let Some(caps) = pat.regex.captures_at(text, pos) else {
                continue;
            };
            let m = caps.get(0)?;
            // Captures are guaranteed to be ≥ pos because of
            // `captures_at`'s contract.
            let start = m.start();
            let end = m.end();
            if best.as_ref().map(|b| start < b.0).unwrap_or(true) {
                let groups: Vec<String> = (0..caps.len())
                    .map(|g| {
                        caps.get(g)
                            .map(|m| m.as_str().to_string())
                            .unwrap_or_default()
                    })
                    .collect();
                best = Some((start, end, i, groups));
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

    fn runs_inner(
        &self,
        doc: &Document,
        text: &str,
        depth: usize,
        style: TextStyle,
        out: &mut Vec<InlineRun>,
    ) {
        if depth >= MAX_DEPTH || self.compiled.is_empty() {
            push_run(out, text, style);
            return;
        }
        let mut pos = 0usize;
        while pos < text.len() {
            let Some((start, end, pat_idx, caps)) = self.find_next(text, pos) else {
                push_run(out, &text[pos..], style);
                break;
            };
            if start > pos {
                push_run(out, &text[pos..start], style);
            }
            let pattern = &self.compiled[pat_idx];
            let args: Vec<Value> = caps.into_iter().map(Value::Utf8).collect();
            match doc.call_value(&pattern.to_span, &[Value::List(args)]) {
                Ok(Value::List(spans)) => {
                    for span in spans {
                        self.runs_variant(doc, &span, depth, style, out);
                    }
                }
                _ => push_run(out, &text[start..end], style),
            }
            pos = end;
            if end == start {
                if let Some(next_ch) = text[pos..].chars().next() {
                    let next_pos = pos + next_ch.len_utf8();
                    push_run(out, &text[pos..next_pos], style);
                    pos = next_pos;
                } else {
                    break;
                }
            }
        }
    }

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
                let st = apply_classes(style, &class_list(map));
                self.runs_inner(doc, &text, depth + 1, st, out);
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
                svg: crate::math::render_inline_math(map),
            }),
            _ => {}
        }
    }

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
            "Math" => crate::math::render_inline_math(map),
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

    fn render_plain(&self, doc: &Document, map: &BTreeMap<String, Value>, depth: usize) -> String {
        let text = map_utf8(map, "text").unwrap_or_default();
        let inner = self.render_inner(doc, &text, depth + 1);
        let class_attr = class_attr(map);
        let mut out = format!("<span{class_attr}>");
        out.push_str(&inner);
        out.push_str("</span>");
        out
    }

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

    fn markdown_inner(&self, doc: &Document, text: &str, depth: usize) -> String {
        if depth >= MAX_DEPTH || self.compiled.is_empty() {
            return escape_md(text);
        }
        let mut out = String::new();
        let mut pos = 0usize;
        while pos < text.len() {
            let Some((start, end, pat_idx, caps)) = self.find_next(text, pos) else {
                out.push_str(&escape_md(&text[pos..]));
                break;
            };
            if start > pos {
                out.push_str(&escape_md(&text[pos..start]));
            }
            let pattern = &self.compiled[pat_idx];
            let args: Vec<Value> = caps.into_iter().map(Value::Utf8).collect();
            match doc.call_value(&pattern.to_span, &[Value::List(args)]) {
                Ok(Value::List(spans)) => {
                    for span in spans {
                        out.push_str(&self.markdown_variant(doc, &span, depth));
                    }
                }
                _ => out.push_str(&escape_md(&text[start..end])),
            }
            pos = end;
            // Same zero-length-match guard as the HTML / PDF paths.
            if end == start {
                if let Some(next_ch) = text[pos..].chars().next() {
                    let next_pos = pos + next_ch.len_utf8();
                    out.push_str(&escape_md(&text[pos..next_pos]));
                    pos = next_pos;
                } else {
                    break;
                }
            }
        }
        out
    }

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
        // Skill target: a bare internal page link resolves into the skill
        // folder layout — `SKILL.md` for the start page, `references/<name>.md`
        // otherwise — prefixed with `../` when the current page is itself a
        // reference. External / anchor / path-relative hrefs and `site:page`
        // cross-site links fall through to the normal resolver below.
        if let Some(start) = self.skill_start.borrow().clone()
            && !is_external_href(href)
        {
            let (target, frag) = match href.find('#') {
                Some(i) => (&href[..i], &href[i..]),
                None => (href, ""),
            };
            if !target.contains(':') {
                if self.page_names.contains(target) {
                    let rel = if target == start {
                        "SKILL.md".to_string()
                    } else {
                        format!("references/{target}.md")
                    };
                    let up = if *self.skill_current_ref.borrow() {
                        "../"
                    } else {
                        ""
                    };
                    return format!("{up}{rel}{frag}");
                }
                self.link_errors
                    .borrow_mut()
                    .push(format!("link to unknown page '{target}'"));
                return href.to_string();
            }
        }
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

fn map_utf8(map: &BTreeMap<String, Value>, name: &str) -> Option<String> {
    match map.get(name)? {
        Value::Utf8(s) | Value::Ascii(s) => Some(s.clone()),
        _ => None,
    }
}

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
