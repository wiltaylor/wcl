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

use super::preview::Sessions;
use super::{EditorState, Workspace, run_blocking};
use crate::serve::{json_error, parse_json_body, query_param};

// ---------------------------------------------------------------------------
// Request context
// ---------------------------------------------------------------------------

/// Sandbox-check a repo-relative file from the request body.
fn file_field(ws: &Workspace, v: &serde_json::Value) -> Result<PathBuf, String> {
    ws.abs(crate::edit::str_field(v, "file")?)
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
    run_blocking(move || block_source(&state2.ws, &v)).await
}

fn block_source(ws: &Workspace, v: &serde_json::Value) -> Result<serde_json::Value, String> {
    let file_abs = file_field(ws, v)?;
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
    run_blocking(move || block_ops(&state2.ws, &state2.sessions, &v)).await
}

pub(super) fn block_ops(
    ws: &Workspace,
    previews: &Sessions,
    v: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let doc_entry = ws.doc_entry_from(v)?;
    let file_abs = file_field(ws, v)?;
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
                    ast_edit::remove_connections_touching(&mut src.items, &id);
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
    previews.invalidate();
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
        "file": ws.rel(&file_abs)?,
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
    run_blocking(move || unit_field(&state2.ws, &state2.sessions, &v)).await
}

fn unit_field(
    ws: &Workspace,
    previews: &Sessions,
    v: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let doc_entry = ws.doc_entry_from(v)?;
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
    previews.invalidate();
    Ok(serde_json::json!({
        "ok": true,
        "file": ws.rel(&file)?,
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
    run_blocking(move || unit_create(&state2.ws, &state2.sessions, &v)).await
}

pub(super) fn unit_create(
    ws: &Workspace,
    previews: &Sessions,
    v: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let doc_entry = ws.doc_entry_from(v)?;
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
    // else a string literal. `type_name` (the schema's fully-qualified name,
    // served with every kind entry) disambiguates kind names shared across
    // namespaces — a WAD `container` must not be schema'd by wdoc's
    // diagram-grouping shape of the same name.
    let schema = unit
        .get("type_name")
        .and_then(serde_json::Value::as_str)
        .and_then(|full| doc.type_decls().find(|d| d.full_name() == full))
        .or_else(|| doc.block_schema(kind));
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
            let abs = ws.abs_new(rel)?;
            if abs.is_file() {
                Placement::Append { file: abs }
            } else {
                Placement::NewTarget { file: abs }
            }
        }
        _ => place_unit(&doc, &doc_entry, kind)?,
    };
    let new_file = write_new_block(placement, id, block, &doc_entry, &mut changes)?;

    if let Some(pin) = v.get("pin") {
        let index_id = crate::edit::str_field(pin, "index_id")?;
        pin_into_index(&doc, &doc_entry, index_id, id, &mut changes)?;
    }
    drop(doc);

    crate::edit::commit(&doc_entry, changes)?;
    previews.invalidate();
    Ok(serde_json::json!({
        "ok": true,
        "file": ws.rel(&new_file)?,
        "id": id,
    }))
}

/// Realise a [`Placement`] for a freshly built top-level block: stage the
/// file writes (a new `<id>.wcl` plus its aggregator import, an append to an
/// existing file, or a named new target imported from the entry) into
/// `changes` and answer the file the block landed in. Shared by
/// [`handle_unit_create`] and the nav model's `create_index`.
pub(super) fn write_new_block(
    placement: Placement,
    id: &str,
    block: ast::Block,
    doc_entry: &Path,
    changes: &mut Vec<(PathBuf, String)>,
) -> Result<PathBuf, String> {
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
            let entry_dir = doc_entry.parent().unwrap_or(doc_entry);
            if let Ok(rel) = file.strip_prefix(entry_dir) {
                let text = crate::edit::read(doc_entry)?;
                let mut esrc = parse_for_edit(&text, doc_entry.display().to_string())
                    .map_err(super::err_str)?;
                if ast_edit::ensure_import(&mut esrc, &rel.to_string_lossy().replace('\\', "/")) {
                    changes.push((doc_entry.to_path_buf(), wcl_format::to_source(&esrc)));
                }
            }
            file
        }
    };
    Ok(new_file)
}

pub(super) enum Placement {
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
pub(super) fn place_unit(
    doc: &Document,
    doc_entry: &Path,
    kind: &str,
) -> Result<Placement, String> {
    let mut per_file: Vec<(PathBuf, usize)> = Vec::new();
    for (path, block) in doc.blocks_with_source() {
        if block.kind() != kind {
            continue;
        }
        let file = path
            .map(Path::to_path_buf)
            .unwrap_or_else(|| doc_entry.to_path_buf());
        if is_generated(&file) {
            continue;
        }
        match per_file.iter_mut().find(|(p, _)| *p == file) {
            Some((_, n)) => *n += 1,
            None => per_file.push((file, 1)),
        }
    }
    if per_file.is_empty() {
        // No instances to learn from. The entry document is the last resort,
        // NOT the first: a projection entry (a WAD's book template, a
        // wskill's) is a different namespace from the data it renders, and a
        // block written there wouldn't even resolve to this schema. Look for
        // a data file of a neighbouring kind instead — the kinds this one
        // nests into, then the kinds that nest into it, then anything else
        // declared in the same schema namespace.
        return Ok(Placement::Append {
            file: kin_file(doc, kind).unwrap_or_else(|| doc_entry.to_path_buf()),
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

/// Is this file written by a generator? Extractor output carries a
/// `GENERATED` banner and is overwritten wholesale on the next run — an
/// object created there would be silently lost (and, where a CI gate checks
/// the tree is fresh, would fail the build). Placement skips such files
/// entirely; objects that are already in them stay editable in place.
fn is_generated(file: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(file) else {
        return false;
    };
    // The banner is a leading comment, so only the head of the file matters.
    text.lines()
        .take(5)
        .take_while(|l| {
            let t = l.trim_start();
            t.is_empty() || t.starts_with("//") || t.starts_with('#')
        })
        .any(|l| l.contains("GENERATED"))
}

/// The data file a brand-new instance of `kind` should join when the kind
/// has no instances of its own: the file holding the most instances of a
/// neighbouring kind, tried in order — the kinds it nests into, the kinds
/// that nest into it, then any other kind declared in the same schema
/// namespace. `None` when the document holds no such data at all.
fn kin_file(doc: &Document, kind: &str) -> Option<PathBuf> {
    let links = kind_links(doc);
    let me = links.iter().find(|k| k.kind == kind)?;
    let ns = me.schema.namespace().to_vec();
    let parents: Vec<&str> = me.parents.iter().map(|(_, k)| k.as_str()).collect();
    let children: Vec<&str> = links
        .iter()
        .filter(|k| k.parents.iter().any(|(_, p)| p == kind))
        .map(|k| k.kind.as_str())
        .collect();
    let same_ns: Vec<&str> = links
        .iter()
        .filter(|k| k.kind != kind && k.schema.namespace() == ns)
        .map(|k| k.kind.as_str())
        .collect();

    for tier in [&parents, &children, &same_ns] {
        let mut per_file: Vec<(PathBuf, usize)> = Vec::new();
        for (path, block) in doc.blocks_with_source() {
            if !tier.contains(&block.kind()) {
                continue;
            }
            let Some(file) = path.map(Path::to_path_buf) else {
                continue;
            };
            if is_generated(&file) {
                continue;
            }
            match per_file.iter_mut().find(|(p, _)| *p == file) {
                Some((_, n)) => *n += 1,
                None => per_file.push((file, 1)),
            }
        }
        // Ties go to the first file in document order, so placement is
        // deterministic rather than dependent on iteration order.
        if let Some((file, _)) = per_file.into_iter().max_by_key(|(_, n)| *n) {
            return Some(file);
        }
    }
    None
}

/// Append `id` to the `related` list of the `index` block labelled
/// `index_id`, layering on top of any pending change to the same file.
/// Does this block's subtree declare an `index` with the given id?
fn subtree_has_index(b: &wcl_lang::Block<'_>, id: &str) -> bool {
    (b.kind() == "index" && first_label(b).as_deref() == Some(id))
        || b.blocks().any(|c| subtree_has_index(&c, id))
}

/// The file declaring the `index` with `id`. Sub-indexes nest inside their
/// parent block, so the search recurses (the block itself is then relocated
/// by the equally recursive [`find_block_by_kind_label`]). Index ids are
/// assumed document-unique; first match wins. Shared by `unit_create`'s pin
/// and the nav model's id-addressed index ops — two callers that each used
/// to walk the document themselves, one of them only at the top level.
pub(super) fn index_file(
    doc: &Document,
    doc_entry: &Path,
    index_id: &str,
) -> Result<PathBuf, String> {
    doc.blocks_with_source()
        .find(|(_, b)| subtree_has_index(b, index_id))
        .map(|(p, _)| {
            p.map(Path::to_path_buf)
                .unwrap_or_else(|| doc_entry.to_path_buf())
        })
        .ok_or_else(|| format!("no `index` with id `{index_id}`"))
}

fn pin_into_index(
    doc: &Document,
    doc_entry: &Path,
    index_id: &str,
    id: &str,
    changes: &mut Vec<(PathBuf, String)>,
) -> Result<(), String> {
    let ifile = index_file(doc, doc_entry, index_id)?;
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
        palette(&state2.ws, &entry, site.as_deref(), page_file.as_deref())
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
    ws: &Workspace,
    entry: &str,
    site: Option<&str>,
    page_file: Option<&str>,
) -> Result<serde_json::Value, String> {
    let doc_entry = ws.doc_entry(entry, page_file)?;
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
        "wad": is_wad(&doc),
        "unit_kinds": unit_kinds(&doc),
        "diagram_kinds": diagram_kinds(&doc),
        "body_kinds": body_kinds,
        "components": components(ws, &doc),
    }))
}

/// Whether the document carries the wskill data model (a gathered `topic`).
pub(super) fn is_wskill(doc: &Document) -> bool {
    doc.blocks().any(|b| b.kind() == "topic")
}

/// Whether the document carries the WAD data model (its one `wad` root
/// metadata block) — the flag that opens the Systems view.
pub(super) fn is_wad(doc: &Document) -> bool {
    doc.blocks().any(|b| b.kind() == "wad")
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

/// Every `@children`-gathered block kind of the document's merged
/// `@document` schemas, minus wdoc's own infrastructure gathers (pages,
/// sites, components, …, excluded by declaring namespace), as
/// `(kind, schema)` in declaration order. The data-object surface every
/// schema-driven view is built from.
pub(super) fn gathered_kinds<'a>(doc: &'a Document) -> Vec<(String, wcl_lang::TypeDecl<'a>)> {
    let mut seen: Vec<String> = Vec::new();
    let mut out: Vec<(String, wcl_lang::TypeDecl<'a>)> = Vec::new();
    for decl in doc.type_decls() {
        if !decl.decorators().any(|d| d.name() == "document") {
            continue;
        }
        for field in decl.effective_fields() {
            let Some(kind) = field.children_block_kind() else {
                continue;
            };
            if seen.contains(&kind) {
                continue;
            }
            seen.push(kind.clone());
            let Some(schema) = gather_elem_decl(&field).or_else(|| doc.block_schema(&kind)) else {
                continue;
            };
            if schema.full_name().starts_with("wdoc.") {
                continue;
            }
            out.push((kind, schema));
        }
    }
    out
}

/// The type a gather field's element names, resolved through the field's own
/// declared type. Namespace-correct where a bare [`Document::block_schema`]
/// name lookup is not: a WAD's `wcl.wad.Container` and wdoc's diagram
/// `container` shape share a *kind* name, and the name lookup answers
/// whichever happens to be declared first.
pub(super) fn gather_elem_decl<'a>(
    field: &wcl_lang::TypeField<'a>,
) -> Option<wcl_lang::TypeDecl<'a>> {
    fn named(t: ResolvedType<'_>) -> Option<wcl_lang::TypeDecl<'_>> {
        match t {
            ResolvedType::Named(d) => Some(d),
            ResolvedType::List(inner) | ResolvedType::Reference(inner) => named(*inner),
            _ => None,
        }
    }
    named(field.resolved_type())
}

/// One gathered kind's derived structure — how instances of it nest, what
/// they reference, and whether the kind is an edge rather than a node.
///
/// The rules (shared by the Systems view and the create path's file
/// placement, so both read the same model):
///
/// - a **parent link** is a scalar `identifier` field whose NAME is another
///   gathered kind's name (`component.container`, `system.boundary`), plus
///   `parent` for self-nesting (`infra_node.parent`). Inline id slots name
///   the block itself and never count.
/// - a **reference** is any other `identifier` / `list<identifier>` field.
/// - an **edge kind** carries both a `source` and a `destination`
///   identifier field; its endpoints are wiring, not containment.
pub(super) struct KindInfo<'a> {
    pub kind: String,
    pub schema: wcl_lang::TypeDecl<'a>,
    /// `(field name, parent kind)` in declaration order.
    pub parents: Vec<(String, String)>,
    /// `(field name, is a list)` — cross-references, not containment.
    pub refs: Vec<(String, bool)>,
    /// `(source field, destination field)` when the kind is an edge kind.
    pub edge: Option<(String, String)>,
}

/// A field's declared type with a trailing `?` stripped.
pub(super) fn bare_type(f: &wcl_lang::TypeField<'_>) -> String {
    let ty = f.type_ref().to_string();
    ty.strip_suffix('?').unwrap_or(&ty).to_string()
}

/// Scalar fields only: child blocks, child-block lists and connections are
/// structure, not properties.
pub(super) fn is_scalar(f: &wcl_lang::TypeField<'_>) -> bool {
    f.child_kind_or_union().is_none()
        && f.children_kind_or_union().is_none()
        && f.connection_schema().is_none()
}

/// [`KindInfo`] for every gathered kind of the document.
pub(super) fn kind_links<'a>(doc: &'a Document) -> Vec<KindInfo<'a>> {
    let gathered = gathered_kinds(doc);
    let names: Vec<String> = gathered.iter().map(|(k, _)| k.clone()).collect();
    gathered
        .into_iter()
        .map(|(kind, schema)| {
            let mut parents: Vec<(String, String)> = Vec::new();
            let mut refs: Vec<(String, bool)> = Vec::new();
            let mut source = None;
            let mut destination = None;
            for f in schema.effective_fields() {
                if !is_scalar(&f) {
                    continue;
                }
                let name = f.name().to_string();
                let ty = bare_type(&f);
                if ty == "identifier" {
                    if name == "source" {
                        source = Some(name.clone());
                    } else if name == "destination" {
                        destination = Some(name.clone());
                    }
                    if f.inline_slot().is_some() {
                        continue;
                    }
                    if name == "parent" {
                        parents.push((name, kind.clone()));
                    } else if names.contains(&name) {
                        parents.push((name.clone(), name));
                    } else {
                        refs.push((name, false));
                    }
                } else if ty == "list<identifier>" {
                    refs.push((name, true));
                }
            }
            let edge = match (source, destination) {
                (Some(s), Some(d)) => Some((s, d)),
                _ => None,
            };
            if let Some((s, d)) = &edge {
                parents.retain(|(f, _)| f != s && f != d);
                refs.retain(|(f, _)| f != s && f != d);
            }
            KindInfo {
                kind,
                schema,
                parents,
                refs,
                edge,
            }
        })
        .collect()
}

/// The addable data-object kinds: [`gathered_kinds`] minus wskill plumbing
/// (by name). Field metadata feeds the generated create form.
pub(super) fn unit_kinds(doc: &Document) -> Vec<serde_json::Value> {
    gathered_kinds(doc)
        .into_iter()
        .filter(|(kind, _)| !UNIT_KIND_DENYLIST.contains(&kind.as_str()))
        .map(|(kind, schema)| kind_entry(&kind, &schema))
        .collect()
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
        out.push(kind_entry(&kind, &decl));
    }
    out.sort_by(|a, b| a["kind"].as_str().cmp(&b["kind"].as_str()));
    out
}

pub(super) fn kind_entry(kind: &str, schema: &wcl_lang::TypeDecl<'_>) -> serde_json::Value {
    let mut fields: Vec<serde_json::Value> = Vec::new();
    let mut has_body = false;
    // Declares a `@children(...)` family — an `insert_child` may nest
    // blocks inside instances of this kind (a wireframe container widget,
    // a diagram grouping). The widget palette keys append-inside vs
    // insert-after off it.
    let mut accepts_children = false;
    for f in schema.effective_fields() {
        if f.child_block_kind().as_deref() == Some("body") {
            has_body = true;
        }
        if f.children_kind_or_union().is_some() {
            accepts_children = true;
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
        // Resolved in the field's OWN namespace — a `wcl.wad` field typed
        // `ContainerKind` must not pick up a same-named set elsewhere.
        let symbols: Option<Vec<String>> = match f.resolved_type() {
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
        "accepts_children": accepts_children,
    })
}

/// `wdoc_component` declarations authored inside the served tree (stdlib
/// components are excluded — their sources live outside the root), with the
/// slot list that drives the property form.
fn components(ws: &Workspace, doc: &Document) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for (path, block) in doc.blocks_with_source() {
        if block.kind() != "wdoc_component" {
            continue;
        }
        if let Some(p) = path
            && !p.starts_with(ws.root_dir())
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
        let file = path.and_then(|p| ws.rel(p).ok());
        out.push(serde_json::json!({ "name": name, "file": file, "slots": slots }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::span_json;
    use crate::editor::testsupport::{
        BODY_DOC, OBJECT_DOC, span_of, workspace_with, write_mini_wskill,
    };

    /// A document whose page holds a diagram of three wired shapes.
    const DIAGRAM_DOC: &str = "import <wdoc.wcl>\n\nsite docs {\n  title = \"D\"\n  root = true\n}\n\npage index {\n  title = \"Hi\"\n\n  diagram {\n    width = 400\n    height = 300\n    rect {\n      id = a\n    }\n    rect {\n      id = b\n    }\n    rect {\n      id = c\n    }\n    a -> b\n  }\n}\n";

    /// Nested fixture for the span-map tests: 7 blocks (site, page, p,
    /// list, li, li, p).
    const NESTED_DOC: &str = "import <wdoc.wcl>\n\nsite docs {\n  title = \"D\"\n  root = true\n}\n\npage index {\n  title = \"Hi\"\n\n  p \"First\"\n\n  list {\n    li \"one\"\n    li \"two\"\n  }\n\n  p \"Last\"\n}\n";

    /// A write path's context: the served tree plus the invalidation handle.
    /// Nothing here needs a preview scratch tree.
    fn previews() -> Sessions {
        Sessions::default()
    }

    fn main_wcl(ws: &Workspace) -> PathBuf {
        ws.root_dir().join("main.wcl")
    }

    fn disk(ws: &Workspace) -> String {
        std::fs::read_to_string(main_wcl(ws)).unwrap()
    }

    /// Wiring shapes together writes `a -> b` connection STATEMENTS — the
    /// language's own relationship syntax — rather than a list field.
    #[test]
    fn connect_ops_write_connection_statements() {
        let (_td, ws) = workspace_with(DIAGRAM_DOC);
        let previews = previews();
        // Every commit reprints the file, so the diagram's span moves — each
        // call re-resolves it, exactly as the client re-anchors on reload.
        let connect = |ops: &dyn Fn(Span) -> serde_json::Value| {
            let text = disk(&ws);
            let diagram = span_of(&text, |b| b.kind == "diagram");
            block_ops(
                &ws,
                &previews,
                &serde_json::json!({
                    "entry": "main.wcl", "file": "main.wcl",
                    "etag": crate::edit::content_etag(&text),
                    "ops": ops(diagram),
                }),
            )
        };

        // Add one plain edge and one kinded edge.
        connect(&|d| {
            serde_json::json!([
                { "op": "connect_add", "span": span_json(d), "from": "b", "to": "c" },
                { "op": "connect_add", "span": span_json(d), "from": "c", "to": "a", "kind": "flow" },
            ])
        })
        .expect("connect_add");
        let text = disk(&ws);
        assert!(text.contains("b -> c"), "{text}");
        assert!(text.contains("c -> a :flow"), "{text}");

        // Removing one leaves the others alone.
        connect(&|d| {
            serde_json::json!([
                { "op": "connect_remove", "span": span_json(d), "from": "b", "to": "c" },
            ])
        })
        .expect("connect_remove");
        let text = disk(&ws);
        assert!(!text.contains("b -> c"), "{text}");
        assert!(text.contains("c -> a :flow"), "{text}");
        assert!(text.contains("a -> b"), "{text}");
    }

    #[test]
    fn connect_ops_reject_nonsense() {
        let (_td, ws) = workspace_with(DIAGRAM_DOC);
        let text = disk(&ws);
        let etag = crate::edit::content_etag(&text);
        let diagram = span_of(&text, |b| b.kind == "diagram");

        for (op, from, to) in [
            ("connect_add", "a", "a"),    // self-connection
            ("connect_add", "a", "b"),    // already wired
            ("connect_remove", "a", "c"), // no such edge
        ] {
            let r = block_ops(
                &ws,
                &previews(),
                &serde_json::json!({
                    "entry": "main.wcl", "file": "main.wcl", "etag": etag,
                    "ops": [{ "op": op, "span": span_json(diagram), "from": from, "to": to }],
                }),
            );
            assert!(r.is_err(), "{op} {from}->{to} should fail");
        }
    }

    /// Deleting a shape takes its edges with it — the sync failure that let a
    /// removed shape leave dangling `a -> b` statements behind.
    #[test]
    fn deleting_a_shape_removes_its_connections() {
        let (_td, ws) = workspace_with(DIAGRAM_DOC);
        let previews = previews();
        let text = disk(&ws);
        let diagram = span_of(&text, |b| b.kind == "diagram");

        // Wire c -> b as well, so `b` has an inbound and an outbound edge.
        block_ops(
            &ws,
            &previews,
            &serde_json::json!({
                "entry": "main.wcl", "file": "main.wcl",
                "etag": crate::edit::content_etag(&text),
                "ops": [{ "op": "connect_add", "span": span_json(diagram), "from": "c", "to": "b" }],
            }),
        )
        .expect("connect_add");

        let text = disk(&ws);
        let shape_b = span_of(&text, |b| {
            b.kind == "rect"
                && b.items.iter().any(|it| {
                    matches!(it, Item::Field(f)
                    if f.name == "id" && matches!(&f.expr, Expr::Identifier(s, _) if s == "b"))
                })
        });
        block_ops(
            &ws,
            &previews,
            &serde_json::json!({
                "entry": "main.wcl", "file": "main.wcl",
                "etag": crate::edit::content_etag(&text),
                "ops": [{ "op": "delete", "span": span_json(shape_b) }],
            }),
        )
        .expect("delete");
        let text = disk(&ws);
        assert!(!text.contains("a -> b"), "outbound edge went too: {text}");
        assert!(!text.contains("c -> b"), "inbound edge went too: {text}");
        assert!(text.contains("id = c"), "other shapes survive: {text}");
    }

    #[test]
    fn ops_edit_insert_move_delete() {
        let (_td, ws) = workspace_with(BODY_DOC);
        let previews = previews();
        let text = disk(&ws);
        let first_p = span_of(&text, |b| {
            b.kind == "p"
                && matches!(b.labels.first(), Some(Expr::Utf8(s)) if s.starts_with("First"))
        });

        // Edit the paragraph text + insert a callout after it, atomically.
        let v = block_ops(
            &ws,
            &previews,
            &serde_json::json!({
                "entry": "main.wcl", "file": "main.wcl",
                "etag": crate::edit::content_etag(&text),
                "ops": [
                    { "op": "set_label", "span": span_json(first_p), "slot": 0,
                      "text": "Edited paragraph" },
                    { "op": "insert_after", "span": span_json(first_p),
                      "source": "callout \"Note\" {\n  body = \"hi\"\n}" },
                ],
            }),
        )
        .expect("edit + insert");
        let new_text = disk(&ws);
        assert_eq!(v["file_text"], new_text.as_str());
        assert_eq!(v["etag"], crate::edit::content_etag(&new_text).as_str());
        assert!(new_text.contains("Edited paragraph"), "{new_text}");
        // The response spans slice the *new* text at the right blocks.
        let spans = v["spans"].as_array().unwrap();
        assert_eq!(spans.len(), 2, "{v:#}");
        let slice = |s: &serde_json::Value| {
            let (a, b) = (
                s["span"]["start"].as_u64().unwrap() as usize,
                s["span"]["end"].as_u64().unwrap() as usize,
            );
            new_text[a..b].to_string()
        };
        assert_eq!(spans[0]["role"], "edited");
        assert!(slice(&spans[0]).starts_with("p \"Edited paragraph\""));
        assert_eq!(spans[1]["role"], "inserted");
        assert!(slice(&spans[1]).starts_with("callout \"Note\""));
        // Order in the page: edited p, callout, second p.
        let ei = new_text.find("Edited paragraph").unwrap();
        let ci = new_text.find("callout").unwrap();
        let si = new_text.find("Second paragraph").unwrap();
        assert!(ei < ci && ci < si, "{new_text}");

        // Move the callout below the second paragraph, then delete it.
        let callout = span_of(&new_text, |b| b.kind == "callout");
        block_ops(
            &ws,
            &previews,
            &serde_json::json!({
                "entry": "main.wcl", "file": "main.wcl",
                "ops": [{ "op": "move", "span": span_json(callout), "dir": "down" }],
            }),
        )
        .expect("move");
        let text = disk(&ws);
        assert!(text.find("Second paragraph").unwrap() < text.find("callout").unwrap());
        let callout = span_of(&text, |b| b.kind == "callout");
        block_ops(
            &ws,
            &previews,
            &serde_json::json!({
                "entry": "main.wcl", "file": "main.wcl",
                "ops": [{ "op": "delete", "span": span_json(callout) }],
            }),
        )
        .expect("delete");
        assert!(!disk(&ws).contains("callout"), "{}", disk(&ws));
    }

    /// `move_to` resolves at the common-ancestor level: dragging a
    /// template title (an `h1` inside a transparent `edit_field` wrapper)
    /// below a sibling section moves the WHOLE wrapper — and the position
    /// is span-addressed, so invisible AST siblings between the two never
    /// skew it.
    #[test]
    fn ops_move_to_promotes_wrapped_blocks() {
        let doc = "import <wdoc.wcl>\n\nsite docs {\n  title = \"D\"\n  root = true\n}\n\npage index {\n  title = \"Hi\"\n\n  edit_field {\n    kind = \"concept\"\n    field = \"name\"\n\n    h1 \"Title\"\n  }\n\n  p \"Summary\"\n\n  p \"References\"\n}\n";
        let (_td, ws) = workspace_with(doc);
        let previews = previews();
        let text = disk(&ws);
        let h1 = span_of(&text, |b| b.kind == "h1");
        let refs = span_of(&text, |b| {
            b.kind == "p" && matches!(b.labels.first(), Some(Expr::Utf8(s)) if s == "References")
        });

        // Drop the title after the references paragraph.
        block_ops(
            &ws,
            &previews,
            &serde_json::json!({
                "entry": "main.wcl", "file": "main.wcl",
                "ops": [{ "op": "move_to", "span": span_json(h1), "after": span_json(refs) }],
            }),
        )
        .expect("move_to after");
        let text = disk(&ws);
        // The wrapper travelled with its h1, landing after both paragraphs.
        let (s, r, e) = (
            text.find("Summary").unwrap(),
            text.find("References").unwrap(),
            text.find("edit_field").unwrap(),
        );
        assert!(s < r && r < e, "wrapper moved below references: {text}");

        // And back above the summary via `before`.
        let h1 = span_of(&text, |b| b.kind == "h1");
        let summary = span_of(&text, |b| {
            b.kind == "p" && matches!(b.labels.first(), Some(Expr::Utf8(s)) if s == "Summary")
        });
        block_ops(
            &ws,
            &previews,
            &serde_json::json!({
                "entry": "main.wcl", "file": "main.wcl",
                "ops": [{ "op": "move_to", "span": span_json(h1), "before": span_json(summary) }],
            }),
        )
        .expect("move_to before");
        let text = disk(&ws);
        assert!(
            text.find("edit_field").unwrap() < text.find("Summary").unwrap(),
            "{text}"
        );

        // A block can't move relative to its own descendant.
        let ef = span_of(&text, |b| b.kind == "edit_field");
        let h1 = span_of(&text, |b| b.kind == "h1");
        assert!(
            block_ops(
                &ws,
                &previews,
                &serde_json::json!({
                    "entry": "main.wcl", "file": "main.wcl",
                    "ops": [{ "op": "move_to", "span": span_json(ef), "before": span_json(h1) }],
                }),
            )
            .is_err()
        );
    }

    fn count_blocks(items: &[Item]) -> usize {
        items
            .iter()
            .map(|it| match it {
                Item::Block(b) => 1 + count_blocks(&b.items),
                _ => 0,
            })
            .sum()
    }

    /// The client's no-reload path patches every live `data-wcl-span`
    /// anchor from the response's `span_map` — it must cover every block
    /// (nested included) and slice the new text at the right places.
    #[test]
    fn ops_span_map_covers_every_block() {
        let (_td, ws) = workspace_with(NESTED_DOC);
        let old = disk(&ws);
        let total = count_blocks(&parse_for_edit(&old, "t").unwrap().items);
        let first_p = span_of(&old, |b| {
            b.kind == "p" && matches!(b.labels.first(), Some(Expr::Utf8(s)) if s == "First")
        });

        let v = block_ops(
            &ws,
            &previews(),
            &serde_json::json!({
                "entry": "main.wcl", "file": "main.wcl",
                "ops": [{ "op": "move", "span": span_json(first_p), "dir": "down" }],
            }),
        )
        .expect("move");
        let new_text = disk(&ws);
        let map = v["span_map"].as_array().unwrap();
        assert_eq!(map.len(), total, "one entry per surviving block: {v:#}");
        // Every `from` slices the OLD text and `to` the NEW text at the
        // same block (same kind token; a move keeps content identical).
        for e in map {
            let f = (
                e["from"]["start"].as_u64().unwrap() as usize,
                e["from"]["end"].as_u64().unwrap() as usize,
            );
            let t = (
                e["to"]["start"].as_u64().unwrap() as usize,
                e["to"]["end"].as_u64().unwrap() as usize,
            );
            let old_kind = old[f.0..f.1].split_whitespace().next().unwrap();
            let new_kind = new_text[t.0..t.1].split_whitespace().next().unwrap();
            assert_eq!(old_kind, new_kind, "{e:#}");
        }
        // The moved paragraph and a nested li map to their exact new text.
        let mapped = |span: Span| -> String {
            let e = map
                .iter()
                .find(|e| {
                    e["from"]["start"].as_u64().unwrap() as usize == span.start
                        && e["from"]["end"].as_u64().unwrap() as usize == span.end
                })
                .unwrap_or_else(|| panic!("span {span:?} not in map"));
            let (a, b) = (
                e["to"]["start"].as_u64().unwrap() as usize,
                e["to"]["end"].as_u64().unwrap() as usize,
            );
            new_text[a..b].to_string()
        };
        assert!(mapped(first_p).starts_with("p \"First\""));
        let li_one = span_of(&old, |b| {
            b.kind == "li" && matches!(b.labels.first(), Some(Expr::Utf8(s)) if s == "one")
        });
        assert!(mapped(li_one).starts_with("li \"one\""));
        // And the move actually happened.
        assert!(new_text.find("list").unwrap() < new_text.find("p \"First\"").unwrap());
    }

    #[test]
    fn ops_span_map_on_set_visibility_and_inserts() {
        let (_td, ws) = workspace_with(NESTED_DOC);
        let old = disk(&ws);
        let total = count_blocks(&parse_for_edit(&old, "t").unwrap().items);
        let first_p = span_of(&old, |b| {
            b.kind == "p" && matches!(b.labels.first(), Some(Expr::Utf8(s)) if s == "First")
        });

        // set_visibility + an insert in one batch: the map still covers
        // exactly the surviving pre-edit blocks (the inserted subtree's
        // sentinel spans are skipped).
        let v = block_ops(
            &ws,
            &previews(),
            &serde_json::json!({
                "entry": "main.wcl", "file": "main.wcl",
                "ops": [
                    { "op": "set_visibility", "span": span_json(first_p),
                      "except_sites": ["deck"] },
                    { "op": "insert_after", "span": span_json(first_p),
                      "source": "p \"Inserted\"" },
                ],
            }),
        )
        .expect("visibility + insert");
        let new_text = disk(&ws);
        let map = v["span_map"].as_array().unwrap();
        assert_eq!(map.len(), total, "inserted block not in the map: {v:#}");
        let e = map
            .iter()
            .find(|e| e["from"]["start"].as_u64().unwrap() as usize == first_p.start)
            .unwrap();
        let (a, b) = (
            e["to"]["start"].as_u64().unwrap() as usize,
            e["to"]["end"].as_u64().unwrap() as usize,
        );
        // A block's span starts at its kind token — the decorator sits just
        // before the mapped slice in the new text.
        assert!(
            new_text[a..b].starts_with("p \"First\""),
            "edited block maps to itself: {}",
            &new_text[a..b]
        );
        assert!(
            new_text.contains("@except(sites = [:deck]) p \"First\"")
                || new_text.contains("@except(sites = [:deck])\np \"First\""),
            "decorator written: {new_text}"
        );
    }

    #[test]
    fn source_classifies_literal_list_tables() {
        let doc = "import <wdoc.wcl>\n\nsite docs {\n  title = \"D\"\n  root = true\n}\n\npage index {\n  title = \"Hi\"\n\n  table {\n    header = [\"Signal\", \"Plain\"]\n    rows = [[\"Audience\", \"AI agents\"], [\"Lifespan\", \"Long-lived\"]]\n  }\n}\n";
        let (_td, ws) = workspace_with(doc);
        let table = span_of(&disk(&ws), |b| b.kind == "table");

        let v = block_source(
            &ws,
            &serde_json::json!({ "file": "main.wcl", "span": span_json(table) }),
        )
        .expect("source");
        // All-string list → `list` with items; list-of-lists → `rows`.
        assert_eq!(v["fields"]["header"]["state"], "list", "{v:#}");
        assert_eq!(v["fields"]["header"]["items"][1], "Plain");
        assert_eq!(v["fields"]["rows"]["state"], "rows", "{v:#}");
        assert_eq!(v["fields"]["rows"]["rows"][1][0], "Lifespan");

        // A computed rows expression stays `computed` (no grid).
        let doc2 = doc.replace(
            "rows = [[\"Audience\", \"AI agents\"], [\"Lifespan\", \"Long-lived\"]]",
            "rows = map([\"x\"], fn(s: utf8) -> list<utf8> { [s, s] })",
        );
        std::fs::write(main_wcl(&ws), &doc2).unwrap();
        let table = span_of(&doc2, |b| b.kind == "table");
        let v = block_source(
            &ws,
            &serde_json::json!({ "file": "main.wcl", "span": span_json(table) }),
        )
        .expect("source");
        assert_eq!(v["fields"]["rows"]["state"], "computed", "{v:#}");
    }

    #[test]
    fn ops_remove_field_on_nested_shape() {
        let doc = "import <wdoc.wcl>\n\nsite docs {\n  title = \"The Docs\"\n  root = true\n}\n\npage index {\n  title = \"Hi\"\n\n  diagram {\n    width = 320\n    height = 160\n\n    rect {\n      id = a\n      x = 20.0\n      y = 30.0\n      width = 80.0\n      height = 50.0\n      fill = \"#88c0d0\"\n    }\n  }\n}\n";
        let (_td, ws) = workspace_with(doc);
        let rect = span_of(&disk(&ws), |b| b.kind == "rect");

        // Reset-position batch: drop x/y plus a field that was never there —
        // absent fields are tolerated so clients can batch removals blindly.
        let v = block_ops(
            &ws,
            &previews(),
            &serde_json::json!({
                "entry": "main.wcl", "file": "main.wcl",
                "ops": [
                    { "op": "remove_field", "span": span_json(rect), "field": "x" },
                    { "op": "remove_field", "span": span_json(rect), "field": "y" },
                    { "op": "remove_field", "span": span_json(rect), "field": "cx" },
                ],
            }),
        )
        .expect("remove_field");
        let text = disk(&ws);
        assert!(!text.contains("x = 20"), "{text}");
        assert!(!text.contains("y = 30"), "{text}");
        assert!(text.contains("fill = \"#88c0d0\""), "{text}");
        // The edited span in the response slices the rect in the new text.
        let s = &v["spans"].as_array().unwrap()[0];
        let (a, b) = (
            s["span"]["start"].as_u64().unwrap() as usize,
            s["span"]["end"].as_u64().unwrap() as usize,
        );
        assert!(text[a..b].starts_with("rect"), "{}", &text[a..b]);
    }

    #[test]
    fn ops_conflict_and_rollback() {
        let (_td, ws) = workspace_with(BODY_DOC);
        let before = disk(&ws);
        let p = span_of(&before, |b| b.kind == "p");

        // A stale etag is refused with the `conflict:` prefix the router
        // maps to 409, and leaves the file untouched.
        let e = block_ops(
            &ws,
            &previews(),
            &serde_json::json!({
                "entry": "main.wcl", "file": "main.wcl", "etag": "stale",
                "ops": [{ "op": "delete", "span": span_json(p) }],
            }),
        )
        .unwrap_err();
        assert!(e.starts_with("conflict:"), "{e}");
        assert_eq!(disk(&ws), before);

        // An edit that breaks the schema (a `page` needs a `title`) rolls
        // back: error, disk unchanged.
        let page = span_of(&before, |b| b.kind == "page");
        assert!(
            block_ops(
                &ws,
                &previews(),
                &serde_json::json!({
                    "entry": "main.wcl", "file": "main.wcl",
                    "ops": [{ "op": "replace_source", "span": span_json(page),
                              "source": "page index {\n  title = 42\n}" }],
                }),
            )
            .is_err()
        );
        assert_eq!(disk(&ws), before, "schema-breaking edit must roll back");

        // A bad fragment (two blocks) is rejected before anything happens.
        let e = block_ops(
            &ws,
            &previews(),
            &serde_json::json!({
                "entry": "main.wcl", "file": "main.wcl",
                "ops": [{ "op": "insert_after", "span": span_json(p),
                          "source": "p \"a\"\n\np \"b\"" }],
            }),
        )
        .unwrap_err();
        assert!(e.contains("exactly one block"), "{e}");
    }

    #[test]
    fn source_classifies_slots() {
        let doc = "import <wdoc.wcl>\n\nsite docs {\n  title = \"D\"\n  root = true\n}\n\nlet greeting = \"hi\"\n\npage index {\n  title = \"Hi\"\n\n  p \"Literal text\"\n\n  p $\"computed ${greeting}\"\n}\n";
        let (_td, ws) = workspace_with(doc);
        let literal = span_of(doc, |b| {
            b.kind == "p" && matches!(b.labels.first(), Some(Expr::Utf8(_)))
        });
        let computed = span_of(doc, |b| {
            b.kind == "p" && matches!(b.labels.first(), Some(Expr::InterpolatedString { .. }))
        });

        let v = block_source(
            &ws,
            &serde_json::json!({ "file": "main.wcl", "span": span_json(literal) }),
        )
        .expect("literal");
        assert_eq!(v["kind"], "p");
        assert_eq!(v["source"], "p \"Literal text\"");
        assert_eq!(v["labels"][0]["state"], "literal");
        assert_eq!(v["labels"][0]["text"], "Literal text");

        let v = block_source(
            &ws,
            &serde_json::json!({ "file": "main.wcl", "span": span_json(computed) }),
        )
        .expect("computed");
        assert_eq!(v["labels"][0]["state"], "computed");
        assert!(v["labels"][0]["text"].is_null());
    }

    /// A per-block view toggle rides the `@except(sites = …)` decorator, and
    /// a block whose visibility the toggles can't express is refused.
    #[test]
    fn visibility_toggle_round_trip() {
        let (_td, ws) = workspace_with(BODY_DOC);
        let previews = previews();
        let p = span_of(&disk(&ws), |b| b.kind == "p");

        // Hide the paragraph from the deck + training views.
        block_ops(
            &ws,
            &previews,
            &serde_json::json!({
                "entry": "main.wcl", "file": "main.wcl",
                "ops": [{ "op": "set_visibility", "span": span_json(p),
                          "except_sites": ["deck", "training"] }],
            }),
        )
        .expect("set_visibility");
        let text = disk(&ws);
        assert!(
            text.contains("@except(sites = [:deck, :training])"),
            "{text}"
        );

        // The classification reflects it.
        let p2 = span_of(&text, |b| b.kind == "p");
        let v = block_source(
            &ws,
            &serde_json::json!({ "file": "main.wcl", "span": span_json(p2) }),
        )
        .expect("source");
        assert_eq!(v["visibility"]["custom"], false);
        assert_eq!(
            v["visibility"]["except_sites"],
            serde_json::json!(["deck", "training"])
        );

        // Empty list removes the decorator again.
        block_ops(
            &ws,
            &previews,
            &serde_json::json!({
                "entry": "main.wcl", "file": "main.wcl",
                "ops": [{ "op": "set_visibility", "span": span_json(p2),
                          "except_sites": [] }],
            }),
        )
        .expect("clear visibility");
        assert!(!disk(&ws).contains("@except"), "{}", disk(&ws));

        // A block with @only is custom: classified, and the toggle refuses.
        let custom_doc = BODY_DOC.replace(
            "  p \"First paragraph\"",
            "  @only(sites = [:docs])\n  p \"First paragraph\"",
        );
        std::fs::write(main_wcl(&ws), &custom_doc).unwrap();
        let pc = span_of(&custom_doc, |b| b.kind == "p");
        let v = block_source(
            &ws,
            &serde_json::json!({ "file": "main.wcl", "span": span_json(pc) }),
        )
        .expect("source");
        assert_eq!(v["visibility"]["custom"], true);
        let e = block_ops(
            &ws,
            &previews,
            &serde_json::json!({
                "entry": "main.wcl", "file": "main.wcl",
                "ops": [{ "op": "set_visibility", "span": span_json(pc),
                          "except_sites": ["deck"] }],
            }),
        )
        .unwrap_err();
        assert!(e.contains("custom"), "{e}");
    }

    #[test]
    fn unit_field_and_unit_create_append_mode() {
        let (_td, ws) = workspace_with(OBJECT_DOC);
        let previews = previews();

        // Set a field on a located object.
        unit_field(
            &ws,
            &previews,
            &serde_json::json!({
                "entry": "main.wcl", "kind": "thing", "target": "alpha",
                "field": "note", "value": "updated note",
            }),
        )
        .expect("unit_field");
        assert!(
            disk(&ws).contains("note = \"updated note\""),
            "{}",
            disk(&ws)
        );

        // Create a new instance: appended to the file already holding the
        // most `thing`s (main.wcl), duplicate ids rejected.
        let v = unit_create(
            &ws,
            &previews,
            &serde_json::json!({
                "entry": "main.wcl",
                "unit": { "kind": "thing", "id": "gamma",
                          "fields": { "note": "third" } },
            }),
        )
        .expect("unit_create");
        assert_eq!(v["file"], "main.wcl");
        let text = disk(&ws);
        assert!(text.contains("thing \"gamma\""), "{text}");
        assert!(text.contains("note = \"third\""), "{text}");
        let e = unit_create(
            &ws,
            &previews,
            &serde_json::json!({
                "entry": "main.wcl",
                "unit": { "kind": "thing", "id": "gamma" },
            }),
        )
        .unwrap_err();
        assert!(e.contains("already exists"), "{e}");
    }

    #[test]
    fn unit_create_per_file_layout_with_pin() {
        let td = tempfile::tempdir().unwrap();
        write_mini_wskill(td.path());
        let ws = Workspace::at(td.path());
        let root = ws.root_dir().to_path_buf();

        let v = unit_create(
            &ws,
            &previews(),
            &serde_json::json!({
                "entry": "main.wcl",
                "unit": { "kind": "concept", "id": "gamma",
                          "fields": { "name": "Gamma" } },
                "pin": { "index_id": "lang" },
            }),
        )
        .expect("unit_create");
        assert_eq!(v["file"], "data/concepts/gamma.wcl");
        // One-per-file layout: its own file, imported by the aggregator,
        // pinned into the index — all in one commit.
        let unit = std::fs::read_to_string(root.join("data/concepts/gamma.wcl")).unwrap();
        assert!(unit.contains("concept gamma"), "{unit}");
        assert!(unit.contains("name = \"Gamma\""), "{unit}");
        let agg = std::fs::read_to_string(root.join("data/concepts/main.wcl")).unwrap();
        assert!(agg.contains("import \"./gamma.wcl\""), "{agg}");
        let idx = std::fs::read_to_string(root.join("data/indexes.wcl")).unwrap();
        assert!(idx.contains("related = [alpha, beta, gamma]"), "{idx}");
    }

    #[test]
    fn palette_lists_kinds_and_components() {
        let doc = format!(
            "{OBJECT_DOC}\nwdoc_component metric_card {{\n  wdoc_slot label\n  wdoc_slot status {{\n    default = \"ok\"\n  }}\n  wdoc_body {{\n    p $\"${{label}}\"\n  }}\n}}\n"
        );
        let (_td, ws) = workspace_with(&doc);

        let v = palette(&ws, "main.wcl", Some("docs"), None).expect("palette");
        assert_eq!(v["site_type"], "book");
        assert_eq!(v["wskill"], false);
        // The user schema kind, with introspected fields.
        let kinds = v["unit_kinds"].as_array().unwrap();
        let thing = kinds
            .iter()
            .find(|k| k["kind"] == "thing")
            .unwrap_or_else(|| panic!("no thing kind: {v:#}"));
        let fields = thing["fields"].as_array().unwrap();
        let name = fields.iter().find(|f| f["name"] == "name").unwrap();
        assert_eq!(name["inline_slot"], 0);
        assert_eq!(name["optional"], false);
        let note = fields.iter().find(|f| f["name"] == "note").unwrap();
        assert_eq!(note["optional"], true);
        // wdoc's own document gathers (site, page, …) are not offered.
        assert!(
            !kinds
                .iter()
                .any(|k| k["kind"] == "site" || k["kind"] == "page"),
            "{v:#}"
        );
        // Curated body kinds carry insertion snippets.
        let body = v["body_kinds"].as_array().unwrap();
        assert!(body.iter().any(|k| k["kind"] == "p"));
        assert!(
            body.iter()
                .all(|k| k["template_source"].as_str().is_some_and(|s| !s.is_empty()))
        );
        // Diagram shape kinds: SvgBlock descendants with introspected fields.
        let shapes = v["diagram_kinds"].as_array().unwrap();
        let process = shapes
            .iter()
            .find(|k| k["kind"] == "process")
            .unwrap_or_else(|| panic!("no process shape kind: {v:#}"));
        let pf = process["fields"].as_array().unwrap();
        for want in ["x", "y", "width", "height"] {
            assert!(pf.iter().any(|f| f["name"] == want), "{v:#}");
        }
        assert!(shapes.iter().any(|k| k["kind"] == "rect"), "{v:#}");
        // Page-level HTML blocks don't extend SvgBlock.
        assert!(!shapes.iter().any(|k| k["kind"] == "diagram"), "{v:#}");
        // The authored component with its slot contract.
        let comps = v["components"].as_array().unwrap();
        let card = comps.iter().find(|c| c["name"] == "metric_card").unwrap();
        let slots = card["slots"].as_array().unwrap();
        assert_eq!(slots.len(), 2, "{v:#}");
        let label = slots.iter().find(|s| s["name"] == "label").unwrap();
        assert_eq!(label["required"], true);
        let status_slot = slots.iter().find(|s| s["name"] == "status").unwrap();
        assert_eq!(status_slot["required"], false);
        assert_eq!(status_slot["default"], "ok");
    }
}
