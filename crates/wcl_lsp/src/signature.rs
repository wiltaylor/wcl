//! Signature help (`textDocument/signatureHelp`).
//!
//! Finding the enclosing call is a forward lexical scan over
//! `source[..offset]` rather than an AST lookup — consistent with the
//! crate's raw-byte style (see `resolve.rs`) and, critically, robust
//! while the user is mid-call (`foo(1, |`), when the source doesn't
//! parse. Strings (`"…"` with `\` escapes) and line comments (`//`,
//! `#`) are skipped; brace/bracket nesting absorbs commas inside list
//! literals and records passed as arguments. Known limitation: a call
//! inside `${…}` string interpolation is treated as string content.
//!
//! The callee resolves, in order, against: builtin functions
//! (`Environment::builtins`), `fn name(…)` items (via `find_symbol`,
//! cross-file through the root document), and local `let f = fn(…)`
//! closures in scope.

use std::collections::HashMap;
use std::path::PathBuf;

use tower_lsp::lsp_types::{
    Documentation, ParameterInformation, ParameterLabel, SignatureHelp, SignatureInformation,
};
use wcl_lang::{Document, SymbolKind, ast, parse_for_edit};

use crate::resolve::{dotted_form, word_at};
use crate::scan::is_ident_byte;
use crate::walk;

/// The innermost unclosed call at the cursor: who is being called and
/// which argument the cursor sits in.
#[derive(Debug, PartialEq)]
pub(crate) struct CallContext {
    pub callee: String,
    pub active_param: u32,
}

/// Compute signature help at `offset`. `open_buffers` lets a cross-file
/// `fn` declaration render from its unsaved editor state.
///
/// A buffer mid-call (`x = add(1, |`) doesn't parse, so the symbol
/// lookups run against a *repaired* source — the text up to the cursor
/// with its open brackets closed (see [`repair_source`]) — whenever the
/// buffer itself won't open.
pub(crate) fn signature_help(
    source: &str,
    uri: &str,
    offset: usize,
    root_doc: Option<&Document>,
    open_buffers: &HashMap<PathBuf, String>,
) -> Option<SignatureHelp> {
    let call = enclosing_call(source, offset)?;
    let repaired = repair_source(source, offset);
    let local_doc = Document::open(source, uri)
        .or_else(|_| Document::open(&repaired, uri))
        .ok();
    let sig = [local_doc.as_ref(), root_doc]
        .into_iter()
        .flatten()
        .find_map(|doc| resolve_signature(doc, &repaired, offset, &call, open_buffers))?;
    let params = sig.parameters.as_ref().map_or(0, Vec::len) as u32;
    Some(SignatureHelp {
        active_signature: Some(0),
        // Clamp so a cursor past the last parameter keeps it highlighted
        // (matters for variadic builtins rendered without parameters).
        active_parameter: Some(call.active_param.min(params.saturating_sub(1))),
        signatures: vec![sig],
    })
}

/// Close the brackets left open at `offset` so the mid-edit buffer
/// parses: `source[..offset]`, a placeholder `0` when the last token
/// expects an expression (`(` or `,`), then the closers innermost
/// first. Best effort — a construct the placeholder can't complete
/// (e.g. an unclosed record field) just leaves the repair unparseable
/// and the caller degrades gracefully.
pub(crate) fn repair_source(source: &str, offset: usize) -> String {
    let bytes = source.as_bytes();
    let end = offset.min(bytes.len());
    let mut closers = Vec::new();
    let mut i = 0;
    while i < end {
        match bytes[i] {
            b'"' => {
                i += 1;
                while i < end {
                    match bytes[i] {
                        b'\\' => i += 1,
                        b'"' => break,
                        _ => {}
                    }
                    i += 1;
                }
            }
            b'#' => skip_line(bytes, &mut i, end),
            b'/' if bytes.get(i + 1) == Some(&b'/') => skip_line(bytes, &mut i, end),
            b'(' => closers.push(')'),
            b'[' => closers.push(']'),
            b'{' => closers.push('}'),
            b')' | b']' | b'}' => {
                closers.pop();
            }
            _ => {}
        }
        i += 1;
    }
    let mut out = source[..end].to_string();
    if matches!(
        out.trim_end().as_bytes().last(),
        Some(b'(' | b',' | b'[' | b'=')
    ) {
        out.push('0');
    }
    out.extend(closers.into_iter().rev());
    out.push('\n');
    out
}

/// Scan `source[..offset]` for the innermost paren that is still open
/// at the cursor *and* has a callee identifier in front of it.
pub(crate) fn enclosing_call(source: &str, offset: usize) -> Option<CallContext> {
    struct Frame {
        open: u8,
        commas: u32,
        callee: Option<String>,
    }
    let bytes = source.as_bytes();
    let end = offset.min(bytes.len());
    let mut stack: Vec<Frame> = Vec::new();
    let mut i = 0;
    while i < end {
        match bytes[i] {
            b'"' => {
                // Skip the string body (escapes included). An unterminated
                // string swallows the rest — the cursor is inside it.
                i += 1;
                while i < end {
                    match bytes[i] {
                        b'\\' => i += 1,
                        b'"' => break,
                        _ => {}
                    }
                    i += 1;
                }
            }
            b'#' => skip_line(bytes, &mut i, end),
            b'/' if bytes.get(i + 1) == Some(&b'/') => skip_line(bytes, &mut i, end),
            b'(' => {
                let callee = callee_before(source, i);
                stack.push(Frame {
                    open: b'(',
                    commas: 0,
                    callee,
                });
            }
            b'[' | b'{' => stack.push(Frame {
                open: bytes[i],
                commas: 0,
                callee: None,
            }),
            b')' | b']' | b'}' => {
                stack.pop();
            }
            b',' => {
                if let Some(top) = stack.last_mut() {
                    top.commas += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    while let Some(frame) = stack.pop() {
        // A bare grouping paren has no callee — keep looking outward.
        if frame.open == b'('
            && let Some(callee) = frame.callee
        {
            return Some(CallContext {
                callee,
                active_param: frame.commas,
            });
        }
    }
    None
}

fn skip_line(bytes: &[u8], i: &mut usize, end: usize) {
    while *i < end && bytes[*i] != b'\n' {
        *i += 1;
    }
}

/// The (possibly dotted) identifier ending at the last non-whitespace
/// byte before the `(` at `paren` — the call's callee, if any.
fn callee_before(source: &str, paren: usize) -> Option<String> {
    let bytes = source.as_bytes();
    let mut i = paren;
    while i > 0 && bytes[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    if i == 0 || !is_ident_byte(bytes[i - 1]) {
        return None;
    }
    let (word, span) = word_at(source, i - 1)?;
    Some(dotted_form(source, span).unwrap_or(word))
}

/// Resolve `call.callee` to a rendered signature against `doc`,
/// trying builtins, then indexed `fn` items, then in-scope closures.
/// `text` is the parseable form of the edited buffer (the repaired
/// source when the buffer itself is mid-edit).
fn resolve_signature(
    doc: &Document,
    text: &str,
    offset: usize,
    call: &CallContext,
    open_buffers: &HashMap<PathBuf, String>,
) -> Option<SignatureInformation> {
    if !call.callee.contains('.')
        && let Some((_, builtin)) = doc
            .environment()
            .builtins()
            .find(|(name, _)| *name == call.callee)
    {
        let info = builtin.signature_info();
        if info.params.is_empty() {
            // A signature-only builtin (variadic `format`): show the
            // printable form with no parameter entries.
            let label = if info.signature.is_empty() {
                format!("{}(…)", call.callee)
            } else {
                format!("{} {}", call.callee, info.signature)
            };
            return Some(plain_signature(label, doc_text(&info.doc)));
        }
        let params: Vec<(String, Option<String>)> = info
            .params
            .iter()
            .map(|p| {
                (
                    format!("{}: {}", p.name, p.ty),
                    (!p.doc.is_empty()).then(|| p.doc.clone()),
                )
            })
            .collect();
        return Some(assemble(
            &call.callee,
            &params,
            &info.return_type,
            doc_text(&info.doc),
        ));
    }

    // `fn name(…)` items, namespace-prefixed form first (more specific).
    let mut fqns = Vec::with_capacity(2);
    let ns = doc.namespace();
    if !ns.is_empty() && !call.callee.contains('.') {
        fqns.push(format!("{}.{}", ns.join("."), call.callee));
    }
    fqns.push(call.callee.clone());
    for fqn in fqns {
        let Some(hit) = doc.find_symbol(&fqn) else {
            continue;
        };
        if !matches!(hit.record.kind, SymbolKind::FnDecl) {
            continue;
        }
        let short = fqn.rsplit('.').next().unwrap_or(&fqn).to_string();
        let item_index = hit.record.path.item_index;
        let rendered = match hit.source_path {
            // Declared in the buffer being edited.
            None => fn_item_signature(text, &short, item_index),
            // Declared in an imported file — render from its overlay
            // text when the file is open, else from disk.
            Some(path) => {
                let text = open_buffers
                    .get(path)
                    .cloned()
                    .or_else(|| std::fs::read_to_string(path).ok())?;
                fn_item_signature(&text, &short, item_index)
            }
        };
        if rendered.is_some() {
            return rendered;
        }
    }

    // Local closures: `let f = fn(…) …` in the enclosing scopes.
    let ast = parse_for_edit(text, "<signature>").ok()?;
    let scopes = walk::enclosing_scopes_at(&ast.items, offset);
    let lit = scopes
        .lets
        .iter()
        .rev()
        .find(|l| l.name == call.callee)
        .and_then(|l| match &l.value {
            ast::Expr::Function(f) => Some(f),
            _ => None,
        })?;
    Some(function_lit_signature(&call.callee, lit, None))
}

/// Parse `text` and render the `fn` item at `item_index`, verifying the
/// name still matches (the index came from a separate parse of the same
/// bytes; a scan-by-name backstops any drift).
fn fn_item_signature(text: &str, name: &str, item_index: usize) -> Option<SignatureInformation> {
    let ast = parse_for_edit(text, "<signature>").ok()?;
    let as_fn = |item: &ast::Item| -> Option<(String, ast::FunctionLit)> {
        let ast::Item::Let(l) = item else { return None };
        let ast::Expr::Function(f) = &l.value else {
            return None;
        };
        Some((l.name.clone(), f.clone()))
    };
    let lit = ast
        .items
        .get(item_index)
        .and_then(&as_fn)
        .filter(|(n, _)| n == name)
        .or_else(|| ast.items.iter().filter_map(&as_fn).find(|(n, _)| n == name))
        .map(|(_, f)| f)?;
    Some(function_lit_signature(name, &lit, None))
}

/// Render `fn name(p: T, …) -> R` from a function literal's AST.
fn function_lit_signature(
    name: &str,
    lit: &ast::FunctionLit,
    doc: Option<Documentation>,
) -> SignatureInformation {
    let params: Vec<(String, Option<String>)> = lit
        .params
        .iter()
        .map(|p| (format!("{}: {}", p.name, p.ty), None))
        .collect();
    assemble(name, &params, &lit.return_ty.to_string(), doc)
}

/// Build the `name(p1, p2) -> ret` label, recording each parameter's
/// byte offsets while assembling so editors highlight precisely.
fn assemble(
    name: &str,
    params: &[(String, Option<String>)],
    return_type: &str,
    documentation: Option<Documentation>,
) -> SignatureInformation {
    let mut label = format!("{name}(");
    let mut infos = Vec::with_capacity(params.len());
    for (i, (param, doc)) in params.iter().enumerate() {
        if i > 0 {
            label.push_str(", ");
        }
        let start = label.len() as u32;
        label.push_str(param);
        infos.push(ParameterInformation {
            label: ParameterLabel::LabelOffsets([start, label.len() as u32]),
            documentation: doc.clone().map(doc_markup),
        });
    }
    label.push(')');
    if !return_type.is_empty() {
        label.push_str(" -> ");
        label.push_str(return_type);
    }
    SignatureInformation {
        label,
        documentation,
        parameters: Some(infos),
        active_parameter: None,
    }
}

fn plain_signature(label: String, documentation: Option<Documentation>) -> SignatureInformation {
    SignatureInformation {
        label,
        documentation,
        parameters: Some(Vec::new()),
        active_parameter: None,
    }
}

fn doc_text(doc: &str) -> Option<Documentation> {
    (!doc.is_empty()).then(|| Documentation::String(doc.to_string()))
}

fn doc_markup(doc: String) -> Documentation {
    Documentation::String(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call_at(src: &str, marker: &str) -> Option<CallContext> {
        let offset = src.find(marker).expect("marker present") + marker.len();
        enclosing_call(src, offset)
    }

    #[test]
    fn finds_call_and_counts_commas() {
        let cases = [
            ("x = clamp(", "clamp(", "clamp", 0),
            ("x = clamp(1, ", "1, ", "clamp", 1),
            ("x = outer(inner(1, 2), ", "2), ", "outer", 1),
            ("x = shared.scale(", "scale(", "shared.scale", 0),
        ];
        for (src, marker, callee, param) in cases {
            let call = call_at(src, marker).unwrap_or_else(|| panic!("no call in {src:?}"));
            assert_eq!(call.callee, callee, "callee in {src:?}");
            assert_eq!(call.active_param, param, "active param in {src:?}");
        }
    }

    #[test]
    fn commas_inside_nested_brackets_do_not_count() {
        let call = call_at("x = clamp([1, 2, 3], ", "3], ").expect("call found");
        assert_eq!(call.callee, "clamp");
        assert_eq!(call.active_param, 1);
        let call = call_at("x = pick({ a: 1, b: 2 }, ", "}, ").expect("call found");
        assert_eq!(call.active_param, 1);
    }

    #[test]
    fn strings_and_comments_are_skipped() {
        // The paren and comma inside the string don't open a call frame.
        let call = call_at("x = len(\"a(b,\", ", "\", ").expect("call found");
        assert_eq!(call.callee, "len");
        assert_eq!(call.active_param, 1);
        // A commented-out `(` doesn't open a frame either.
        assert!(call_at("// open(\nx = 1", "x = 1").is_none());
    }

    #[test]
    fn closed_call_and_grouping_paren_yield_nothing() {
        assert!(call_at("x = len(\"a\") + 1", "+ 1").is_none());
        // A bare grouping paren has no callee; an enclosing call wins.
        let call = call_at("x = max(1, (2 + ", "2 + ").expect("outer call");
        assert_eq!(call.callee, "max");
        assert_eq!(call.active_param, 1);
    }

    #[test]
    fn builtin_signature_resolves_with_param_offsets() {
        let src = "x = clamp(";
        let help = signature_help(src, "test.wcl", src.len(), None, &HashMap::new())
            .expect("builtin help");
        let sig = &help.signatures[0];
        assert!(sig.label.starts_with("clamp("), "{}", sig.label);
        let params = sig.parameters.as_ref().expect("params");
        assert!(!params.is_empty());
        // Every offset pair slices an in-bounds, comma-free label chunk.
        for p in params {
            let ParameterLabel::LabelOffsets([s, e]) = p.label else {
                panic!("expected offsets");
            };
            let chunk = &sig.label[s as usize..e as usize];
            assert!(!chunk.contains(','), "chunk {chunk:?}");
        }
        assert_eq!(help.active_parameter, Some(0));
    }

    #[test]
    fn user_fn_item_signature_tracks_active_param() {
        let src = "fn add(a: i64, b: i64) -> i64 { a + b }\nx = add(1, ";
        let help = signature_help(src, "test.wcl", src.len(), None, &HashMap::new())
            .expect("fn item help");
        let sig = &help.signatures[0];
        assert_eq!(sig.label, "add(a: i64, b: i64) -> i64");
        assert_eq!(help.active_parameter, Some(1));
    }

    #[test]
    fn local_closure_signature_resolves() {
        let src = "x = {\n  let scale = fn(v: f64) -> f64 v * 2.0;\n  scale(";
        let help = signature_help(src, "test.wcl", src.len(), None, &HashMap::new())
            .expect("closure help");
        assert!(help.signatures[0].label.starts_with("scale(v: f64)"));
    }
}
