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

use crate::icons::IconRegistry;
use crate::image::ImageRegistry;
use crate::render::escape_html;
use crate::tileset::TilesetRegistry;

/// Maximum recursion depth when re-tokenizing a match's text
/// fields. Keeps a self-referential pattern from blowing the
/// stack.
const MAX_DEPTH: usize = 8;

pub(crate) struct InlinePatterns {
    compiled: Vec<CompiledPattern>,
    /// Names of every `page` block in the document, used by
    /// `render_link` to recognise `[text](page_name)` references
    /// and rewrite them to `page_name.html`.
    page_names: HashSet<String>,
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
    pub(crate) fn load(
        doc: &Document,
        page_names: HashSet<String>,
        icons: IconRegistry,
        tilesets: TilesetRegistry,
        images: ImageRegistry,
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
            link_errors: RefCell::new(Vec::new()),
            icons,
            tilesets,
            images,
        }
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
    fn resolve_href(&self, href: &str) -> String {
        if is_external_href(href) {
            return href.to_string();
        }
        let (page, fragment) = match href.find('#') {
            Some(i) => (&href[..i], &href[i..]),
            None => (href, ""),
        };
        if self.page_names.contains(page) {
            return format!("{page}.html{fragment}");
        }
        self.link_errors
            .borrow_mut()
            .push(format!("link to unknown page '{page}'"));
        href.to_string()
    }
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
