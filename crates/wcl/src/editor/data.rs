//! The editor's Data mode: schema-driven tables over data-object types.
//!
//! A `@block` type opts in with `@wdoc.editable(file?)` (stdlib
//! `file_placement.wcl`); the editor then shows one table of all its
//! instances with add / modify / remove. `GET /api/data/types` enumerates
//! the registered types with the same field metadata the create forms use;
//! `GET /api/data/rows` lists a kind's instances with per-cell literal /
//! computed classification (the row editor reuses `/api/block/ops`; adds go
//! through `/api/unit/create` with the resolved target `file`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::State;
use axum::http::Uri;
use axum::response::Response;

use wcl_lang::ast::{self, Item};
use wcl_lang::{DeclName, Document, parse_for_edit};

use super::blocks::{dec_first_string, first_label, kind_entry, visibility_json};
use super::{EditorState, run_blocking};
use crate::serve::{query_param, sandboxed};

/// The last segment of a decorator name when it's bare or `wdoc.`-qualified.
fn dec_name<'a>(d: &wcl_lang::Decorator<'a>) -> Option<&'a str> {
    match d.name_segments() {
        [n] => Some(n.as_str()),
        [ns, n] if ns == "wdoc" => Some(n.as_str()),
        _ => None,
    }
}

/// Every `@wdoc.editable` type: `(kind, target file hint, TypeDecl)`.
fn editable_types<'a>(doc: &'a Document) -> Vec<(String, Option<String>, wcl_lang::TypeDecl<'a>)> {
    let mut out = Vec::new();
    for decl in doc.type_decls() {
        let mut editable = None;
        let mut kind = None;
        let mut file_hint = None;
        for d in decl.decorators() {
            match dec_name(&d) {
                Some("editable") => editable = Some(dec_first_string(&d)),
                Some("block") | Some("table") => kind = dec_first_string(&d),
                Some("file") => file_hint = dec_first_string(&d),
                _ => {}
            }
        }
        let Some(editable_file) = editable else {
            continue;
        };
        let Some(kind) = kind else { continue };
        out.push((kind, editable_file.or(file_hint), decl));
    }
    out
}

/// `GET /api/data/types?entry=…&page_file=…` → the registered types with
/// their form metadata and resolved target file for new rows.
pub(super) async fn handle_data_types(State(state): State<Arc<EditorState>>, uri: Uri) -> Response {
    let entry = query_param(&uri, "entry");
    let page_file = query_param(&uri, "page_file");
    let state2 = Arc::clone(&state);
    run_blocking(move || {
        let entry = entry.ok_or("missing entry")?;
        data_types(&state2, &entry, page_file.as_deref())
    })
    .await
}

fn resolve_doc_entry(
    state: &EditorState,
    entry: &str,
    page_file: Option<&str>,
) -> Result<PathBuf, String> {
    let entry_abs = sandboxed(&state.root_dir, &state.root_dir.join(entry))
        .ok_or_else(|| format!("file outside the served tree: {entry}"))?;
    Ok(page_file
        .filter(|s| !s.is_empty())
        .and_then(|pf| sandboxed(&state.root_dir, Path::new(pf)))
        .map(|pf| wcl_wdoc::doc_entry_for_page(&entry_abs, &pf))
        .unwrap_or(entry_abs))
}

fn data_types(
    state: &EditorState,
    entry: &str,
    page_file: Option<&str>,
) -> Result<serde_json::Value, String> {
    let doc_entry = resolve_doc_entry(state, entry, page_file)?;
    let doc = wcl_wdoc::open_doc_for_edit(&doc_entry).map_err(|e| e.to_string())?;
    let entry_dir = doc_entry.parent().unwrap_or(&doc_entry);
    let types: Vec<serde_json::Value> = editable_types(&doc)
        .into_iter()
        .map(|(kind, file_hint, decl)| {
            let mut entry_json = kind_entry(&doc, &kind, &decl);
            // The target for new rows, repo-relative (hints are
            // entry-relative by convention).
            let file = file_hint.map(|f| {
                let abs = entry_dir.join(&f);
                std::fs::canonicalize(&abs)
                    .unwrap_or(abs)
                    .strip_prefix(&state.root_dir)
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or(f)
            });
            entry_json["file"] = serde_json::json!(file);
            entry_json["type_name"] = serde_json::json!(decl.name());
            entry_json
        })
        .collect();
    Ok(serde_json::json!({ "ok": true, "types": types }))
}

/// `GET /api/data/rows?entry=…&kind=…` → every instance of the kind, one
/// row per block, cells classified literal / computed off the declaring
/// file's AST (spans + etags ready for `/api/block/ops`).
pub(super) async fn handle_data_rows(State(state): State<Arc<EditorState>>, uri: Uri) -> Response {
    let entry = query_param(&uri, "entry");
    let page_file = query_param(&uri, "page_file");
    let kind = query_param(&uri, "kind");
    let state2 = Arc::clone(&state);
    run_blocking(move || {
        let entry = entry.ok_or("missing entry")?;
        let kind = kind.ok_or("missing kind")?;
        data_rows(&state2, &entry, page_file.as_deref(), &kind)
    })
    .await
}

fn data_rows(
    state: &EditorState,
    entry: &str,
    page_file: Option<&str>,
    kind: &str,
) -> Result<serde_json::Value, String> {
    let doc_entry = resolve_doc_entry(state, entry, page_file)?;
    let doc = wcl_wdoc::open_doc_for_edit(&doc_entry).map_err(|e| e.to_string())?;
    let mut asts: HashMap<PathBuf, (String, ast::Source)> = HashMap::new();
    let mut rows: Vec<serde_json::Value> = Vec::new();
    for (path, b) in doc.blocks_with_source() {
        if b.kind() != kind {
            continue;
        }
        let file = path
            .map(Path::to_path_buf)
            .unwrap_or_else(|| doc_entry.clone());
        if !asts.contains_key(&file) {
            let text = crate::edit::read(&file)?;
            let src =
                parse_for_edit(&text, file.display().to_string()).map_err(|e| e.to_string())?;
            asts.insert(file.clone(), (text, src));
        }
        let (text, src) = &asts[&file];
        let span = b.span();
        let Some(blk) = super::find_block_at(&src.items, span) else {
            continue;
        };
        let labels: Vec<serde_json::Value> = blk
            .labels
            .iter()
            .enumerate()
            .map(|(slot, e)| {
                let mut cell = classify_cell(e);
                cell["slot"] = serde_json::json!(slot);
                cell
            })
            .collect();
        let cells: serde_json::Map<String, serde_json::Value> = blk
            .items
            .iter()
            .filter_map(|it| match it {
                Item::Field(f) => Some((f.name.clone(), classify_cell(&f.expr))),
                _ => None,
            })
            .collect();
        rows.push(serde_json::json!({
            "label": first_label(&b),
            "file": std::fs::canonicalize(&file)
                .unwrap_or(file.clone())
                .strip_prefix(&state.root_dir)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| file.display().to_string()),
            "span": super::span_json(span),
            "etag": crate::edit::content_etag(text),
            "labels": labels,
            "cells": cells,
            "visibility": visibility_json(blk),
        }));
    }
    Ok(serde_json::json!({ "ok": true, "kind": kind, "rows": rows }))
}

/// Table-cell classification — richer than the prose path's: scalar
/// literals (numbers, bools, symbols, identifiers) are editable too, with
/// `expr: true` telling the client to write them back as parsed WCL rather
/// than a quoted string.
fn classify_cell(e: &wcl_lang::ast::Expr) -> serde_json::Value {
    use wcl_lang::ast::Expr;
    let (state, text, expr) = match e {
        Expr::Utf8(s) | Expr::Ascii(s) => ("literal", Some(s.clone()), false),
        Expr::Identifier(s, _) => ("literal", Some(s.clone()), true),
        Expr::Symbol(s) => ("literal", Some(format!(":{s}")), true),
        Expr::Bool(b) => ("literal", Some(b.to_string()), true),
        Expr::I64(n) => ("literal", Some(n.to_string()), true),
        Expr::U64(n) => ("literal", Some(n.to_string()), true),
        Expr::F64(n) => ("literal", Some(n.to_string()), true),
        _ => ("computed", None, false),
    };
    serde_json::json!({ "state": state, "text": text, "expr": expr })
}
