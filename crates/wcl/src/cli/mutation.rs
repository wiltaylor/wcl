//! Shared parsing/utilities for `wcl set`, `wcl remove`, and `wcl add`.
//!
//! Spec grammar (set, remove):
//!     <selector>  [ '~>' <action> ]
//!
//! Action grammar:
//!   set:    <path> '=' <wcl-expression>
//!   remove: <path>      (or empty — selector-only removes whole block)
//!
//! Assignment grammar (add only):
//!     <path> '=' <wcl-expression>
//!
//! Path grammar:
//!     ('.' segment)+
//!     segment := ident ( '#' ident )?

use std::collections::HashMap;

use crate::eval::{BlockRef, Evaluator, QueryEngine, ScopeId, ScopeKind, Value};
use crate::lang::ast::{Block, BodyItem, DocItem, Document, InlineId};
use crate::lang::Span;

#[derive(Debug, Clone)]
pub struct PathSegment {
    pub name: String,
    pub id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AssignmentPath {
    pub segments: Vec<PathSegment>,
}

impl AssignmentPath {
    pub fn last(&self) -> &PathSegment {
        self.segments.last().expect("non-empty path")
    }
}

/// Split a `set`/`remove` spec into a selector string and an optional action string.
///
/// `<selector> ~> <action>`  →  (selector, Some(action))
/// `<selector>`              →  (selector, None)
///
/// We split on the *first* `~>` that is not inside a string literal so users can
/// embed `~>` in selector filter expressions if they need to.
pub fn split_spec(spec: &str) -> (&str, Option<&str>) {
    let bytes = spec.as_bytes();
    let mut i = 0;
    let mut in_string = false;
    let mut string_quote = b'"';
    while i + 1 < bytes.len() {
        let b = bytes[i];
        if in_string {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == string_quote {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if b == b'"' || b == b'\'' {
            in_string = true;
            string_quote = b;
            i += 1;
            continue;
        }
        if b == b'~' && bytes[i + 1] == b'>' {
            let selector = spec[..i].trim_end();
            let action = spec[i + 2..].trim_start();
            return (selector, Some(action));
        }
        i += 1;
    }
    (spec.trim(), None)
}

/// Parse a `.foo` or `.foo.bar.baz` or `.foo#id.bar` path.
///
/// Returns the parsed path. Stops at the first non-path character.
/// On success, returns `(path, bytes_consumed)`.
pub fn parse_path(input: &str) -> Result<(AssignmentPath, usize), String> {
    let bytes = input.as_bytes();
    let mut i = 0;

    // Skip leading whitespace
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }

    let mut segments = Vec::new();

    while i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        // Read identifier
        let name_start = i;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
        }
        if i == name_start {
            return Err(format!("expected identifier after '.' at offset {}", i));
        }
        let name = input[name_start..i].to_string();

        // Optional #id
        let id = if i < bytes.len() && bytes[i] == b'#' {
            i += 1;
            let id_start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'-')
            {
                i += 1;
            }
            if i == id_start {
                return Err(format!("expected identifier after '#' at offset {}", i));
            }
            Some(input[id_start..i].to_string())
        } else {
            None
        };

        segments.push(PathSegment { name, id });
    }

    if segments.is_empty() {
        return Err("expected a path starting with '.'".to_string());
    }

    Ok((AssignmentPath { segments }, i))
}

/// Parse `<path> = <wcl-expression>`. Returns the path and the raw rhs source text.
pub fn parse_assignment(input: &str) -> Result<(AssignmentPath, String), String> {
    let (path, mut i) = parse_path(input)?;
    let bytes = input.as_bytes();
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'=' {
        return Err("expected '=' after path".to_string());
    }
    i += 1;
    let rhs = input[i..].trim().to_string();
    if rhs.is_empty() {
        return Err("expected expression after '='".to_string());
    }
    // Validate the rhs by parsing it as a WCL expression. We discard the AST
    // and use the source text verbatim when rewriting the file.
    crate::lang::parse_expression(&rhs, crate::FileId(0)).map_err(|d| {
        let messages: Vec<String> = d
            .into_diagnostics()
            .into_iter()
            .map(|x| x.message)
            .collect();
        format!("invalid expression: {}", messages.join("; "))
    })?;
    Ok((path, rhs))
}

/// Parse just a path with no `=` (used by `wcl remove`'s action).
pub fn parse_path_only(input: &str) -> Result<AssignmentPath, String> {
    let (path, i) = parse_path(input)?;
    let rest = input[i..].trim();
    if !rest.is_empty() {
        return Err(format!("unexpected text after path: '{}'", rest));
    }
    Ok(path)
}

/// Build a flat map from block AST span -> &Block by walking the document
/// recursively. Used to map query results back to mutable AST positions.
pub fn build_span_to_block(doc: &Document) -> HashMap<Span, &Block> {
    let mut map = HashMap::new();
    for item in &doc.items {
        if let DocItem::Body(BodyItem::Block(b)) = item {
            visit_block(b, &mut map);
        }
    }
    map
}

fn visit_block<'a>(block: &'a Block, map: &mut HashMap<Span, &'a Block>) {
    map.insert(block.span, block);
    for item in &block.body {
        if let BodyItem::Block(child) = item {
            visit_block(child, map);
        }
    }
}

/// Look up the inline id of a block, if it has a literal one.
pub fn block_inline_id(block: &Block) -> Option<&str> {
    match &block.inline_id {
        Some(InlineId::Literal(lit)) => Some(&lit.value),
        _ => None,
    }
}

/// Run a query pipeline against the evaluated document, returning matched
/// `BlockRef`s with proper attribute values (unlike `Document::query` which
/// re-evaluates against an empty scope and drops attributes that depend on
/// let bindings).
pub fn run_selector(doc: &crate::Document, selector_str: &str) -> Result<Vec<Value>, String> {
    let pipeline = crate::lang::parse_query(selector_str, crate::FileId(0)).map_err(|d| {
        let messages: Vec<String> = d
            .into_diagnostics()
            .into_iter()
            .map(|x| x.message)
            .collect();
        format!("selector parse error: {}", messages.join("; "))
    })?;

    // Pull every BlockRef out of doc.values (top-level) and recursively from
    // their children so the query engine can match nested kinds via `..kind`.
    let mut blocks: Vec<BlockRef> = Vec::new();
    for value in doc.values.values() {
        collect_block_refs_from_value(value, &mut blocks);
    }

    let mut evaluator = Evaluator::new();
    let scope: ScopeId = evaluator.scopes_mut().create_scope(ScopeKind::Module, None);
    let engine = QueryEngine::new();
    let result = engine
        .execute(&pipeline, &blocks, &mut evaluator, scope)
        .map_err(|e| format!("selector error: {}", e))?;

    let matches = match result {
        Value::List(items) => items,
        single => vec![single],
    };
    Ok(matches)
}

fn collect_block_refs_from_value(value: &Value, out: &mut Vec<BlockRef>) {
    match value {
        Value::BlockRef(br) => out.push(br.clone()),
        Value::List(items) => {
            for item in items {
                collect_block_refs_from_value(item, out);
            }
        }
        _ => {}
    }
}

/// Indentation string (run of spaces/tabs) for the line containing `byte_offset`.
pub fn line_indent(source: &str, byte_offset: usize) -> String {
    let bytes = source.as_bytes();
    let mut start = byte_offset.min(bytes.len());
    while start > 0 && bytes[start - 1] != b'\n' {
        start -= 1;
    }
    let mut end = start;
    while end < bytes.len() && (bytes[end] == b' ' || bytes[end] == b'\t') {
        end += 1;
    }
    source[start..end].to_string()
}
