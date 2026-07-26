//! Block-level editing endpoints for the editor's Design mode (WYSIWYG).
//!
//! Every mutation composes the same pipeline `wcl set` / `wcl answer` use:
//! [`parse_for_edit`] → span-addressed AST mutators ([`wcl_lang::edit`]) →
//! [`wcl_format::to_source`] → [`crate::edit::commit`] (write →
//! schema-validate → rollback). Requests address blocks by the byte spans
//! stamped into the edit-mode preview (`data-wcl-span` / `data-wcl-file`),
//! guarded by a content etag against the on-disk bytes. Because a commit
//! canonically reformats the whole file (shifting every span), each mutating
//! response carries the post-format spans of the touched blocks plus the full
//! new file text, so the client can re-anchor without re-reading.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::State;
use axum::http::{StatusCode, Uri};
use axum::response::Response;

use wcl_lang::ast::{self, Expr, Item};
use wcl_lang::{
    DeclName, Document, ResolvedType, Span, Value, edit as ast_edit, format as wcl_format,
    parse_expr, parse_for_edit,
};

use super::{EditorState, run_blocking};
use crate::serve::{json_error, parse_json_body, query_param, sandboxed};

// ---------------------------------------------------------------------------
// Request context
// ---------------------------------------------------------------------------

/// Sandbox-check a repo-relative file from the request body.
fn file_field(state: &EditorState, v: &serde_json::Value) -> Result<PathBuf, String> {
    let file = crate::edit::str_field(v, "file")?;
    sandboxed(&state.root_dir, &state.root_dir.join(file))
        .ok_or_else(|| format!("file outside the served tree: {file}"))
}

/// A `{start, end}` byte span from a JSON object field.
pub(super) fn span_field(v: &serde_json::Value, key: &str) -> Result<Span, String> {
    let s = v.get(key).ok_or_else(|| format!("missing `{key}`"))?;
    let num = |k: &str| {
        s.get(k)
            .and_then(serde_json::Value::as_u64)
            .map(|n| n as usize)
            .ok_or_else(|| format!("missing `{key}.{k}`"))
    };
    Ok(Span::new(num("start")?, num("end")?))
}

/// A file path made repo-relative with `/` separators (the client's view).
fn rel_path(state: &EditorState, file: &Path) -> Result<String, String> {
    let canon = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    canon
        .strip_prefix(&state.root_dir)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .map_err(|_| {
            format!(
                "{} is outside the served directory — not editable here",
                canon.display()
            )
        })
}

// ---------------------------------------------------------------------------
// `POST /api/block/source` — read a block's source + slot classification
// ---------------------------------------------------------------------------

/// Body: `{ file, span: {start, end} }` → the block's exact source slice
/// plus a per-slot classification (`literal` slots carry their text and are
/// inline-editable; `computed` slots — interpolations, expressions — lock the
/// client to the fragment editor).
pub(super) async fn handle_block_source(
    State(state): State<Arc<EditorState>>,
    body: String,
) -> Response {
    let v = match parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    let state2 = Arc::clone(&state);
    run_blocking(move || block_source(&state2, &v)).await
}

fn block_source(state: &EditorState, v: &serde_json::Value) -> Result<serde_json::Value, String> {
    let file_abs = file_field(state, v)?;
    let text = crate::edit::read(&file_abs)?;
    let span = span_field(v, "span")?;
    let mut src = parse_for_edit(&text, file_abs.display().to_string()).map_err(super::err_str)?;
    let block = ast_edit::find_block_by_span(&mut src.items, span)
        .ok_or("no block at that span — the file changed; rebuild the preview")?;
    let source = text
        .get(span.start..span.end)
        .ok_or("span out of bounds — the file changed; rebuild the preview")?;
    let labels: Vec<serde_json::Value> = block
        .labels
        .iter()
        .enumerate()
        .map(|(slot, e)| {
            let (state, text) = classify_expr(e);
            serde_json::json!({ "slot": slot, "state": state, "text": text })
        })
        .collect();
    let fields: serde_json::Map<String, serde_json::Value> = block
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Field(f) => Some((f.name.clone(), field_json(&f.expr))),
            _ => None,
        })
        .collect();
    // `a -> b` statements are items, not fields, so they'd be invisible to
    // the client otherwise — the edge editors read the wiring from here.
    let connections: Vec<serde_json::Value> = block
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Connection(c) => Some(serde_json::json!({
                "from": c.lhs, "to": c.rhs, "kind": c.kind,
            })),
            _ => None,
        })
        .collect();
    Ok(serde_json::json!({
        "ok": true,
        "kind": block.kind,
        "source": source,
        "etag": crate::edit::content_etag(&text),
        "labels": labels,
        "fields": fields,
        "connections": connections,
        "visibility": visibility_json(block),
    }))
}

/// Field-value JSON for `/api/block/source`: the scalar classification,
/// plus structured contents for all-string-literal lists — `state: "list"`
/// with `items` for `header = ["…", …]`, `state: "rows"` with `rows` for
/// `rows = [["…", …], …]` — so the Design-mode table editor can grid-edit
/// list-literal tables, not just pipe rows.
fn field_json(e: &Expr) -> serde_json::Value {
    let (state, text) = classify_expr(e);
    let mut v = serde_json::json!({ "state": state, "text": text });
    let strings = |es: &[Expr]| -> Option<Vec<String>> {
        es.iter()
            .map(|e| match e {
                Expr::Utf8(s) | Expr::Ascii(s) => Some(s.clone()),
                _ => None,
            })
            .collect()
    };
    if let Expr::ListLit { elements, .. } = e {
        if !elements.is_empty()
            && let Some(items) = strings(elements)
        {
            v["state"] = "list".into();
            v["items"] = serde_json::json!(items);
        } else if let Some(rows) = elements
            .iter()
            .map(|e| match e {
                Expr::ListLit { elements, .. } => strings(elements),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
        {
            v["state"] = "rows".into();
            v["rows"] = serde_json::json!(rows);
        }
    }
    v
}

/// `literal` (plain string, editable in place), `number` / `bool` /
/// `symbol` (scalar literals — form-editable, written back as exprs), vs
/// `computed` (interpolation / expression — fragment-editor only). Callers
/// that can only write strings gate on `literal`; the shape properties
/// editors additionally consume the scalar states.
pub(super) fn classify_expr(e: &Expr) -> (&'static str, Option<String>) {
    match e {
        Expr::Utf8(s) | Expr::Ascii(s) => ("literal", Some(s.clone())),
        Expr::Bool(b) => ("bool", Some(b.to_string())),
        Expr::Symbol(s) => ("symbol", Some(s.clone())),
        Expr::I8(n) => ("number", Some(n.to_string())),
        Expr::I16(n) => ("number", Some(n.to_string())),
        Expr::I32(n) => ("number", Some(n.to_string())),
        Expr::I64(n) => ("number", Some(n.to_string())),
        Expr::I128(n) => ("number", Some(n.to_string())),
        Expr::Isize(n) => ("number", Some(n.to_string())),
        Expr::U8(n) => ("number", Some(n.to_string())),
        Expr::U16(n) => ("number", Some(n.to_string())),
        Expr::U32(n) => ("number", Some(n.to_string())),
        Expr::U64(n) => ("number", Some(n.to_string())),
        Expr::U128(n) => ("number", Some(n.to_string())),
        Expr::Usize(n) => ("number", Some(n.to_string())),
        Expr::F32(n) => ("number", Some(n.to_string())),
        Expr::F64(n) => ("number", Some(n.to_string())),
        _ => ("computed", None),
    }
}

/// The block's visibility state as the toggles UI understands it: the
/// `@except(sites = [:…])` symbol list, plus a `custom` flag for anything
/// richer (`@only`, positional args, `templates`/`backends` axes, computed
/// lists) — the UI defers those to the fragment editor.
pub(super) fn visibility_json(block: &ast::Block) -> serde_json::Value {
    let mut except_sites: Vec<String> = Vec::new();
    let mut custom = false;
    for d in &block.decorators {
        let name = match d.name.as_slice() {
            [n] => n.as_str(),
            [ns, n] if ns == "wdoc" => n.as_str(),
            _ => continue,
        };
        match name {
            "only" => custom = true,
            "except" => {
                if !d.positional.is_empty() {
                    custom = true;
                }
                for arg in &d.named {
                    if arg.name != "sites" {
                        custom = true;
                        continue;
                    }
                    match &arg.value {
                        Expr::ListLit { elements, .. }
                            if elements.iter().all(|e| matches!(e, Expr::Symbol(_))) =>
                        {
                            except_sites.extend(elements.iter().filter_map(|e| match e {
                                Expr::Symbol(s) => Some(s.clone()),
                                _ => None,
                            }));
                        }
                        _ => custom = true,
                    }
                }
            }
            _ => {}
        }
    }
    serde_json::json!({ "except_sites": except_sites, "custom": custom })
}

// ---------------------------------------------------------------------------
// `POST /api/block/ops` — a batch of span-addressed mutations on one file
// ---------------------------------------------------------------------------

/// Body: `{ entry, page_file?, file, etag?, ops: [...] }`. All ops apply to
/// a single parse of the file, so their spans all refer to the *pre-edit*
/// bytes — this is what makes "commit text + insert sibling" atomic. Op
/// union (each also carries `span`):
///
/// - `set_label { slot, text | expr }` — an inline-label slot
/// - `set_field { field, text | expr }` — a named field (insert-or-update)
/// - `set_kind { kind }` — rewrite the block kind, keeping labels/fields
/// - `replace_source { source }` — swap the whole block for a fragment
/// - `insert_after { source }` — a new sibling block after the target
/// - `insert_child { index, source }` — a new child at block-index `index`
/// - `append_top_level { source }` — a new top-level block (no `span`)
/// - `set_visibility { except_sites: [names] }` — rewrite the block's
///   `@except(sites = [:…])` decorator (empty list removes it)
/// - `connect_add { from, to, kind? }` / `connect_remove { from, to }` — add or
///   drop an `a -> b` connection statement on the addressed container block
///   (a diagram wiring its shapes, a procedure wiring its steps).
/// - `related_add { id }` / `related_remove { id }` — append to / remove from
///   the block's `related` identifier list (the graph view's edge writes;
///   refuses computed lists, duplicates, and self-loops)
/// - `delete {}` — remove the block
/// - `move { dir: "up"|"down" }` — swap with the adjacent block sibling
/// - `move_to { before: span | after: span }` — move relative to another
///   block, resolved at the common-ancestor level (a transparent wrapper
///   like `edit_field` moves with its child; invisible AST siblings —
///   `project` splices, view-hidden blocks — never skew the position)
pub(super) async fn handle_block_ops(
    State(state): State<Arc<EditorState>>,
    body: String,
) -> Response {
    let v = match parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    let state2 = Arc::clone(&state);
    run_blocking(move || block_ops(&state2, &v)).await
}

fn block_ops(state: &EditorState, v: &serde_json::Value) -> Result<serde_json::Value, String> {
    let doc_entry = super::resolve_doc_entry_from(state, v)?;
    let file_abs = file_field(state, v)?;
    let disk = crate::edit::read(&file_abs)?;
    if let Some(etag) = v.get("etag").and_then(serde_json::Value::as_str)
        && etag != crate::edit::content_etag(&disk)
    {
        return Err("conflict: the file changed on disk — rebuild the preview and retry".into());
    }
    let mut src = parse_for_edit(&disk, file_abs.display().to_string()).map_err(super::err_str)?;

    let ops = v
        .get("ops")
        .and_then(serde_json::Value::as_array)
        .filter(|a| !a.is_empty())
        .ok_or("missing ops")?;
    // Markers for the response spans: edited blocks keep their pre-edit span
    // on the AST node (mutators never touch `span`); inserted blocks get a
    // unique sentinel span so each is findable after the batch.
    let mut tracked: Vec<(&'static str, Span)> = Vec::new();
    let mut inserts = 0usize;
    let mut sentinel = || {
        inserts += 1;
        Span::new(usize::MAX - inserts, usize::MAX)
    };
    for op in ops {
        let name = crate::edit::str_field(op, "op")?;
        match name {
            "set_label" => {
                let span = span_field(op, "span")?;
                let slot = op
                    .get("slot")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or("missing `slot`")? as usize;
                let expr = value_expr(op)?;
                let block = find_block(&mut src.items, span)?;
                if !ast_edit::set_label(block, slot, expr) {
                    return Err(format!("label slot {slot} is past the block's labels"));
                }
                tracked.push(("edited", span));
            }
            "set_field" => {
                let span = span_field(op, "span")?;
                let field = crate::edit::str_field(op, "field")?;
                let expr = value_expr(op)?;
                let block = find_block(&mut src.items, span)?;
                ast_edit::set_or_insert_field(block, field, expr);
                tracked.push(("edited", span));
            }
            "remove_field" => {
                let span = span_field(op, "span")?;
                let field = crate::edit::str_field(op, "field")?;
                let block = find_block(&mut src.items, span)?;
                // Tolerant of an absent field so callers can batch removals
                // (e.g. reset-position dropping x/y/width/height) blindly.
                ast_edit::remove_field(block, field);
                tracked.push(("edited", span));
            }
            "set_kind" => {
                let span = span_field(op, "span")?;
                let kind = crate::edit::str_field(op, "kind")?;
                if !is_identifier(kind) {
                    return Err(format!("`{kind}` is not a valid block kind"));
                }
                let block = find_block(&mut src.items, span)?;
                block.kind = kind.to_string();
                tracked.push(("edited", span));
            }
            "replace_source" => {
                let span = span_field(op, "span")?;
                let block = parse_fragment(crate::edit::str_field(op, "source")?)?;
                if !ast_edit::replace_block_by_span(&mut src.items, span, block) {
                    return Err(stale_span());
                }
                // `replace_block_by_span` keeps the old span on the new node.
                tracked.push(("edited", span));
            }
            "insert_after" => {
                let span = span_field(op, "span")?;
                let mut block = parse_fragment(crate::edit::str_field(op, "source")?)?;
                let mark = sentinel();
                block.span = mark;
                if !ast_edit::insert_block_after_span(&mut src.items, span, block) {
                    return Err(stale_span());
                }
                tracked.push(("inserted", mark));
            }
            "insert_child" => {
                let span = span_field(op, "span")?;
                let index = op
                    .get("index")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or("missing `index`")? as usize;
                let mut block = parse_fragment(crate::edit::str_field(op, "source")?)?;
                let mark = sentinel();
                block.span = mark;
                let parent = find_block(&mut src.items, span)?;
                ast_edit::insert_block_at_index(&mut parent.items, index, block);
                tracked.push(("inserted", mark));
            }
            "append_top_level" => {
                let mut block = parse_fragment(crate::edit::str_field(op, "source")?)?;
                let mark = sentinel();
                block.span = mark;
                ast_edit::append_top_level_block(&mut src, block);
                tracked.push(("inserted", mark));
            }
            "set_visibility" => {
                let span = span_field(op, "span")?;
                let sites: Vec<String> = op
                    .get("except_sites")
                    .and_then(serde_json::Value::as_array)
                    .ok_or("missing `except_sites`")?
                    .iter()
                    .filter_map(|s| s.as_str().map(str::to_string))
                    .collect();
                if let Some(bad) = sites.iter().find(|s| !is_identifier(s)) {
                    return Err(format!("`{bad}` is not a valid site name"));
                }
                let block = find_block(&mut src.items, span)?;
                if visibility_json(block)["custom"] == true {
                    return Err(
                        "the block has custom visibility (@only / other axes) — edit its source instead"
                            .into(),
                    );
                }
                let named = if sites.is_empty() {
                    Vec::new()
                } else {
                    vec![(
                        "sites".to_string(),
                        Expr::ListLit {
                            elem_trivia: sites.iter().map(|_| Default::default()).collect(),
                            elements: sites.into_iter().map(Expr::Symbol).collect(),
                            trailing_trivia: Vec::new(),
                            span: Span::new(0, 0),
                        },
                    )]
                };
                ast_edit::set_or_remove_decorator(block, "except", named);
                tracked.push(("edited", span));
            }
            "related_add" | "related_remove" => {
                let span = span_field(op, "span")?;
                let id = crate::edit::str_field(op, "id")?;
                if !is_identifier(id) {
                    return Err(format!("`{id}` is not a valid unit id"));
                }
                let block = find_block(&mut src.items, span)?;
                if ast_label(block).as_deref() == Some(id) {
                    return Err("a unit cannot relate to itself".into());
                }
                let current: Vec<String> = match block.items.iter().find_map(|it| match it {
                    Item::Field(f) if f.name == "related" => Some(&f.expr),
                    _ => None,
                }) {
                    Some(Expr::ListLit { elements, .. }) => elements
                        .iter()
                        .filter_map(|e| match e {
                            Expr::Identifier(s, _) => Some(s.clone()),
                            Expr::Utf8(s) | Expr::Ascii(s) => Some(s.clone()),
                            _ => None,
                        })
                        .collect(),
                    Some(_) => {
                        return Err(
                            "the block's related list is computed — edit its source instead".into(),
                        );
                    }
                    None => Vec::new(),
                };
                let next: Vec<String> = if name == "related_add" {
                    if current.iter().any(|s| s == id) {
                        return Err(format!("already related to `{id}`"));
                    }
                    let mut n = current;
                    n.push(id.to_string());
                    n
                } else {
                    if !current.iter().any(|s| s == id) {
                        return Err(format!("`{id}` is not in the related list"));
                    }
                    current.into_iter().filter(|s| s != id).collect()
                };
                ast_edit::set_or_insert_field(
                    block,
                    "related",
                    Expr::ListLit {
                        elem_trivia: next.iter().map(|_| Default::default()).collect(),
                        elements: next
                            .into_iter()
                            .map(|s| Expr::Identifier(s, Span::new(0, 0)))
                            .collect(),
                        trailing_trivia: Vec::new(),
                        span: Span::new(0, 0),
                    },
                );
                tracked.push(("edited", span));
            }
            // A diagram/procedure wires its children with `a -> b` connection
            // STATEMENTS, not a list field — so these address the container
            // block (the diagram, the procedure) and name its children by id.
            "connect_add" | "connect_remove" => {
                let span = span_field(op, "span")?;
                let from = crate::edit::str_field(op, "from")?;
                let to = crate::edit::str_field(op, "to")?;
                for id in [from, to] {
                    if !is_identifier(id) {
                        return Err(format!("`{id}` is not a valid shape id"));
                    }
                }
                if from == to {
                    return Err("a shape cannot connect to itself".into());
                }
                let block = find_block(&mut src.items, span)?;
                if name == "connect_add" {
                    // The kind is a bare symbol (`a -> b :yes`); the schema
                    // rejects one outside the connection's symbol set.
                    let kind = op.get("kind").and_then(serde_json::Value::as_str);
                    if let Some(k) = kind
                        && !is_identifier(k)
                    {
                        return Err(format!("`{k}` is not a valid connection kind"));
                    }
                    if !ast_edit::add_connection(block, from, to, kind) {
                        return Err(format!("`{from}` is already connected to `{to}`"));
                    }
                } else if !ast_edit::remove_connection(block, from, to) {
                    return Err(format!("no connection `{from} -> {to}`"));
                }
                tracked.push(("edited", span));
            }
            "delete" => {
                let span = span_field(op, "span")?;
                // A shape's edges name it by id, so deleting the shape must
                // take them with it — an edge to a shape that no longer exists
                // renders nothing and warns at build time.
                let orphan = find_block(&mut src.items, span)
                    .ok()
                    .and_then(|b| shape_id_of(b));
                if !ast_edit::remove_block_by_span(&mut src.items, span) {
                    return Err(stale_span());
                }
                if let Some(id) = orphan {
                    prune_connections(&mut src.items, &id);
                }
            }
            "move" => {
                let span = span_field(op, "span")?;
                let down = match crate::edit::str_field(op, "dir")? {
                    "down" => true,
                    "up" => false,
                    other => return Err(format!("bad move dir `{other}`")),
                };
                if !ast_edit::move_block_by_span(&mut src.items, span, down) {
                    return Err("the block is already at the edge".into());
                }
                tracked.push(("edited", span));
            }
            "move_to" => {
                let span = span_field(op, "span")?;
                let (target, before) = if op.get("before").is_some() {
                    (span_field(op, "before")?, true)
                } else if op.get("after").is_some() {
                    (span_field(op, "after")?, false)
                } else {
                    return Err("move_to needs `before` or `after`".into());
                };
                if !move_block_relative(&mut src.items, span, target, before) {
                    return Err(stale_span());
                }
                tracked.push(("edited", span));
            }
            other => return Err(format!("unknown op `{other}`")),
        }
    }

    let new_text = wcl_format::to_source(&src);
    // Resolve each marker to its structural index path in the mutated AST,
    // then re-parse the printed text — identical item structure, fresh spans.
    let paths: Vec<(&'static str, Option<Vec<usize>>)> = tracked
        .iter()
        .map(|(role, mark)| (*role, index_path_by_span(&src.items, *mark)))
        .collect();
    crate::edit::commit(&doc_entry, vec![(file_abs.clone(), new_text.clone())])?;
    // The disk changed under every built preview: bump each session's
    // generation so the lazy per-page GET rebuild stops serving pre-commit
    // HTML as fresh (the in-place commit path never POSTs /api/preview).
    for s in state.preview_sessions.lock().unwrap().values_mut() {
        s.generation += 1;
    }
    let fresh = parse_for_edit(&new_text, "<post-format>").map_err(super::err_str)?;
    let spans: Vec<serde_json::Value> = paths
        .into_iter()
        .filter_map(|(role, path)| {
            let span = block_at_path(&fresh.items, &path?)?.span;
            Some(serde_json::json!({
                "role": role,
                "span": super::span_json(span),
            }))
        })
        .collect();
    Ok(serde_json::json!({
        "ok": true,
        "file": rel_path(state, &file_abs)?,
        "etag": crate::edit::content_etag(&new_text),
        "file_text": new_text,
        "spans": spans,
        "span_map": span_map_json(&src.items, &fresh.items),
    }))
}

pub(super) fn stale_span() -> String {
    "no block at that span — the file changed; rebuild the preview".to_string()
}

fn find_block(items: &mut [Item], span: Span) -> Result<&mut ast::Block, String> {
    ast_edit::find_block_by_span(items, span).ok_or_else(stale_span)
}

/// The value for `set_label` / `set_field`: `text` (a string literal) or
/// `expr` (parsed WCL — symbols, numbers, lists).
/// A diagram shape's own id — the name its `a -> b` edges use. Either an
/// `id = <ident>` field or, for shapes whose id is an inline label, the first
/// label. `None` when the block has neither (it can carry no edges).
fn shape_id_of(block: &ast::Block) -> Option<String> {
    let field = block.items.iter().find_map(|it| match it {
        Item::Field(f) if f.name == "id" => match &f.expr {
            Expr::Identifier(s, _) | Expr::Utf8(s) | Expr::Ascii(s) => Some(s.clone()),
            _ => None,
        },
        _ => None,
    });
    field.or_else(|| ast_label(block))
}

/// Drop every connection statement naming `id`, anywhere in the tree — the
/// deleted shape's container isn't known here, and an id is diagram-unique.
fn prune_connections(items: &mut Vec<Item>, id: &str) {
    items.retain(|it| !matches!(it, Item::Connection(c) if c.lhs == id || c.rhs == id));
    for item in items.iter_mut() {
        if let Item::Block(b) = item {
            prune_connections(&mut b.items, id);
        }
    }
}

fn value_expr(op: &serde_json::Value) -> Result<Expr, String> {
    if let Some(text) = op.get("text").and_then(serde_json::Value::as_str) {
        return Ok(ast_edit::string_literal_expr(text));
    }
    if let Some(src) = op.get("expr").and_then(serde_json::Value::as_str) {
        return parse_expr(src, "<design value>").map_err(|e| format!("bad value expr: {e}"));
    }
    Err("missing `text` or `expr`".into())
}

/// Parse a WCL fragment that must be exactly one block.
fn parse_fragment(source: &str) -> Result<ast::Block, String> {
    let src = parse_for_edit(source, "<fragment>")
        .map_err(|e| format!("fragment does not parse: {e}"))?;
    let mut blocks: Vec<ast::Block> = src
        .items
        .into_iter()
        .filter_map(|it| match it {
            Item::Block(b) => Some(b),
            _ => None,
        })
        .collect();
    if blocks.len() != 1 {
        return Err("the fragment must be exactly one block".into());
    }
    Ok(blocks.remove(0))
}

pub(super) fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Move the block at `span` so it sits before/after the block at `target`
/// — resolved at the COMMON-ANCESTOR level: the moved item is the dragged
/// block's ancestor where the two index paths diverge, so a title `h1`
/// inside a transparent `edit_field` wrapper moves WITH its wrapper when
/// dropped relative to a sibling section. Span-addressed on both ends, so
/// the client never counts steps across invisible AST siblings (a
/// `project` body splice, blocks hidden in the current view). Fails when
/// either span is stale or one block contains the other.
fn move_block_relative(items: &mut Vec<Item>, span: Span, target: Span, before: bool) -> bool {
    let (Some(pa), Some(pb)) = (
        index_path_by_span(items, span),
        index_path_by_span(items, target),
    ) else {
        return false;
    };
    let k = pa.iter().zip(pb.iter()).take_while(|(a, b)| a == b).count();
    if k == pa.len() || k == pb.len() {
        return false; // one contains the other
    }
    let mut list = items;
    for &i in &pa[..k] {
        match list.get_mut(i) {
            Some(Item::Block(b)) => list = &mut b.items,
            _ => return false,
        }
    }
    let (ai, bi) = (pa[k], pb[k]);
    if ai == bi {
        return false;
    }
    let item = list.remove(ai);
    let mut ti = bi - usize::from(ai < bi);
    if !before {
        ti += 1;
    }
    list.insert(ti, item);
    true
}

/// Old-span → new-span for every block surviving the batch, by parallel
/// structural walk of the mutated AST and its post-format re-parse (the
/// printed text re-parses to the exact same item structure with fresh
/// spans). Inserted subtrees carry sentinel spans and are skipped — their
/// children's fragment-relative spans never existed in the pre-edit file.
/// The client uses the map to patch the live preview's `data-wcl-span`
/// anchors after an in-place commit (no rebuild, no iframe reload).
fn collect_span_map(old: &[Item], new: &[Item], out: &mut Vec<(Span, Span)>) {
    for (o, n) in old.iter().zip(new.iter()) {
        if let (Item::Block(ob), Item::Block(nb)) = (o, n) {
            if ob.span.end == usize::MAX {
                continue; // inserted sentinel subtree
            }
            out.push((ob.span, nb.span));
            collect_span_map(&ob.items, &nb.items, out);
        }
    }
}

/// [`collect_span_map`] with duplicate `from` spans dropped (defensive: a
/// `replace_source` fragment's children carry fragment-relative spans that
/// could collide with a genuine pre-edit span), as the response JSON.
fn span_map_json(old: &[Item], new: &[Item]) -> Vec<serde_json::Value> {
    let mut pairs: Vec<(Span, Span)> = Vec::new();
    collect_span_map(old, new, &mut pairs);
    let mut counts: std::collections::HashMap<(usize, usize), u32> =
        std::collections::HashMap::new();
    for (from, _) in &pairs {
        *counts.entry((from.start, from.end)).or_default() += 1;
    }
    pairs
        .into_iter()
        .filter(|(from, _)| counts[&(from.start, from.end)] == 1)
        .map(|(from, to)| {
            serde_json::json!({
                "from": super::span_json(from),
                "to": super::span_json(to),
            })
        })
        .collect()
}

/// The item-index path of the block whose span matches `span`, walking the
/// mutated AST. Paths survive printing: the re-parsed output has the exact
/// same item structure with fresh spans.
fn index_path_by_span(items: &[Item], span: Span) -> Option<Vec<usize>> {
    for (i, item) in items.iter().enumerate() {
        if let Item::Block(b) = item {
            if b.span == span {
                return Some(vec![i]);
            }
            if let Some(mut rest) = index_path_by_span(&b.items, span) {
                let mut path = vec![i];
                path.append(&mut rest);
                return Some(path);
            }
        }
    }
    None
}

fn block_at_path<'a>(items: &'a [Item], path: &[usize]) -> Option<&'a ast::Block> {
    let (&head, rest) = path.split_first()?;
    match items.get(head)? {
        Item::Block(b) if rest.is_empty() => Some(b),
        Item::Block(b) => block_at_path(&b.items, rest),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// `POST /api/unit/field` — set one field on a located data object
// ---------------------------------------------------------------------------

/// Body: `{ entry, page_file?, kind, target?, field, value }` — the write
/// half of the `edit_field` inline bindings: resolve the object like
/// `/api/object/locate`, set the field to a string literal, commit.
pub(super) async fn handle_unit_field(
    State(state): State<Arc<EditorState>>,
    body: String,
) -> Response {
    let v = match parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    let state2 = Arc::clone(&state);
    run_blocking(move || unit_field(&state2, &v)).await
}

fn unit_field(state: &EditorState, v: &serde_json::Value) -> Result<serde_json::Value, String> {
    let doc_entry = super::resolve_doc_entry_from(state, v)?;
    let kind = crate::edit::str_field(v, "kind")?;
    let target = v
        .get("target")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty());
    let field = crate::edit::str_field(v, "field")?;
    let value = crate::edit::str_field(v, "value")?;

    let (file, span) = crate::edit::locate_object(&doc_entry, kind, target, HashMap::new())?;
    let text = crate::edit::read(&file)?;
    let mut src = parse_for_edit(&text, file.display().to_string()).map_err(super::err_str)?;
    let block = find_block(&mut src.items, span)?;
    ast_edit::set_or_insert_field(block, field, ast_edit::string_literal_expr(value));
    let new_text = wcl_format::to_source(&src);
    let path = index_path_by_span(&src.items, span);
    crate::edit::commit(&doc_entry, vec![(file.clone(), new_text.clone())])?;
    let fresh = parse_for_edit(&new_text, "<post-format>").map_err(super::err_str)?;
    let spans: Vec<serde_json::Value> = path
        .and_then(|p| block_at_path(&fresh.items, &p))
        .map(|b| {
            vec![serde_json::json!({
                "role": "edited",
                "span": super::span_json(b.span),
            })]
        })
        .unwrap_or_default();
    Ok(serde_json::json!({
        "ok": true,
        "file": rel_path(state, &file)?,
        "etag": crate::edit::content_etag(&new_text),
        "file_text": new_text,
        "spans": spans,
    }))
}

// ---------------------------------------------------------------------------
// `POST /api/unit/create` — a new data-object instance with file placement
// ---------------------------------------------------------------------------

/// Body: `{ entry, page_file?, unit: { kind, id, fields? }, pin?: { index_id } }`.
///
/// File placement follows the document's own conventions: when existing
/// instances of the kind live one-per-file in a single directory (the wskill
/// `data/<kind>s/` layout), the unit gets its own `<id>.wcl` there — plus an
/// `ensure_import` into the sibling `main.wcl` aggregator when one exists.
/// Otherwise it's appended to the file holding the most instances of the
/// kind (multi-block layout), falling back to the entry document itself.
/// `pin` appends the id to the named `index` block's `related` list. All
/// changes land in one [`crate::edit::commit`] (rollback covers them all).
pub(super) async fn handle_unit_create(
    State(state): State<Arc<EditorState>>,
    body: String,
) -> Response {
    let v = match parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    let state2 = Arc::clone(&state);
    run_blocking(move || unit_create(&state2, &v)).await
}

fn unit_create(state: &EditorState, v: &serde_json::Value) -> Result<serde_json::Value, String> {
    let doc_entry = super::resolve_doc_entry_from(state, v)?;
    let unit = v.get("unit").ok_or("missing `unit`")?;
    let kind = crate::edit::str_field(unit, "kind")?;
    let id = crate::edit::str_field(unit, "id")?;
    if !is_identifier(id) {
        return Err(format!(
            "`{id}` is not a valid id (letters, digits, `_`, not starting with a digit)"
        ));
    }
    let doc = wcl_wdoc::open_doc_for_edit(&doc_entry).map_err(super::err_str)?;
    if crate::edit::locate_object(&doc_entry, kind, Some(id), HashMap::new()).is_ok() {
        return Err(format!("a `{kind}` with id `{id}` already exists"));
    }

    // The inline-label expression: an identifier when the schema's
    // `@inline(0)` field is identifier-typed (the wskill unit convention),
    // else a string literal.
    let schema = doc.block_schema(kind);
    let ident_label = schema
        .as_ref()
        .and_then(|s| {
            s.effective_fields()
                .into_iter()
                .find(|f| f.inline_slot() == Some(0))
        })
        .map(|f| f.type_ref().to_string() == "identifier")
        .unwrap_or(false);
    let label = if ident_label {
        Expr::Identifier(id.to_string(), Span::new(0, 0))
    } else {
        ast_edit::string_literal_expr(id)
    };

    let mut fields: Vec<(String, Expr)> = Vec::new();
    if let Some(map) = unit.get("fields").and_then(serde_json::Value::as_object) {
        for (name, val) in map {
            fields.push((name.clone(), json_to_expr(val)?));
        }
    }
    let mut block = ast_edit::build_block(kind, &[], vec![label], fields);
    // An empty body child gives the canvas an insertion target right away.
    let has_body = schema
        .as_ref()
        .map(|s| {
            s.effective_fields()
                .into_iter()
                .any(|f| f.child_block_kind().as_deref() == Some("body"))
        })
        .unwrap_or(false);
    if has_body {
        block.items.push(Item::Block(ast_edit::build_block(
            "body",
            &[],
            vec![],
            vec![],
        )));
    }

    let mut changes: Vec<(PathBuf, String)> = Vec::new();
    // An explicit target file (the Data mode's `@wdoc.editable` hint) wins
    // over convention-derived placement. A brand-new target file also gets
    // imported into the entry so its instances gather.
    let placement = match unit.get("file").and_then(serde_json::Value::as_str) {
        Some(rel) if !rel.is_empty() => {
            let abs = crate::serve::sandboxed_create(&state.root_dir, &state.root_dir.join(rel))
                .ok_or_else(|| format!("file outside the served tree: {rel}"))?;
            if abs.is_file() {
                Placement::Append { file: abs }
            } else {
                Placement::NewTarget { file: abs }
            }
        }
        _ => place_unit(&doc, &doc_entry, kind)?,
    };
    let new_file = match placement {
        Placement::NewFile { dir, aggregator } => {
            let file = dir.join(format!("{id}.wcl"));
            if file.exists() {
                return Err(format!("{} already exists", file.display()));
            }
            let mut src = ast::Source {
                items: Vec::new(),
                trailing_trivia: Vec::new(),
            };
            ast_edit::append_top_level_block(&mut src, block);
            changes.push((file.clone(), wcl_format::to_source(&src)));
            if let Some(agg) = aggregator {
                let text = crate::edit::read(&agg)?;
                let mut asrc =
                    parse_for_edit(&text, agg.display().to_string()).map_err(super::err_str)?;
                ast_edit::ensure_import(&mut asrc, &format!("./{id}.wcl"));
                changes.push((agg, wcl_format::to_source(&asrc)));
            }
            file
        }
        Placement::Append { file } => {
            let text = crate::edit::read(&file)?;
            let mut src =
                parse_for_edit(&text, file.display().to_string()).map_err(super::err_str)?;
            ast_edit::append_top_level_block(&mut src, block);
            changes.push((file.clone(), wcl_format::to_source(&src)));
            file
        }
        Placement::NewTarget { file } => {
            let mut src = ast::Source {
                items: Vec::new(),
                trailing_trivia: Vec::new(),
            };
            ast_edit::append_top_level_block(&mut src, block);
            changes.push((file.clone(), wcl_format::to_source(&src)));
            // Import it from the entry so the new instances gather.
            let entry_dir = doc_entry.parent().unwrap_or(&doc_entry);
            if let Ok(rel) = file.strip_prefix(entry_dir) {
                let text = crate::edit::read(&doc_entry)?;
                let mut esrc = parse_for_edit(&text, doc_entry.display().to_string())
                    .map_err(super::err_str)?;
                if ast_edit::ensure_import(&mut esrc, &rel.to_string_lossy().replace('\\', "/")) {
                    changes.push((doc_entry.clone(), wcl_format::to_source(&esrc)));
                }
            }
            file
        }
    };

    if let Some(pin) = v.get("pin") {
        let index_id = crate::edit::str_field(pin, "index_id")?;
        pin_into_index(&doc, &doc_entry, index_id, id, &mut changes)?;
    }
    drop(doc);

    crate::edit::commit(&doc_entry, changes)?;
    Ok(serde_json::json!({
        "ok": true,
        "file": rel_path(state, &new_file)?,
        "id": id,
    }))
}

enum Placement {
    NewFile {
        dir: PathBuf,
        aggregator: Option<PathBuf>,
    },
    Append {
        file: PathBuf,
    },
    /// An explicitly named file that doesn't exist yet: create it and
    /// import it from the owning entry document.
    NewTarget {
        file: PathBuf,
    },
}

/// Where a new instance of `kind` belongs, derived from where the existing
/// instances live (see [`handle_unit_create`]).
fn place_unit(doc: &Document, doc_entry: &Path, kind: &str) -> Result<Placement, String> {
    let mut per_file: Vec<(PathBuf, usize)> = Vec::new();
    for (path, block) in doc.blocks_with_source() {
        if block.kind() != kind {
            continue;
        }
        let file = path
            .map(Path::to_path_buf)
            .unwrap_or_else(|| doc_entry.to_path_buf());
        match per_file.iter_mut().find(|(p, _)| *p == file) {
            Some((_, n)) => *n += 1,
            None => per_file.push((file, 1)),
        }
    }
    if per_file.is_empty() {
        return Ok(Placement::Append {
            file: doc_entry.to_path_buf(),
        });
    }
    // One-per-file layout: every instance alone in its file, all in one
    // directory → a fresh `<id>.wcl` beside them.
    let one_per_file = per_file.iter().all(|(_, n)| *n == 1);
    let dirs: Vec<&Path> = per_file.iter().filter_map(|(p, _)| p.parent()).collect();
    if one_per_file
        && dirs.windows(2).all(|w| w[0] == w[1])
        && let Some(dir) = dirs.first()
    {
        let aggregator = dir.join("main.wcl");
        return Ok(Placement::NewFile {
            dir: dir.to_path_buf(),
            aggregator: aggregator.is_file().then_some(aggregator),
        });
    }
    let (file, _) = per_file
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .expect("non-empty");
    Ok(Placement::Append { file })
}

/// Append `id` to the `related` list of the `index` block labelled
/// `index_id`, layering on top of any pending change to the same file.
fn pin_into_index(
    doc: &Document,
    doc_entry: &Path,
    index_id: &str,
    id: &str,
    changes: &mut Vec<(PathBuf, String)>,
) -> Result<(), String> {
    let (ifile, _) = doc
        .blocks_with_source()
        .find(|(_, b)| b.kind() == "index" && first_label(b).as_deref() == Some(index_id))
        .map(|(p, b)| {
            (
                p.map(Path::to_path_buf)
                    .unwrap_or_else(|| doc_entry.to_path_buf()),
                b.span(),
            )
        })
        .ok_or_else(|| format!("no `index` with id `{index_id}`"))?;
    // Base text: a pending change to the same file, else disk. Located by
    // kind + label (not span) because pending edits shift spans.
    let base = match changes.iter().find(|(p, _)| *p == ifile) {
        Some((_, text)) => text.clone(),
        None => crate::edit::read(&ifile)?,
    };
    let mut src = parse_for_edit(&base, ifile.display().to_string()).map_err(super::err_str)?;
    let block = find_block_by_kind_label(&mut src.items, "index", index_id)
        .ok_or_else(|| format!("could not relocate index `{index_id}`"))?;
    let related = block.items.iter_mut().find_map(|it| match it {
        Item::Field(f) if f.name == "related" => Some(f),
        _ => None,
    });
    let new_ident = Expr::Identifier(id.to_string(), Span::new(0, 0));
    match related {
        Some(f) => match &mut f.expr {
            Expr::ListLit {
                elements,
                elem_trivia,
                ..
            } => {
                elements.push(new_ident);
                elem_trivia.push(Default::default());
            }
            _ => {
                return Err(format!(
                    "index `{index_id}`'s related list is computed — edit its source instead"
                ));
            }
        },
        None => ast_edit::set_or_insert_field(
            block,
            "related",
            Expr::ListLit {
                elements: vec![new_ident],
                elem_trivia: vec![Default::default()],
                trailing_trivia: Vec::new(),
                span: Span::new(0, 0),
            },
        ),
    }
    let text = wcl_format::to_source(&src);
    match changes.iter_mut().find(|(p, _)| *p == ifile) {
        Some((_, pending)) => *pending = text,
        None => changes.push((ifile, text)),
    }
    Ok(())
}

/// The first inline label of an AST block when it's a plain identifier or
/// string literal.
pub(super) fn ast_label(b: &ast::Block) -> Option<String> {
    match b.labels.first()? {
        Expr::Utf8(s) | Expr::Ascii(s) => Some(s.clone()),
        Expr::Identifier(s, _) => Some(s.clone()),
        _ => None,
    }
}

pub(super) fn find_block_by_kind_label<'a>(
    items: &'a mut [Item],
    kind: &str,
    label: &str,
) -> Option<&'a mut ast::Block> {
    for item in items {
        if let Item::Block(b) = item {
            if b.kind == kind && ast_label(b).as_deref() == Some(label) {
                return Some(b);
            }
            if let Some(found) = find_block_by_kind_label(&mut b.items, kind, label) {
                return Some(found);
            }
        }
    }
    None
}

/// The first label of a document-view block as a plain string.
pub(super) fn first_label(b: &wcl_lang::Block<'_>) -> Option<String> {
    b.labels()
        .ok()
        .and_then(|ls| ls.first().map(value_string))
        .filter(|s| !s.is_empty())
}

pub(super) fn value_string(v: &Value) -> String {
    match v {
        Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) => s.clone(),
        Value::Symbol(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::I64(n) => n.to_string(),
        Value::U64(n) => n.to_string(),
        Value::F64(n) => n.to_string(),
        other => format!("{other:?}"),
    }
}

/// A JSON value from the create form → a WCL expression. Strings become
/// string literals; `{ "sym": "name" }` a symbol; `{ "ident": "name" }` an
/// identifier; `{ "expr": "…" }` parsed WCL; numbers, bools and arrays map
/// structurally.
fn json_to_expr(v: &serde_json::Value) -> Result<Expr, String> {
    use serde_json::Value as J;
    match v {
        J::String(s) => Ok(ast_edit::string_literal_expr(s)),
        J::Bool(b) => Ok(Expr::Bool(*b)),
        J::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Expr::I64(i))
            } else {
                Ok(Expr::F64(n.as_f64().ok_or("bad number")?))
            }
        }
        J::Array(items) => Ok(Expr::ListLit {
            elements: items.iter().map(json_to_expr).collect::<Result<_, _>>()?,
            elem_trivia: items.iter().map(|_| Default::default()).collect(),
            trailing_trivia: Vec::new(),
            span: Span::new(0, 0),
        }),
        J::Object(map) => {
            if let Some(s) = map.get("sym").and_then(J::as_str) {
                if !is_identifier(s) {
                    return Err(format!("`{s}` is not a valid symbol"));
                }
                Ok(Expr::Symbol(s.to_string()))
            } else if let Some(s) = map.get("ident").and_then(J::as_str) {
                if !is_identifier(s) {
                    return Err(format!("`{s}` is not a valid identifier"));
                }
                Ok(Expr::Identifier(s.to_string(), Span::new(0, 0)))
            } else if let Some(s) = map.get("expr").and_then(J::as_str) {
                parse_expr(s, "<design value>").map_err(|e| format!("bad value expr: {e}"))
            } else {
                Err("unsupported field value shape".into())
            }
        }
        J::Null => Err("null field value".into()),
    }
}

// ---------------------------------------------------------------------------
// `GET /api/palette` — what the add-block UI can insert here
// ---------------------------------------------------------------------------

/// Query: `entry`, `site?`, `page_file?` → `{ site_type, wskill, unit_kinds,
/// body_kinds, components }`. Unit kinds come from the merged `@document`
/// gathers with schema introspection (field metadata drives the generated
/// create form); body kinds are the curated wdoc content blocks with
/// canonical insertion snippets; components are the `wdoc_component`
/// declarations authored inside the served tree, with their slots.
pub(super) async fn handle_palette(State(state): State<Arc<EditorState>>, uri: Uri) -> Response {
    let entry = query_param(&uri, "entry");
    let site = query_param(&uri, "site");
    let page_file = query_param(&uri, "page_file");
    let state2 = Arc::clone(&state);
    run_blocking(move || {
        let entry = entry.ok_or("missing entry")?;
        palette(&state2, &entry, site.as_deref(), page_file.as_deref())
    })
    .await
}

/// Wskill plumbing kinds that never belong in the add-a-unit palette.
const UNIT_KIND_DENYLIST: &[&str] = &[
    "topic",
    "skill",
    "artifact",
    "source",
    "question",
    "wskill_ref",
];

/// The curated body-block palette: `(kind, label, canonical snippet)`.
/// Static because most of these render via Rust fundamentals — there is no
/// WCL schema rich enough to introspect an insertion template from.
const BODY_KINDS: &[(&str, &str, &str)] = &[
    ("p", "Paragraph", "p \"New paragraph\""),
    ("h2", "Heading", "h2 \"New heading\""),
    ("h3", "Subheading", "h3 \"New subheading\""),
    (
        "code",
        "Code block",
        "code \"text\" {\n  source = <<'SRC'\n\nSRC\n}",
    ),
    (
        "callout",
        "Callout",
        "callout \"Note\" {\n  body = \"Callout text\"\n}",
    ),
    ("list", "List", "list {\n  li \"First item\"\n}"),
    (
        "table",
        "Table",
        "table {\n  rows:\n    | \"Column\" | \"Column\" |\n    | \"\" | \"\" |\n}",
    ),
    ("image", "Image", "image \"\" {\n  alt = \"\"\n}"),
];

fn palette(
    state: &EditorState,
    entry: &str,
    site: Option<&str>,
    page_file: Option<&str>,
) -> Result<serde_json::Value, String> {
    let doc_entry = super::resolve_doc_entry(state, entry, page_file)?;
    let doc = wcl_wdoc::open_doc_for_edit(&doc_entry).map_err(super::err_str)?;

    let body_kinds: Vec<serde_json::Value> = BODY_KINDS
        .iter()
        .map(|(kind, label, snippet)| {
            serde_json::json!({ "kind": kind, "label": label, "template_source": snippet })
        })
        .collect();

    Ok(serde_json::json!({
        "ok": true,
        "site_type": site_kind(&doc, site),
        "wskill": is_wskill(&doc),
        "unit_kinds": unit_kinds(&doc),
        "diagram_kinds": diagram_kinds(&doc),
        "body_kinds": body_kinds,
        "components": components(state, &doc),
    }))
}

/// Whether the document carries the wskill data model (a gathered `topic`).
pub(super) fn is_wskill(doc: &Document) -> bool {
    doc.blocks().any(|b| b.kind() == "topic")
}

/// `book` / `website` / `presentation`, from the selected `site` block's
/// nav-declaring child (`toc` / `menu` / `deck`).
pub(super) fn site_kind(doc: &Document, site: Option<&str>) -> &'static str {
    let block = doc.blocks().find(|b| {
        b.kind() == "site"
            && match site {
                Some(name) => first_label(b).as_deref() == Some(name),
                None => true,
            }
    });
    let Some(site) = block else { return "book" };
    let child_kinds: Vec<String> = site.blocks().map(|b| b.kind().to_string()).collect();
    if child_kinds.iter().any(|k| k == "deck") {
        "presentation"
    } else if child_kinds.iter().any(|k| k == "menu") {
        "website"
    } else {
        "book"
    }
}

/// The addable data-object kinds: every `@children`-gathered kind of the
/// document's merged `@document` schemas, minus wdoc infrastructure (by
/// declaring namespace) and wskill plumbing (by name). Field metadata feeds
/// the generated create form.
pub(super) fn unit_kinds(doc: &Document) -> Vec<serde_json::Value> {
    let mut seen: Vec<String> = Vec::new();
    let mut out: Vec<serde_json::Value> = Vec::new();
    for decl in doc.type_decls() {
        if !decl.decorators().any(|d| d.name() == "document") {
            continue;
        }
        for field in decl.effective_fields() {
            let Some(kind) = field.children_block_kind() else {
                continue;
            };
            if seen.contains(&kind) || UNIT_KIND_DENYLIST.contains(&kind.as_str()) {
                continue;
            }
            seen.push(kind.clone());
            let Some(schema) = doc.block_schema(&kind) else {
                continue;
            };
            // wdoc's own document gathers (pages, sites, components, …) are
            // not data units.
            if schema.full_name().starts_with("wdoc.") {
                continue;
            }
            out.push(kind_entry(doc, &kind, &schema));
        }
    }
    out
}

/// The first positional argument of a decorator, as a string.
pub(super) fn dec_first_string(d: &wcl_lang::Decorator<'_>) -> Option<String> {
    d.positional().ok()?.first().map(|v| match v {
        Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) => s.clone(),
        other => format!("{other:?}"),
    })
}

/// The addable diagram shape kinds: every `@block("kind")` type descending
/// from `wdoc.SvgBlock`, with the same field metadata as `unit_kinds` (the
/// shape properties form is generated from it). The client curates which
/// kinds surface in the add-shape palette; the full list is served so any
/// selected shape — including user-declared ones — gets a schema-driven form.
pub(super) fn diagram_kinds(doc: &Document) -> Vec<serde_json::Value> {
    let mut seen: Vec<String> = Vec::new();
    let mut out: Vec<serde_json::Value> = Vec::new();
    for decl in doc.type_decls() {
        let Some(kind) = decl
            .decorators()
            .find(|d| d.name() == "block")
            .and_then(|d| dec_first_string(&d))
        else {
            continue;
        };
        if seen.contains(&kind) || !decl.is_descendant_of("wdoc.SvgBlock") {
            continue;
        }
        seen.push(kind.clone());
        out.push(kind_entry(doc, &kind, &decl));
    }
    out.sort_by(|a, b| a["kind"].as_str().cmp(&b["kind"].as_str()));
    out
}

pub(super) fn kind_entry(
    doc: &Document,
    kind: &str,
    schema: &wcl_lang::TypeDecl<'_>,
) -> serde_json::Value {
    let mut fields: Vec<serde_json::Value> = Vec::new();
    let mut has_body = false;
    for f in schema.effective_fields() {
        if f.child_block_kind().as_deref() == Some("body") {
            has_body = true;
        }
        // Child blocks / connections aren't form fields.
        if f.child_kind_or_union().is_some()
            || f.children_kind_or_union().is_some()
            || f.connection_schema().is_some()
        {
            continue;
        }
        let ty = f.type_ref();
        // Function-valued fields (an SvgBlock's `lower`, computed hooks)
        // aren't form-editable properties.
        if ty.to_string().starts_with("fn") {
            continue;
        }
        let symbols: Option<Vec<String>> = match doc.resolve(ty) {
            ResolvedType::SymbolSet(ss) => {
                Some(ss.symbols().map(|s| s.name().to_string()).collect())
            }
            _ => None,
        };
        fields.push(serde_json::json!({
            "name": f.name(),
            "type": ty.to_string(),
            "optional": f.optional(),
            "inline_slot": f.inline_slot(),
            "symbols": symbols,
            "default": f.default_value().as_ref().map(value_string),
            "doc": f.doc_comment(),
        }));
    }
    serde_json::json!({
        "kind": kind,
        "doc": schema.doc_comment(),
        "fields": fields,
        "has_body": has_body,
    })
}

/// `wdoc_component` declarations authored inside the served tree (stdlib
/// components are excluded — their sources live outside the root), with the
/// slot list that drives the property form.
fn components(state: &EditorState, doc: &Document) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for (path, block) in doc.blocks_with_source() {
        if block.kind() != "wdoc_component" {
            continue;
        }
        if let Some(p) = path
            && !p.starts_with(&state.root_dir)
        {
            continue;
        }
        let Some(name) = first_label(&block) else {
            continue;
        };
        let slots: Vec<serde_json::Value> = block
            .blocks()
            .filter(|b| b.kind() == "wdoc_slot")
            .map(|slot| {
                let default = slot
                    .field("default")
                    .and_then(|f| f.value().ok().cloned())
                    .as_ref()
                    .map(value_string);
                let required = default.is_none();
                serde_json::json!({
                    "name": first_label(&slot),
                    "default": default,
                    "required": required,
                })
            })
            .collect();
        let file = path.and_then(|p| rel_path(state, p).ok());
        out.push(serde_json::json!({ "name": name, "file": file, "slots": slots }));
    }
    out
}
