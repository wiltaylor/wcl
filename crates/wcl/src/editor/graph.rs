//! The Design-mode unit graph: every wskill unit as a laid-out node with
//! its `related` / index-membership edges and per-view visibility, down to
//! the individual body blocks — so the graph shows exactly which blocks
//! ship in which view, and the client can toggle them.
//!
//! `GET /api/graph?entry=…&sites=book,deck,…&kinds=book=book,deck=presentation,…`
//! — `sites` is the wskill's view site-name list and `kinds` maps each site
//! to its artifact kind (both from the grouped `/api/sites` payload).
//! Per-view booleans combine two mechanisms: each block's
//! `@except(sites = […])` decorator (anything richer reports `custom` and
//! the client sends users to the source), and — for top-level units and
//! indexes — the wskill `audience` routing (`:book`/`:ai`/`:both`): the
//! book renders `!= :ai`, the skill `!= :book`, and indexes exist only in
//! those two views. Layout is server-side via the deterministic diagram
//! force solver ([`wcl_wdoc::layout_graph`]).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::State;
use axum::http::Uri;
use axum::response::Response;

use wcl_lang::ast::{self, Item};
use wcl_lang::{Document, Span, Value, parse_for_edit};

use super::blocks::{ast_label, first_label, unit_kinds, value_string, visibility_json};

use super::{EditorState, run_blocking};
use crate::serve::{query_param, sandboxed};

pub(super) async fn handle_graph(State(state): State<Arc<EditorState>>, uri: Uri) -> Response {
    let entry = query_param(&uri, "entry");
    let sites = query_param(&uri, "sites").unwrap_or_default();
    let kinds = query_param(&uri, "kinds").unwrap_or_default();
    let state2 = Arc::clone(&state);
    run_blocking(move || {
        let entry = entry.ok_or("missing entry")?;
        graph(&state2, &entry, &sites, &kinds)
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
    /// The wskill audience routing value (`book` / `ai` / `both`) — the
    /// block's own field, else its kind schema's declared default.
    audience: String,
    blocks: Vec<serde_json::Value>,
    related: Vec<String>,
    related_editable: bool,
    /// For `index` nodes: the ordered pinned ids (the `related` list) —
    /// the index panel edits this order.
    pinned: Vec<String>,
    /// For `index` nodes: nested sub-indexes (`{id, title, pinned,
    /// related_editable, children}`, recursive) — the index panel's
    /// sub-headings. Sub-indexes are not graph nodes; their pins ride the
    /// top-level index's edges with an `index_id` attribution.
    children: Vec<serde_json::Value>,
}

/// The ordered `related` id list of an index/unit block (empty when the
/// field is absent or not a literal-enough list to evaluate).
fn related_ids(b: &wcl_lang::Block<'_>) -> Vec<String> {
    b.field("related")
        .and_then(|f| f.value().ok().cloned())
        .map(|v| match v {
            Value::List(items) => items.iter().map(value_string).collect(),
            _ => Vec::new(),
        })
        .unwrap_or_default()
}

/// A `related` list is editable only when absent or a literal list — a
/// computed expression must not be clobbered by pin/unpin/reorder writes.
fn related_editable_of(blk: Option<&ast::Block>) -> bool {
    blk.is_some_and(|blk| {
        !blk.items.iter().any(|it| {
            matches!(it, Item::Field(f)
                if f.name == "related" && !matches!(f.expr, ast::Expr::ListLit { .. }))
        })
    })
}

/// Recurse an index's nested sub-index blocks into the panel's tree
/// payload, pushing every nested pin as `(top_id, owning id, unit id)`.
fn index_children(
    src: &ast::Source,
    b: &wcl_lang::Block<'_>,
    top_id: &str,
    pins: &mut Vec<(String, String, String)>,
) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for c in b.blocks().filter(|c| c.kind() == "index") {
        let Some(id) = first_label(&c) else { continue };
        let title = c
            .field("name")
            .and_then(|f| f.value().ok().cloned())
            .as_ref()
            .map(value_string)
            .unwrap_or_else(|| id.clone());
        let pinned = related_ids(&c);
        for rid in &pinned {
            pins.push((top_id.to_string(), id.clone(), rid.clone()));
        }
        let children = index_children(src, &c, top_id, pins);
        out.push(serde_json::json!({
            "id": id,
            "title": title,
            "pinned": pinned,
            "related_editable": related_editable_of(super::find_block_at(&src.items, c.span())),
            "children": children,
        }));
    }
    out
}

fn graph(
    state: &EditorState,
    entry: &str,
    sites_csv: &str,
    kinds_csv: &str,
) -> Result<serde_json::Value, String> {
    let entry_abs = sandboxed(&state.root_dir, &state.root_dir.join(entry))
        .ok_or_else(|| format!("file outside the served tree: {entry}"))?;
    let doc = wcl_wdoc::open_doc_for_edit(&entry_abs).map_err(super::err_str)?;
    let sites: Vec<String> = sites_csv
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    // site name → artifact kind (`book` / `ai_skill` / `presentation` / …).
    let site_kinds: HashMap<String, String> = kinds_csv
        .split(',')
        .filter_map(|pair| {
            let (site, kind) = pair.trim().split_once('=')?;
            (!site.is_empty()).then(|| (site.to_string(), kind.to_string()))
        })
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
    // (top-level index id, owning index id, unit id) — the owner differs
    // from the top-level id for pins inside nested sub-indexes.
    let mut pins: Vec<(String, String, String)> = Vec::new();

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
            let src = parse_for_edit(&text, file.display().to_string()).map_err(super::err_str)?;
            asts.insert(file.clone(), src);
        }
        let src = &asts[&file];
        let ast_block = super::find_block_at(&src.items, span);
        let visibility = ast_block
            .map(visibility_json)
            .unwrap_or_else(|| serde_json::json!({ "except_sites": [], "custom": false }));
        let blocks = ast_block
            .map(|blk| content_blocks(state, blk, &file, &sites))
            .unwrap_or_default();
        // The out-port is editable only when the `related` field is absent
        // or a literal list — a computed expression must not be clobbered.
        let related_editable = related_editable_of(ast_block);

        // Audience routing: the block's own field wins, else the kind
        // schema's declared default (research ships `:ai`, the rest `:book`).
        let audience = b
            .field("audience")
            .and_then(|f| f.value().ok().cloned())
            .map(|v| value_string(&v))
            .unwrap_or_else(|| default_audience(&doc, &kind));

        // Edges.
        let related = related_ids(&b);
        let mut children = Vec::new();
        if is_index {
            for rid in &related {
                pins.push((id.clone(), id.clone(), rid.clone()));
            }
            children = index_children(src, &b, &id, &mut pins);
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
            audience,
            blocks,
            pinned: if is_index {
                related.clone()
            } else {
                Vec::new()
            },
            related: if is_index { Vec::new() } else { related },
            related_editable,
            children,
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
    for (top_id, owner_id, unit_id) in &pins {
        if let (Some(&i), Some(&j)) = (
            index_of_id.get(top_id.as_str()),
            index_of_id.get(unit_id.as_str()),
        ) {
            layout_edges.push((i, j));
            // `index_id` is the level whose `related` list holds the pin —
            // the sub-index itself for nested pins (edge writes target it).
            edges.push(serde_json::json!({
                "from": key_of(i), "to": key_of(j), "kind": "pin", "index_id": owner_id,
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
                "span": super::span_json(n.span),
                "x": x, "y": y, "w": w, "h": h,
                "visibility": n.visibility,
                "audience": n.audience,
                "views": node_views_map(
                    &sites,
                    &site_kinds,
                    &n.visibility,
                    &n.audience,
                    n.node_type == "index",
                ),
                "organized": if n.node_type == "unit" {
                    organized_sites(&n.kind, &sites, &site_kinds)
                } else {
                    Vec::new()
                },
                "blocks": n.blocks,
                "related_editable": n.related_editable,
                "pinned": n.pinned,
                "children": n.children,
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

/// Per-view membership for a top-level unit / index node: the `@except`
/// visibility axis AND the wskill audience routing. The book renders
/// `audience != :ai`, the skill `!= :book`; indexes shape only those two
/// views (a deck / training site reports them absent), while units in the
/// other views stay visibility-governed (their data is selected by the
/// template, not by audience). A site with no known kind (a caller that
/// didn't pass `kinds`) keeps the plain visibility behaviour.
fn node_views_map(
    sites: &[String],
    site_kinds: &HashMap<String, String>,
    visibility: &serde_json::Value,
    audience: &str,
    is_index: bool,
) -> serde_json::Value {
    let except: Vec<&str> = visibility["except_sites"]
        .as_array()
        .map(|a| a.iter().filter_map(|s| s.as_str()).collect())
        .unwrap_or_default();
    let map: serde_json::Map<String, serde_json::Value> = sites
        .iter()
        .map(|s| {
            let visible = !except.contains(&s.as_str());
            let routed = match site_kinds.get(s).map(String::as_str) {
                Some("book") => audience != "ai",
                Some("ai_skill") => audience != "book",
                Some(_) => !is_index,
                None => true,
            };
            (s.clone(), (visible && routed).into())
        })
        .collect();
    serde_json::Value::Object(map)
}

/// The sites whose own navigation includes a unit of `kind` STRUCTURALLY —
/// the training syllabus is built from the lesson/module data itself, and a
/// deck renders its `presentation` unit as the slides. Such units are never
/// index-pinned yet aren't "unindexed": the client folds these into the
/// membership counts so lessons stop reading as orphans (and stop matching
/// the unindexed filter). Kind names are hardcoded like the audience
/// routing above — they're the canonical wskill base-schema vocabulary.
fn organized_sites(
    kind: &str,
    sites: &[String],
    site_kinds: &HashMap<String, String>,
) -> Vec<String> {
    sites
        .iter()
        .filter(|s| {
            matches!(
                (site_kinds.get(*s).map(String::as_str), kind),
                (Some("training"), "lesson" | "module") | (Some("presentation"), "presentation")
            )
        })
        .cloned()
        .collect()
}

/// The declared `@default` of a kind schema's `audience` field, else
/// `book` (the base schema's default for every unit kind except research).
fn default_audience(doc: &Document, kind: &str) -> String {
    doc.block_schema(kind)
        .and_then(|schema| {
            schema
                .effective_fields()
                .into_iter()
                .find(|f| f.name() == "audience")
                .and_then(|f| f.default_value().as_ref().map(value_string))
        })
        .unwrap_or_else(|| "book".to_string())
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
        "span": super::span_json(b.span),
        "views": views_map(sites, &visibility),
        "visibility": visibility,
    })
}
