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

use std::collections::BTreeMap;
use std::fmt::Write as _;

use regex::Regex;
use wcl_lang::{Document, FnValue, Value, VariantPayload};

use crate::render::escape_html;

/// Maximum recursion depth when re-tokenizing a match's text
/// fields. Keeps a self-referential pattern from blowing the
/// stack.
const MAX_DEPTH: usize = 8;

pub(crate) struct InlinePatterns {
    compiled: Vec<CompiledPattern>,
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
    pub(crate) fn load(doc: &Document) -> Self {
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
        InlinePatterns { compiled }
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
            _ => String::new(),
        }
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
        let inner = self.render_inner(doc, &text, depth + 1);
        let class_attr = class_attr(map);
        let mut out = String::new();
        write!(
            out,
            "<a{class_attr} href=\"{}\">{inner}</a>",
            escape_html(&href)
        )
        .expect("write to String");
        out
    }
}

fn map_utf8(map: &BTreeMap<String, Value>, name: &str) -> Option<String> {
    match map.get(name)? {
        Value::Utf8(s) | Value::Ascii(s) => Some(s.clone()),
        _ => None,
    }
}

fn class_attr(map: &BTreeMap<String, Value>) -> String {
    let Some(Value::List(items)) = map.get("class") else {
        return String::new();
    };
    let names: Vec<String> = items
        .iter()
        .filter_map(|v| match v {
            Value::Utf8(s) | Value::Ascii(s) => Some(s.clone()),
            _ => None,
        })
        .collect();
    if names.is_empty() {
        return String::new();
    }
    format!(" class=\"{}\"", escape_html(&names.join(" ")))
}
