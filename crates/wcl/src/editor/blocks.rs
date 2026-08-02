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
use axum::http::StatusCode;
use axum::response::Response;

use wcl_lang::ast::{self, Expr, Item};
use wcl_lang::{Span, edit as ast_edit, format as wcl_format, parse_expr, parse_for_edit};

use super::kinds::{Kind, KindModel};
use super::placement::{Placement, pin_into_index, place_unit, write_new_block};
use super::preview::Sessions;
use super::util::{ast_label, is_identifier, span_field, stale_span};
use super::{EditorState, Workspace, run_blocking};
use crate::serve::{json_error, parse_json_body};

// ---------------------------------------------------------------------------
// Request context
// ---------------------------------------------------------------------------

/// Sandbox-check a repo-relative file from the request body.
fn file_field(ws: &Workspace, v: &serde_json::Value) -> Result<PathBuf, String> {
    ws.abs(crate::edit::str_field(v, "file")?)
}

// ---------------------------------------------------------------------------
// `POST /api/block/source` — read a block's source + slot classification
// ---------------------------------------------------------------------------

/// Body: `{ file, span: {start, end} }` → the block's exact source slice
/// plus the block's cells ([`super::cell`]): positional `labels` and named
/// `fields`, each carrying the state its form control is chosen from
/// (`text` slots are inline-editable; `computed` ones lock the client to
/// the fragment editor).
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
        "cells": super::cell::block_cells(block),
        "connections": connections,
        "visibility": visibility_json(block),
    }))
}

/// The block's visibility state as the toggles UI understands it, as JSON.
/// The classification itself is [`wcl_wdoc::declared_visibility`]'s — the
/// `@only`/`@except` vocabulary is wdoc's, and the build's own anchor stamps
/// read the same classifier, so a stamp can't disagree with what
/// `set_visibility` will accept.
pub(super) fn visibility_json(block: &ast::Block) -> serde_json::Value {
    let v = wcl_wdoc::declared_visibility(block);
    serde_json::json!({ "except_sites": v.except_sites, "custom": v.custom })
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

/// Body: `{ entry, page_file?, unit: { kind, id, file?, type_name? },
/// fields?, pin?: { index_id } }`.
///
/// What to write comes from the [kind model](super::kinds): whether the id
/// is an identifier or a string, and whether to seed an empty prose body.
/// Where it goes comes from [`super::placement`] — an explicit `file`
/// (Data mode's `@wdoc.editable` hint) overrides the convention-derived
/// placement. `pin` appends the id to the named `index` block's `related`
/// list. All changes land in one [`crate::edit::commit`] (rollback covers
/// them all).
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
    if crate::edit::locate_object(&doc_entry, kind, Some(id), HashMap::new()).is_ok() {
        return Err(format!("a `{kind}` with id `{id}` already exists"));
    }

    // Everything derived from the document is derived inside this scope, so
    // the model's borrow of it — and the document itself — are gone by the
    // time the commit rewrites the files they were read from.
    let (new_file, changes) = build_unit(ws, &doc_entry, v, unit, kind, id)?;

    crate::edit::commit(&doc_entry, changes)?;
    previews.invalidate();
    Ok(serde_json::json!({
        "ok": true,
        "file": ws.rel(&new_file)?,
        "id": id,
    }))
}

/// Build the new block and stage every file change creating it implies:
/// the file it lands in, its aggregator import, and an optional index pin.
/// Answers the file the block landed in.
fn build_unit(
    ws: &Workspace,
    doc_entry: &Path,
    v: &serde_json::Value,
    unit: &serde_json::Value,
    kind: &str,
    id: &str,
) -> Result<(PathBuf, Vec<(PathBuf, String)>), String> {
    let doc = wcl_wdoc::open_doc_for_edit(doc_entry).map_err(super::err_str)?;

    // The kind model answers both schema questions this path asks — is the
    // `@inline(0)` slot identifier-typed (the wskill unit convention), and
    // does an instance carry a prose body. `type_name` (the fully-qualified
    // schema name, served with every kind entry) disambiguates kind names
    // shared across namespaces — a WAD `container` must not be schema'd by
    // wdoc's diagram-grouping shape of the same name.
    let model = KindModel::new(&doc);
    let described = model.describe(
        kind,
        unit.get("type_name").and_then(serde_json::Value::as_str),
    );
    let label = if described.as_ref().is_some_and(Kind::id_is_identifier) {
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
    if described.as_ref().is_some_and(Kind::has_body) {
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
        _ => place_unit(&model, &doc, doc_entry, kind)?,
    };
    let new_file = write_new_block(placement, id, block, doc_entry, &mut changes)?;

    if let Some(pin) = v.get("pin") {
        let index_id = crate::edit::str_field(pin, "index_id")?;
        pin_into_index(&doc, doc_entry, index_id, id, &mut changes)?;
    }
    Ok((new_file, changes))
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
#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::span_json;
    use crate::editor::testsupport::{
        BODY_DOC, Edits, OBJECT_DOC, kind_is, labelled, span_of, with_id, workspace_built_by,
        workspace_with, write_mini_wskill,
    };

    /// A document whose page holds a diagram of three wired shapes.
    const DIAGRAM_DOC: &str = "import <wdoc.wcl>\n\nsite docs {\n  title = \"D\"\n  root = true\n}\n\npage index {\n  title = \"Hi\"\n\n  diagram {\n    width = 400\n    height = 300\n    rect {\n      id = a\n    }\n    rect {\n      id = b\n    }\n    rect {\n      id = c\n    }\n    a -> b\n  }\n}\n";

    /// Nested fixture for the span-map tests: 7 blocks (site, page, p,
    /// list, li, li, p).
    const NESTED_DOC: &str = "import <wdoc.wcl>\n\nsite docs {\n  title = \"D\"\n  root = true\n}\n\npage index {\n  title = \"Hi\"\n\n  p \"First\"\n\n  list {\n    li \"one\"\n    li \"two\"\n  }\n\n  p \"Last\"\n}\n";

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
        let ed = Edits::main(&ws);

        // Add one plain edge and one kinded edge.
        ed.run(|at| {
            serde_json::json!([
                { "op": "connect_add", "span": at(&kind_is("diagram")), "from": "b", "to": "c" },
                { "op": "connect_add", "span": at(&kind_is("diagram")), "from": "c", "to": "a", "kind": "flow" },
            ])
        })
        .expect("connect_add");
        let text = ed.text();
        assert!(text.contains("b -> c"), "{text}");
        assert!(text.contains("c -> a :flow"), "{text}");

        // Removing one leaves the others alone.
        ed.run(|at| {
            serde_json::json!([
                { "op": "connect_remove", "span": at(&kind_is("diagram")), "from": "b", "to": "c" },
            ])
        })
        .expect("connect_remove");
        let text = ed.text();
        assert!(!text.contains("b -> c"), "{text}");
        assert!(text.contains("c -> a :flow"), "{text}");
        assert!(text.contains("a -> b"), "{text}");
    }

    #[test]
    fn connect_ops_reject_nonsense() {
        let (_td, ws) = workspace_with(DIAGRAM_DOC);
        let ed = Edits::main(&ws);

        for (op, from, to) in [
            ("connect_add", "a", "a"),    // self-connection
            ("connect_add", "a", "b"),    // already wired
            ("connect_remove", "a", "c"), // no such edge
        ] {
            let r = ed.run(|at| {
                serde_json::json!([
                    { "op": op, "span": at(&kind_is("diagram")), "from": from, "to": to },
                ])
            });
            assert!(r.is_err(), "{op} {from}->{to} should fail");
        }
    }

    /// Deleting a shape takes its edges with it — the sync failure that let a
    /// removed shape leave dangling `a -> b` statements behind.
    #[test]
    fn deleting_a_shape_removes_its_connections() {
        let (_td, ws) = workspace_with(DIAGRAM_DOC);
        let ed = Edits::main(&ws);

        // Wire c -> b as well, so `b` has an inbound and an outbound edge.
        ed.run(|at| {
            serde_json::json!([
                { "op": "connect_add", "span": at(&kind_is("diagram")), "from": "c", "to": "b" },
            ])
        })
        .expect("connect_add");
        ed.run(|at| serde_json::json!([{ "op": "delete", "span": at(&with_id("rect", "b")) }]))
            .expect("delete");

        let text = ed.text();
        assert!(!text.contains("a -> b"), "outbound edge went too: {text}");
        assert!(!text.contains("c -> b"), "inbound edge went too: {text}");
        assert!(text.contains("id = c"), "other shapes survive: {text}");
    }

    #[test]
    fn ops_edit_insert_move_delete() {
        let (_td, ws) = workspace_with(BODY_DOC);
        let ed = Edits::main(&ws);

        // Edit the paragraph text + insert a callout after it, atomically.
        let v = ed
            .run(|at| {
                serde_json::json!([
                    { "op": "set_label", "span": at(&labelled("p", "First paragraph")),
                      "slot": 0, "text": "Edited paragraph" },
                    { "op": "insert_after", "span": at(&labelled("p", "First paragraph")),
                      "source": "callout \"Note\" {\n  body = \"hi\"\n}" },
                ])
            })
            .expect("edit + insert");
        let new_text = ed.text();
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
        ed.run(|at| {
            serde_json::json!([{ "op": "move", "span": at(&kind_is("callout")), "dir": "down" }])
        })
        .expect("move");
        let text = ed.text();
        assert!(text.find("Second paragraph").unwrap() < text.find("callout").unwrap());
        ed.run(|at| serde_json::json!([{ "op": "delete", "span": at(&kind_is("callout")) }]))
            .expect("delete");
        assert!(!ed.text().contains("callout"), "{}", ed.text());
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
        let ed = Edits::main(&ws);

        // Drop the title after the references paragraph.
        ed.run(|at| {
            serde_json::json!([{
                "op": "move_to", "span": at(&kind_is("h1")),
                "after": at(&labelled("p", "References")),
            }])
        })
        .expect("move_to after");
        let text = ed.text();
        // The wrapper travelled with its h1, landing after both paragraphs.
        let (s, r, e) = (
            text.find("Summary").unwrap(),
            text.find("References").unwrap(),
            text.find("edit_field").unwrap(),
        );
        assert!(s < r && r < e, "wrapper moved below references: {text}");

        // And back above the summary via `before`.
        ed.run(|at| {
            serde_json::json!([{
                "op": "move_to", "span": at(&kind_is("h1")),
                "before": at(&labelled("p", "Summary")),
            }])
        })
        .expect("move_to before");
        let text = ed.text();
        assert!(
            text.find("edit_field").unwrap() < text.find("Summary").unwrap(),
            "{text}"
        );

        // A block can't move relative to its own descendant.
        assert!(
            ed.run(|at| {
                serde_json::json!([{
                    "op": "move_to", "span": at(&kind_is("edit_field")),
                    "before": at(&kind_is("h1")),
                }])
            })
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
            &Sessions::default(),
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
            &Sessions::default(),
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
        // A list of cells → `list` with items; list-of-lists → `rows`.
        assert_eq!(v["cells"]["fields"]["header"]["state"], "list", "{v:#}");
        assert_eq!(v["cells"]["fields"]["header"]["items"][1]["text"], "Plain");
        assert_eq!(v["cells"]["fields"]["rows"]["state"], "rows", "{v:#}");
        assert_eq!(
            v["cells"]["fields"]["rows"]["rows"][1][0]["text"],
            "Lifespan"
        );

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
        assert_eq!(v["cells"]["fields"]["rows"]["state"], "computed", "{v:#}");
    }

    #[test]
    fn ops_remove_field_on_nested_shape() {
        let doc = "import <wdoc.wcl>\n\nsite docs {\n  title = \"The Docs\"\n  root = true\n}\n\npage index {\n  title = \"Hi\"\n\n  diagram {\n    width = 320\n    height = 160\n\n    rect {\n      id = a\n      x = 20.0\n      y = 30.0\n      width = 80.0\n      height = 50.0\n      fill = \"#88c0d0\"\n    }\n  }\n}\n";
        let (_td, ws) = workspace_with(doc);
        let ed = Edits::main(&ws);

        // Reset-position batch: drop x/y plus a field that was never there —
        // absent fields are tolerated so clients can batch removals blindly.
        let v = ed
            .run(|at| {
                serde_json::json!([
                    { "op": "remove_field", "span": at(&kind_is("rect")), "field": "x" },
                    { "op": "remove_field", "span": at(&kind_is("rect")), "field": "y" },
                    { "op": "remove_field", "span": at(&kind_is("rect")), "field": "cx" },
                ])
            })
            .expect("remove_field");
        let text = ed.text();
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
            &Sessions::default(),
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
                &Sessions::default(),
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
            &Sessions::default(),
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
        assert_eq!(v["cells"]["labels"][0]["state"], "text");
        assert_eq!(v["cells"]["labels"][0]["text"], "Literal text");

        let v = block_source(
            &ws,
            &serde_json::json!({ "file": "main.wcl", "span": span_json(computed) }),
        )
        .expect("computed");
        assert_eq!(v["cells"]["labels"][0]["state"], "computed");
        assert!(v["cells"]["labels"][0]["text"].is_null());
    }

    /// A per-block view toggle rides the `@except(sites = …)` decorator, and
    /// a block whose visibility the toggles can't express is refused.
    #[test]
    fn visibility_toggle_round_trip() {
        let (_td, ws) = workspace_with(BODY_DOC);
        let ed = Edits::main(&ws);
        let classify = |text: &str| {
            block_source(
                &ws,
                &serde_json::json!({
                    "file": "main.wcl",
                    "span": span_json(span_of(text, kind_is("p"))),
                }),
            )
            .expect("source")
        };

        // Hide the paragraph from the deck + training views.
        ed.run(|at| {
            serde_json::json!([{
                "op": "set_visibility", "span": at(&kind_is("p")),
                "except_sites": ["deck", "training"],
            }])
        })
        .expect("set_visibility");
        let text = ed.text();
        assert!(
            text.contains("@except(sites = [:deck, :training])"),
            "{text}"
        );

        // The classification reflects it.
        let v = classify(&text);
        assert_eq!(v["visibility"]["custom"], false);
        assert_eq!(
            v["visibility"]["except_sites"],
            serde_json::json!(["deck", "training"])
        );

        // Empty list removes the decorator again.
        ed.run(|at| {
            serde_json::json!([{
                "op": "set_visibility", "span": at(&kind_is("p")), "except_sites": [],
            }])
        })
        .expect("clear visibility");
        assert!(!ed.text().contains("@except"), "{}", ed.text());

        // A block with @only is custom: classified, and the toggle refuses.
        let custom_doc = BODY_DOC.replace(
            "  p \"First paragraph\"",
            "  @only(sites = [:docs])\n  p \"First paragraph\"",
        );
        std::fs::write(main_wcl(&ws), &custom_doc).unwrap();
        assert_eq!(classify(&custom_doc)["visibility"]["custom"], true);
        let e = ed
            .run(|at| {
                serde_json::json!([{
                    "op": "set_visibility", "span": at(&kind_is("p")),
                    "except_sites": ["deck"],
                }])
            })
            .unwrap_err();
        assert!(e.contains("custom"), "{e}");
    }

    #[test]
    fn unit_field_and_unit_create_append_mode() {
        let (_td, ws) = workspace_with(OBJECT_DOC);
        let previews = Sessions::default();

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
        let (_td, ws) = workspace_built_by(write_mini_wskill);
        let root = ws.root_dir().to_path_buf();

        let v = unit_create(
            &ws,
            &Sessions::default(),
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
}
