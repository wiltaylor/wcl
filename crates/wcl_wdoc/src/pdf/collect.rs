//! Collect a page's blocks into the PDF [`ir`](super::ir).
//!
//! This is the PDF twin of the HTML render dispatch: rather than calling
//! `render_html_variant` (which emits HTML strings), it reuses the shared
//! lowering ([`lower_to_values`](crate::render::lower_to_values), which runs a
//! block's WCL `lower` and hands back the raw `HtmlFundamental` values) and
//! walks those fundamentals into block/inline IR nodes. Inline text runs
//! through the shared inline-pattern engine via
//! [`InlinePatterns::render_runs`](crate::inline::InlinePatterns::render_runs),
//! so `**bold**` / `_italic_` / `` `code` `` / `[links](page)` resolve exactly
//! as on the HTML path. Non-prose fundamentals are skipped at this phase and
//! rejoin with the SVG/table work.

use std::collections::BTreeMap;

use wcl_lang::{Block, Document, Value, VariantPayload};

use crate::inline::InlinePatterns;
use crate::render::{kind_for_variant, lower_to_values, map_utf8, map_utf8_list};

use super::ir::{BlockNode, InlineRun, TextStyle};

/// Collect every child block of `page` into a flat list of flow nodes.
pub(crate) fn collect_page(
    doc: &Document,
    page: &Block<'_>,
    patterns: &InlinePatterns,
) -> Vec<BlockNode> {
    let mut out = Vec::new();
    for child in page.blocks() {
        collect_block(doc, &child, patterns, &mut out);
    }
    out
}

fn collect_block(
    doc: &Document,
    block: &Block<'_>,
    patterns: &InlinePatterns,
    out: &mut Vec<BlockNode>,
) {
    let kind = block.kind();
    let Some(values) = lower_to_values(doc, block, kind) else {
        return;
    };
    for v in &values {
        walk_block_variant(doc, v, patterns, out);
    }
}

/// Turn one fundamental value into block-level IR, pushing onto `out`.
fn walk_block_variant(
    doc: &Document,
    value: &Value,
    patterns: &InlinePatterns,
    out: &mut Vec<BlockNode>,
) {
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
                            text: inline_text(children),
                            style: TextStyle::heading(),
                        }],
                    });
                }
                // Unknown wrapper: descend, treating children as blocks.
                _ => {
                    for c in children {
                        walk_block_variant(doc, c, patterns, out);
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
        _ => {}
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

/// The concatenated raw text of an element's inline children (for headings,
/// where we don't run the emphasis engine).
fn inline_text(children: &[Value]) -> String {
    let mut s = String::new();
    for c in children {
        if let Some((kind, map)) = as_record_variant(c) {
            match kind.as_str() {
                "inline" => s.push_str(&map_utf8(map, "text").unwrap_or_default()),
                "paragraph" => s.push_str(&map_utf8_list(map, "spans").join("")),
                "element" => s.push_str(&inline_text(map_list(map, "children"))),
                _ => {}
            }
        }
    }
    s
}

fn heading_level(classes: &[String]) -> Option<u8> {
    for c in classes {
        if let Some(n) = c.strip_prefix("heading-")
            && let Ok(level) = n.parse::<u8>()
            && (1..=6).contains(&level)
        {
            return Some(level);
        }
    }
    None
}

/// Destructure a `Value::Variant` with a record payload into `(kind, map)`.
fn as_record_variant(value: &Value) -> Option<(String, &BTreeMap<String, Value>)> {
    let Value::Variant {
        variant, payload, ..
    } = value
    else {
        return None;
    };
    let VariantPayload::Record(map) = payload else {
        return None;
    };
    Some((kind_for_variant(variant), map))
}

/// Read a list-typed field from a payload map (empty slice when absent).
fn map_list<'a>(map: &'a BTreeMap<String, Value>, name: &str) -> &'a [Value] {
    match map.get(name) {
        Some(Value::List(items)) => items,
        _ => &[],
    }
}
