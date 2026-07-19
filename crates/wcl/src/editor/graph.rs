//! The Design-mode unit graph: every wskill unit as a laid-out node with
//! its `related` / index-membership edges and per-view visibility, down to
//! the individual body blocks — so the graph shows exactly which blocks
//! ship in which view, and the client can toggle them.
//!
//! `GET /api/graph?entry=…&sites=book,deck,…` — `sites` is the wskill's
//! view site-name list (the client has it from the grouped `/api/sites`
//! payload); per-view booleans are computed from each block's
//! `@except(sites = […])` decorator (anything richer reports `custom` and
//! the client sends users to the source). Layout is server-side via the
//! deterministic diagram force solver ([`wcl_wdoc::layout_graph`]).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::State;
use axum::http::Uri;
use axum::response::Response;

use wcl_lang::ast::{self, Item};
use wcl_lang::{Span, Value, parse_for_edit};

use super::blocks::{ast_label, first_label, unit_kinds, value_string, visibility_json};
use super::{EditorState, run_blocking};
use crate::serve::{query_param, sandboxed};

pub(super) async fn handle_graph(State(state): State<Arc<EditorState>>, uri: Uri) -> Response {
    let entry = query_param(&uri, "entry");
    let sites = query_param(&uri, "sites").unwrap_or_default();
    let state2 = Arc::clone(&state);
    run_blocking(move || {
        let entry = entry.ok_or("missing entry")?;
        graph(&state2, &entry, &sites)
    })
    .await
}

/// One graph node under construction.
struct NodeInfo {
    key: String,
    node_type: &'static str, // "unit" | "index"
    id: String,
    kind: String,
    title: String,
    file: PathBuf,
    span: Span,
    visibility: serde_json::Value,
    blocks: Vec<serde_json::Value>,
    related: Vec<String>,
    related_editable: bool,
    /// For `index` nodes: the ordered pinned ids (the `related` list) —
    /// the index panel edits this order.
    pinned: Vec<String>,
}

fn graph(state: &EditorState, entry: &str, sites_csv: &str) -> Result<serde_json::Value, String> {
    let entry_abs = sandboxed(&state.root_dir, &state.root_dir.join(entry))
        .ok_or_else(|| format!("file outside the served tree: {entry}"))?;
    let doc = wcl_wdoc::open_doc_for_edit(&entry_abs).map_err(|e| e.to_string())?;
    let sites: Vec<String> = sites_csv
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();

    let kind_names: Vec<String> = unit_kinds(&doc)
        .iter()
        .filter_map(|k| k.get("kind").and_then(serde_json::Value::as_str))
        .filter(|k| *k != "index")
        .map(str::to_string)
        .collect();

    // Per-file AST cache: block-level detail (children, decorators) comes
    // from the parse, keyed by the doc view's spans.
    let mut asts: HashMap<PathBuf, ast::Source> = HashMap::new();

    let mut nodes: Vec<NodeInfo> = Vec::new();
    let mut pins: Vec<(String, String)> = Vec::new(); // (index id, unit id)

    for (path, b) in doc.blocks_with_source() {
        let kind = b.kind().to_string();
        let is_index = kind == "index";
        if !is_index && !kind_names.contains(&kind) {
            continue;
        }
        let Some(id) = first_label(&b) else { continue };
        let title = ["name", "title", "topic"]
            .iter()
            .find_map(|f| b.field(f).and_then(|f| f.value().ok().cloned()))
            .as_ref()
            .map(value_string)
            .unwrap_or_else(|| id.clone());
        let file = path
            .map(Path::to_path_buf)
            .unwrap_or_else(|| entry_abs.clone());
        let span = b.span();

        // AST-level detail for visibility + body blocks.
        if !asts.contains_key(&file) {
            let text = crate::edit::read(&file)?;
            let src =
                parse_for_edit(&text, file.display().to_string()).map_err(|e| e.to_string())?;
            asts.insert(file.clone(), src);
        }
        let src = &asts[&file];
        let ast_block = find_block_at(&src.items, span);
        let visibility = ast_block
            .map(visibility_json)
            .unwrap_or_else(|| serde_json::json!({ "except_sites": [], "custom": false }));
        let blocks = ast_block
            .map(|blk| content_blocks(state, blk, &file, &sites))
            .unwrap_or_default();
        // The out-port is editable only when the `related` field is absent
        // or a literal list — a computed expression must not be clobbered.
        let related_editable = ast_block.is_some_and(|blk| {
            !blk.items.iter().any(|it| {
                matches!(it, Item::Field(f)
                    if f.name == "related" && !matches!(f.expr, ast::Expr::ListLit { .. }))
            })
        });

        // Edges.
        let related: Vec<String> = b
            .field("related")
            .and_then(|f| f.value().ok().cloned())
            .map(|v| match v {
                Value::List(items) => items.iter().map(value_string).collect(),
                _ => Vec::new(),
            })
            .unwrap_or_default();
        if is_index {
            for rid in &related {
                pins.push((id.clone(), rid.clone()));
            }
        }

        nodes.push(NodeInfo {
            key: format!("{kind}:{id}"),
            node_type: if is_index { "index" } else { "unit" },
            id,
            kind,
            title,
            file,
            span,
            visibility,
            blocks,
            pinned: if is_index {
                related.clone()
            } else {
                Vec::new()
            },
            related: if is_index { Vec::new() } else { related },
            related_editable,
        });
    }

    // Resolve edges to node indices (related ids match any unit kind).
    let index_of_id: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let key_of = |i: usize| nodes[i].key.clone();
    let mut edges: Vec<serde_json::Value> = Vec::new();
    let mut layout_edges: Vec<(usize, usize)> = Vec::new();
    for (i, n) in nodes.iter().enumerate() {
        for rid in &n.related {
            if let Some(&j) = index_of_id.get(rid.as_str()) {
                layout_edges.push((i, j));
                edges.push(serde_json::json!({
                    "from": key_of(i), "to": key_of(j), "kind": "related",
                }));
            }
        }
    }
    for (index_id, unit_id) in &pins {
        if let (Some(&i), Some(&j)) = (
            index_of_id.get(index_id.as_str()),
            index_of_id.get(unit_id.as_str()),
        ) {
            layout_edges.push((i, j));
            edges.push(serde_json::json!({
                "from": key_of(i), "to": key_of(j), "kind": "pin",
            }));
        }
    }

    // Deterministic force layout over title-sized boxes.
    let sizes: Vec<(f64, f64)> = nodes
        .iter()
        .map(|n| {
            let w = (n.title.chars().count() as f64 * 7.5 + 30.0).clamp(90.0, 260.0);
            (w, 48.0)
        })
        .collect();
    let offsets = wcl_wdoc::layout_graph(&sizes, &layout_edges);

    let nodes_json: Vec<serde_json::Value> = nodes
        .iter()
        .zip(sizes.iter().zip(offsets.iter()))
        .map(|(n, (&(w, h), &(x, y)))| {
            serde_json::json!({
                "key": n.key,
                "type": n.node_type,
                "id": n.id,
                "kind": n.kind,
                "title": n.title,
                "file": rel(state, &n.file),
                "span": { "start": n.span.start, "end": n.span.end },
                "x": x, "y": y, "w": w, "h": h,
                "visibility": n.visibility,
                "views": views_map(&sites, &n.visibility),
                "blocks": n.blocks,
                "related_editable": n.related_editable,
                "pinned": n.pinned,
            })
        })
        .collect();
    Ok(serde_json::json!({
        "ok": true,
        "sites": sites,
        "nodes": nodes_json,
        "edges": edges,
    }))
}

fn rel(state: &EditorState, file: &Path) -> String {
    std::fs::canonicalize(file)
        .unwrap_or_else(|_| file.to_path_buf())
        .strip_prefix(&state.root_dir)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| file.display().to_string())
}

/// `{ site: visible }` from a `visibility_json` payload (custom ⇒ every
/// site reports visible; the `custom` flag tells the client to defer).
fn views_map(sites: &[String], visibility: &serde_json::Value) -> serde_json::Value {
    let except: Vec<&str> = visibility["except_sites"]
        .as_array()
        .map(|a| a.iter().filter_map(|s| s.as_str()).collect())
        .unwrap_or_default();
    let map: serde_json::Map<String, serde_json::Value> = sites
        .iter()
        .map(|s| (s.clone(), (!except.contains(&s.as_str())).into()))
        .collect();
    serde_json::Value::Object(map)
}

fn find_block_at(items: &[Item], span: Span) -> Option<&ast::Block> {
    for item in items {
        if let Item::Block(b) = item {
            if b.span == span {
                return Some(b);
            }
            if let Some(found) = find_block_at(&b.items, span) {
                return Some(found);
            }
        }
    }
    None
}

/// The unit's content blocks, flattened one level: direct children, with
/// transparent containers (`body`, the addressable per-step `bodies`)
/// spliced so the graph shows the blocks that actually render.
fn content_blocks(
    state: &EditorState,
    unit: &ast::Block,
    file: &Path,
    sites: &[String],
) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for item in &unit.items {
        let Item::Block(b) = item else { continue };
        if b.kind == "body" {
            for inner in &b.items {
                if let Item::Block(c) = inner {
                    out.push(block_entry(state, c, file, sites, None));
                }
            }
        } else {
            let label = ast_label(b);
            out.push(block_entry(state, b, file, sites, label.as_deref()));
        }
    }
    out
}

fn block_entry(
    state: &EditorState,
    b: &ast::Block,
    file: &Path,
    sites: &[String],
    label: Option<&str>,
) -> serde_json::Value {
    let preview: String = label
        .map(str::to_string)
        .or_else(|| ast_label(b))
        .unwrap_or_default()
        .chars()
        .take(60)
        .collect();
    let visibility = visibility_json(b);
    serde_json::json!({
        "kind": b.kind,
        "preview": preview,
        "file": rel(state, file),
        "span": { "start": b.span.start, "end": b.span.end },
        "views": views_map(sites, &visibility),
        "visibility": visibility,
    })
}
