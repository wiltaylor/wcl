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
use wcl_lang::{Document, parse_for_edit};

use super::blocks::{dec_first_string, first_label, visibility_json};
use super::kinds::KindModel;
use super::{EditorState, Workspace, run_blocking};
use crate::serve::query_param;

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
        data_types(&state2.ws, &entry, page_file.as_deref())
    })
    .await
}

fn data_types(
    ws: &Workspace,
    entry: &str,
    page_file: Option<&str>,
) -> Result<serde_json::Value, String> {
    let doc_entry = ws.doc_entry(entry, page_file)?;
    let doc = wcl_wdoc::open_doc_for_edit(&doc_entry).map_err(super::err_str)?;
    let model = KindModel::new(&doc);
    let entry_dir = doc_entry.parent().unwrap_or(&doc_entry);
    // The same kind shape every other endpoint serves — including
    // `type_name`, the FULLY-QUALIFIED schema name the create path matches
    // on. (Data mode used to serve the short name here; forwarding that
    // would silently fall back to a name-only lookup, which is the
    // namespace collision `type_name` exists to prevent.)
    let types: Vec<serde_json::Value> = editable_types(&doc)
        .into_iter()
        .map(|(kind, file_hint, decl)| {
            let mut entry_json = model.describe_decl(&kind, decl).json();
            // The target for new rows, repo-relative (hints are
            // entry-relative by convention).
            let file = file_hint.map(|f| {
                let abs = entry_dir.join(&f);
                std::fs::canonicalize(&abs)
                    .unwrap_or(abs)
                    .strip_prefix(ws.root_dir())
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or(f)
            });
            entry_json["file"] = serde_json::json!(file);
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
        data_rows(&state2.ws, &entry, page_file.as_deref(), &kind)
    })
    .await
}

fn data_rows(
    ws: &Workspace,
    entry: &str,
    page_file: Option<&str>,
    kind: &str,
) -> Result<serde_json::Value, String> {
    let doc_entry = ws.doc_entry(entry, page_file)?;
    let doc = wcl_wdoc::open_doc_for_edit(&doc_entry).map_err(super::err_str)?;
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
            let src = parse_for_edit(&text, file.display().to_string()).map_err(super::err_str)?;
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
                .strip_prefix(ws.root_dir())
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
pub(super) fn classify_cell(e: &wcl_lang::ast::Expr) -> serde_json::Value {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::blocks::{block_ops, unit_create};
    use crate::editor::preview::Sessions;

    /// The Data mode surface: `@wdoc.editable` registers a type; rows list
    /// with cell classification; adds honour the decorator's target file
    /// (creating + importing it on first use); edits/deletes reuse block ops.
    #[test]
    fn types_rows_and_crud() {
        let doc = "import <wdoc.wcl>\n\n\
            @document\ntype Doc {\n  @children(\"character\") characters: list<Character>\n}\n\n\
            @block(\"character\") @wdoc.editable(\"data/characters.wcl\")\ntype Character {\n  @inline(0) id: identifier\n  name: utf8\n  hp: i64?\n}\n\n\
            site docs {\n  title = \"D\"\n  root = true\n}\n\n\
            character hero {\n  name = \"Hero\"\n  hp = 10\n}\n\n\
            page index {\n  title = \"Hi\"\n\n  h1 \"Hi\"\n}\n";
        let (_td, ws) = crate::editor::testsupport::workspace_with(doc);
        let root = ws.root_dir().to_path_buf();
        let previews = Sessions::default();
        let rows = || data_rows(&ws, "main.wcl", None, "character").expect("rows");

        // Types: the registered kind with metadata + resolved target file.
        let v = data_types(&ws, "main.wcl", None).expect("types");
        let types = v["types"].as_array().unwrap();
        assert_eq!(types.len(), 1, "{v:#}");
        assert_eq!(types[0]["kind"], "character");
        assert_eq!(types[0]["file"], "data/characters.wcl");
        assert!(
            types[0]["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["name"] == "hp")
        );

        // Rows: the existing instance, cells classified.
        let v = rows();
        let list = v["rows"].as_array().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["label"], "hero");
        assert_eq!(list[0]["cells"]["name"]["state"], "literal");
        assert_eq!(list[0]["cells"]["name"]["text"], "Hero");
        // Numbers are editable too — written back as parsed WCL.
        assert_eq!(list[0]["cells"]["hp"]["state"], "literal");
        assert_eq!(list[0]["cells"]["hp"]["text"], "10");
        assert_eq!(list[0]["cells"]["hp"]["expr"], true);

        // Add a row into the decorator's target file (created + imported).
        let v = unit_create(
            &ws,
            &previews,
            &serde_json::json!({
                "entry": "main.wcl",
                "unit": { "kind": "character", "id": "villain",
                          "fields": { "name": "Villain", "hp": 13 },
                          "file": "data/characters.wcl" },
            }),
        )
        .expect("create");
        assert_eq!(v["file"], "data/characters.wcl");
        let created = std::fs::read_to_string(root.join("data/characters.wcl")).unwrap();
        assert!(created.contains("character villain"), "{created}");
        assert!(created.contains("hp = 13"), "{created}");
        let main = std::fs::read_to_string(root.join("main.wcl")).unwrap();
        assert!(main.contains("import \"data/characters.wcl\""), "{main}");

        // Both rows list now; edit one cell + delete the other via block ops.
        let v = rows();
        let list = v["rows"].as_array().unwrap();
        assert_eq!(list.len(), 2, "{v:#}");
        let villain = list.iter().find(|r| r["label"] == "villain").unwrap();
        block_ops(
            &ws,
            &previews,
            &serde_json::json!({
                "entry": "main.wcl", "file": villain["file"], "etag": villain["etag"],
                "ops": [{ "op": "set_field", "span": villain["span"],
                          "field": "name", "text": "Big Bad" }],
            }),
        )
        .expect("set_field");
        assert!(
            std::fs::read_to_string(root.join("data/characters.wcl"))
                .unwrap()
                .contains("name = \"Big Bad\""),
        );
        let v = rows();
        let hero = v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["label"] == "hero")
            .unwrap();
        block_ops(
            &ws,
            &previews,
            &serde_json::json!({
                "entry": "main.wcl", "file": hero["file"],
                "ops": [{ "op": "delete", "span": hero["span"] }],
            }),
        )
        .expect("delete");
        assert_eq!(rows()["rows"].as_array().unwrap().len(), 1);
    }
}
