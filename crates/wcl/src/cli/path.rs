//! Shared path parsing and AST node lookup for CLI mutation commands (set, remove, add).
//!
//! A WCL path looks like:
//!   - `service#svc-api.port`    → attribute `port` inside block `service` with id `svc-api`
//!   - `service#svc-api`         → the block itself
//!   - `config.debug`            → attribute `debug` inside block `config` (no id)

use crate::lang::ast::*;
use crate::lang::span::Span;

/// A parsed segment of a WCL CLI path.
#[derive(Debug)]
pub enum Segment {
    /// `block_type#block_id` — select a block by type and inline ID
    BlockById { kind: String, id: String },
    /// `block_type` — select a block by type (first match, no id)
    BlockByKind { kind: String },
    /// `.attribute_name` — select an attribute within the current scope
    Attribute(String),
}

/// The result of resolving a path against an AST.
#[derive(Debug)]
pub enum Resolved<'a> {
    /// Found an attribute — includes the whole attribute span and just the value span
    Attribute { attr: &'a Attribute },
    /// Found a block
    Block { block: &'a Block },
}

/// Parse a CLI path string like `service#svc-api.port` into segments.
pub fn parse_path(path: &str) -> Result<Vec<Segment>, String> {
    let mut segments = Vec::new();

    for part in path.split('.') {
        if part.is_empty() {
            return Err("empty segment in path".to_string());
        }
        if let Some(hash_pos) = part.find('#') {
            let block_type = &part[..hash_pos];
            let block_id = &part[hash_pos + 1..];
            if block_type.is_empty() || block_id.is_empty() {
                return Err(format!("invalid block reference: {}", part));
            }
            segments.push(Segment::BlockById {
                kind: block_type.to_string(),
                id: block_id.to_string(),
            });
        } else if segments.is_empty() {
            // First segment without # could be a block kind (if followed by more segments)
            // or a top-level attribute. We'll try block first, fallback to attribute.
            segments.push(Segment::BlockByKind {
                kind: part.to_string(),
            });
        } else {
            segments.push(Segment::Attribute(part.to_string()));
        }
    }

    if segments.is_empty() {
        return Err("empty path".to_string());
    }
    Ok(segments)
}

/// Get the inline ID string from a block, if present.
fn block_id(block: &Block) -> Option<&str> {
    match &block.inline_id {
        Some(InlineId::Literal(lit)) => Some(&lit.value),
        _ => None,
    }
}

/// Resolve a parsed path against a document's top-level items, returning the matched node.
pub fn resolve<'a>(doc: &'a Document, segments: &[Segment]) -> Result<Resolved<'a>, String> {
    if segments.is_empty() {
        return Err("empty path".to_string());
    }

    // Find the target body items (start from top-level)
    let top_body: Vec<&'a BodyItem> = doc
        .items
        .iter()
        .filter_map(|item| match item {
            DocItem::Body(bi) => Some(bi),
            _ => None,
        })
        .collect();

    resolve_in_body(&top_body, segments, 0)
}

fn resolve_in_body<'a>(
    body: &[&'a BodyItem],
    segments: &[Segment],
    seg_idx: usize,
) -> Result<Resolved<'a>, String> {
    let seg = &segments[seg_idx];
    let is_last = seg_idx == segments.len() - 1;

    match seg {
        Segment::BlockById { kind, id } => {
            let block = body
                .iter()
                .filter_map(|bi| match bi {
                    BodyItem::Block(b) => Some(b),
                    _ => None,
                })
                .find(|b| b.kind.name == *kind && block_id(b) == Some(id.as_str()))
                .ok_or_else(|| format!("block {}#{} not found", kind, id))?;

            if is_last {
                Ok(Resolved::Block { block })
            } else {
                // Descend into block body
                let children: Vec<&BodyItem> = block.body.iter().collect();
                resolve_in_body(&children, segments, seg_idx + 1)
            }
        }
        Segment::BlockByKind { kind } => {
            // If this is the last segment, it might be a top-level attribute
            if is_last {
                // Try attribute first
                if let Some(attr) = body.iter().find_map(|bi| match bi {
                    BodyItem::Attribute(a) if a.name.name == *kind => Some(a),
                    _ => None,
                }) {
                    return Ok(Resolved::Attribute { attr });
                }
            }

            // Try block
            let block = body
                .iter()
                .filter_map(|bi| match bi {
                    BodyItem::Block(b) => Some(b),
                    _ => None,
                })
                .find(|b| b.kind.name == *kind)
                .ok_or_else(|| format!("block or attribute '{}' not found", kind))?;

            if is_last {
                Ok(Resolved::Block { block })
            } else {
                let children: Vec<&BodyItem> = block.body.iter().collect();
                resolve_in_body(&children, segments, seg_idx + 1)
            }
        }
        Segment::Attribute(name) => {
            let attr = body
                .iter()
                .find_map(|bi| match bi {
                    BodyItem::Attribute(a) if a.name.name == *name => Some(a),
                    _ => None,
                })
                .ok_or_else(|| format!("attribute '{}' not found", name))?;

            Ok(Resolved::Attribute { attr })
        }
    }
}

/// Find the byte range of a line in the source that contains the given span,
/// extending to include the full line (from start-of-line to end-of-line including newline).
pub fn line_span_of(source: &str, span: Span) -> (usize, usize) {
    let bytes = source.as_bytes();

    // Find start of line
    let mut line_start = span.start;
    while line_start > 0 && bytes[line_start - 1] != b'\n' {
        line_start -= 1;
    }

    // Find end of line (including newline)
    let mut line_end = span.end;
    while line_end < bytes.len() && bytes[line_end] != b'\n' {
        line_end += 1;
    }
    if line_end < bytes.len() {
        line_end += 1; // include the newline
    }

    (line_start, line_end)
}

/// Find the full span of a block including any leading decorator lines.
/// Returns (start, end) byte offsets covering the block from the first decorator
/// (or the block keyword) through the closing `}` and its trailing newline.
pub fn block_full_span(source: &str, block: &Block) -> (usize, usize) {
    let start = if let Some(first_dec) = block.decorators.first() {
        line_span_of(source, first_dec.span).0
    } else {
        line_span_of(source, block.span).0
    };

    let (_, end) = line_span_of(source, block.span);
    (start, end)
}

/// Find the full span of an attribute including any leading decorator lines.
pub fn attr_full_span(source: &str, attr: &Attribute) -> (usize, usize) {
    let start = if let Some(first_dec) = attr.decorators.first() {
        line_span_of(source, first_dec.span).0
    } else {
        line_span_of(source, attr.span).0
    };

    let (_, end) = line_span_of(source, attr.span);
    (start, end)
}
