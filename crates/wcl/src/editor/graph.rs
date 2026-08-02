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

use super::blocks::visibility_json;
use super::kinds::KindModel;
use super::util::{ast_label, field_string, first_label, value_string};

use super::{EditorState, Workspace, run_blocking};
use crate::serve::query_param;

pub(super) async fn handle_graph(State(state): State<Arc<EditorState>>, uri: Uri) -> Response {
    let entry = query_param(&uri, "entry");
    let sites = query_param(&uri, "sites").unwrap_or_default();
    let kinds = query_param(&uri, "kinds").unwrap_or_default();
    let state2 = Arc::clone(&state);
    run_blocking(move || {
        let entry = entry.ok_or("missing entry")?;
        graph(&state2.ws, &entry, &sites, &kinds)
    })
    .await
}

/// Id of the synthetic top-level syllabus node (a training view's stand-in
/// for an `index`). Double-underscored so it cannot collide with an authored
/// block id, which WCL identifiers never start with.
pub(super) const SYLLABUS_ID: &str = "__course";

/// One graph node under construction.
struct NodeInfo {
    key: String,
    node_type: &'static str, // "unit" | "index"
    id: String,
    kind: String,
    title: String,
    /// The unit's one-line description (`summary`), when it declares one —
    /// a search field of its own, and the hit list's subtitle.
    summary: String,
    /// Every literal string the block carries, newline-joined — the prose
    /// the find-a-unit box searches. See [`block_text`].
    text: String,
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
    ws: &Workspace,
    entry: &str,
    sites_csv: &str,
    kinds_csv: &str,
) -> Result<serde_json::Value, String> {
    let entry_abs = ws.abs(entry)?;
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

    let model = KindModel::new(&doc);
    let kind_names: Vec<String> = model.unit_kind_names();

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
            .map(|blk| content_blocks(ws, blk, &file, &sites))
            .unwrap_or_default();
        let text = ast_block.map(block_text).unwrap_or_default();
        // The out-port is editable only when the `related` field is absent
        // or a literal list — a computed expression must not be clobbered.
        let related_editable = related_editable_of(ast_block);

        // Audience routing: the block's own field wins, else the kind
        // schema's declared default (research ships `:ai`, the rest `:book`).
        let audience = b
            .field("audience")
            .and_then(|f| f.value().ok().cloned())
            .map(|v| value_string(&v))
            .unwrap_or_else(|| default_audience(&model, &kind));

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
            summary: field_string(&b, "summary").unwrap_or_default(),
            text,
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
                // The find-a-unit box's two extra search fields (id and
                // title are above) — see `block_text`.
                "summary": n.summary,
                "text": n.text,
                "file": rel(ws, &n.file),
                "span": super::span_json(n.span),
                "x": x, "y": y, "w": w, "h": h,
                "visibility": n.visibility,
                "audience": n.audience,
                "views": node_views_map(
                    &sites,
                    &site_kinds,
                    &n.visibility,
                    &n.kind,
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
    let mut nodes_json = nodes_json;
    nodes_json.extend(syllabus_nodes(&doc, &sites, &site_kinds, ws, &entry_abs));
    Ok(serde_json::json!({
        "ok": true,
        "sites": sites,
        "nodes": nodes_json,
        "edges": edges,
    }))
}

/// The training view's syllabus, shaped as index nodes so the index panel can
/// show and reorder it.
///
/// A course has no `index` blocks — its structure IS the data: `module`s and
/// `lesson`s ordered by `n`. Without this the panel is empty for a training
/// view. One top-level node ("Course") carries the ungrouped lessons as pins
/// and each module as a sub-level, mirroring the index / sub-index tree.
///
/// `syllabus: true` marks the levels as ordered-by-`n` rather than pinned by a
/// `related` list: reordering rewrites the lessons' `n` (see
/// `nav::syllabus_reorder`), and there is nothing to pin or unpin — a lesson
/// belongs to the course by existing. Emitted after layout with zero geometry,
/// since index nodes never render on the canvas.
fn syllabus_nodes(
    doc: &Document,
    sites: &[String],
    site_kinds: &HashMap<String, String>,
    ws: &Workspace,
    entry_abs: &Path,
) -> Vec<serde_json::Value> {
    let training: Vec<&String> = sites
        .iter()
        .filter(|s| site_kinds.get(*s).map(String::as_str) == Some("training"))
        .collect();
    if training.is_empty() {
        return Vec::new();
    }
    let (lessons, modules) = course_structure(doc);
    if lessons.is_empty() && modules.is_empty() {
        return Vec::new();
    }
    let views: serde_json::Map<String, serde_json::Value> = sites
        .iter()
        .map(|s| (s.clone(), training.contains(&s).into()))
        .collect();
    let children: Vec<serde_json::Value> = modules
        .iter()
        .map(|m| {
            serde_json::json!({
                "id": m.id,
                "title": m.title,
                "pinned": m.lessons,
                "related_editable": true,
                "syllabus": true,
                "children": [],
            })
        })
        .collect();
    vec![serde_json::json!({
        "key": format!("index:{SYLLABUS_ID}"),
        "type": "index",
        "id": SYLLABUS_ID,
        "kind": "index",
        "title": "Course",
        // Synthesized from the lesson data — no block, so no prose.
        "summary": "",
        "text": "",
        "file": rel(ws, entry_abs),
        "span": super::span_json(Span::new(0, 0)),
        "x": 0.0, "y": 0.0, "w": 0.0, "h": 0.0,
        "visibility": serde_json::json!({ "except_sites": [], "custom": false }),
        "audience": "book",
        "views": serde_json::Value::Object(views),
        "organized": Vec::<String>::new(),
        "blocks": Vec::<serde_json::Value>::new(),
        "related_editable": true,
        "syllabus": true,
        "pinned": lessons,
        "children": children,
    })]
}

/// One part of a course: its id, display title, and lesson ids in `n` order.
pub(super) struct CourseModule {
    pub id: String,
    pub title: String,
    pub lessons: Vec<String>,
}

/// The course: ungrouped lesson ids in `n` order, then each module in `n`
/// order with its own lessons ordered the same way.
pub(super) fn course_structure(doc: &Document) -> (Vec<String>, Vec<CourseModule>) {
    let ordered = |blocks: Vec<wcl_lang::Block<'_>>| -> Vec<(u64, String)> {
        let mut v: Vec<(u64, String)> = blocks
            .iter()
            .filter_map(|b| Some((order_of(b), first_label(b)?)))
            .collect();
        v.sort_by_key(|(n, _)| *n);
        v
    };
    let lessons = ordered(doc.blocks().filter(|b| b.kind() == "lesson").collect())
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let mut modules: Vec<(u64, String, String, Vec<String>)> = doc
        .blocks()
        .filter(|b| b.kind() == "module")
        .filter_map(|m| {
            let id = first_label(&m)?;
            let title = m
                .field("title")
                .and_then(|f| f.value().ok().cloned())
                .as_ref()
                .map(value_string)
                .unwrap_or_else(|| id.clone());
            let kids = ordered(m.blocks().filter(|b| b.kind() == "lesson").collect())
                .into_iter()
                .map(|(_, id)| id)
                .collect();
            Some((order_of(&m), id, title, kids))
        })
        .collect();
    modules.sort_by_key(|(n, ..)| *n);
    (
        lessons,
        modules
            .into_iter()
            .map(|(_, id, title, lessons)| CourseModule { id, title, lessons })
            .collect(),
    )
}

/// A course block's `n` (its position); missing / non-numeric sorts last.
fn order_of(b: &wcl_lang::Block<'_>) -> u64 {
    b.field("n")
        .and_then(|f| f.value().ok().cloned())
        .and_then(|v| match v {
            Value::U32(n) => Some(n as u64),
            Value::U64(n) => Some(n),
            Value::I64(n) if n >= 0 => Some(n as u64),
            _ => None,
        })
        .unwrap_or(u64::MAX)
}

fn rel(ws: &Workspace, file: &Path) -> String {
    std::fs::canonicalize(file)
        .unwrap_or_else(|_| file.to_path_buf())
        .strip_prefix(ws.root_dir())
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
/// The site kind that OWNS a unit kind — the one view whose projection
/// renders it, because that view is built from this data and no other reads
/// it. `None` for reference content (concept / entity / fact / procedure /
/// research / index), which the book and the skill share and route by
/// `audience` instead.
///
/// Kind names are hardcoded like the audience routing below — they are the
/// canonical wskill base-schema vocabulary.
fn owning_view_kind(unit_kind: &str) -> Option<&'static str> {
    match unit_kind {
        "lesson" | "module" => Some("training"),
        "presentation" => Some("presentation"),
        _ => None,
    }
}

fn node_views_map(
    sites: &[String],
    site_kinds: &HashMap<String, String>,
    visibility: &serde_json::Value,
    unit_kind: &str,
    audience: &str,
    is_index: bool,
) -> serde_json::Value {
    let except: Vec<&str> = visibility["except_sites"]
        .as_array()
        .map(|a| a.iter().filter_map(|s| s.as_str()).collect())
        .unwrap_or_default();
    let owner = if is_index {
        None
    } else {
        owning_view_kind(unit_kind)
    };
    let map: serde_json::Map<String, serde_json::Value> = sites
        .iter()
        .map(|s| {
            let visible = !except.contains(&s.as_str());
            let kind = site_kinds.get(s).map(String::as_str);
            let routed = match owner {
                // A view-owned kind appears ONLY in the view built from it: a
                // lesson is not book content, and a concept is not a lesson.
                Some(owner) => kind == Some(owner),
                // Reference content is shared by the book and the skill,
                // routed by audience; the data-owned views don't render it.
                None => match kind {
                    Some("book") => audience != "ai",
                    Some("ai_skill") => audience != "book",
                    Some(_) => false,
                    None => true,
                },
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
    let Some(owner) = owning_view_kind(kind) else {
        return Vec::new();
    };
    sites
        .iter()
        .filter(|s| site_kinds.get(*s).map(String::as_str) == Some(owner))
        .cloned()
        .collect()
}

/// The declared `@default` of a kind schema's `audience` field, else
/// `book` (the base schema's default for every unit kind except research).
fn default_audience(model: &KindModel<'_>, kind: &str) -> String {
    model
        .get(kind)
        .and_then(|k| k.field_default("audience"))
        .unwrap_or_else(|| "book".to_string())
}

/// The string literals a block's subtree carries, in source order and
/// newline-joined: its string labels, its string-valued fields (through
/// lists, interpolations and record literals), then the same again for
/// every nested block — a unit's `body` paragraphs, its tables, a
/// procedure's steps. This is the searchable prose behind the editor's
/// find-a-unit box, so it is deliberately only the *content*: field names,
/// block kinds, identifiers and symbols stay out, or searching "related"
/// would hit every unit that declares the field rather than the one whose
/// prose says the word. Strings a *computed* field would produce are
/// likewise absent — this reads the source, not an evaluation.
///
/// Nothing is truncated: a hit the box can't show is a hit the reader
/// can't find. The cost is bounded by what a wskill is — the largest one
/// in this repo (`docs/wskills/wcl`, 189 units, ~3x the size the box was
/// asked to stay usable at) contributes 138 KB to a 402 KB payload.
///
/// The ids and names the box also searches ride on the node itself, so
/// they need no extraction here (they land in the text as well, being
/// string fields; the client attributes a hit to the narrowest source
/// that carries it).
fn block_text(b: &ast::Block) -> String {
    let mut out = Vec::new();
    collect_block_strings(b, &mut out);
    out.join("\n")
}

fn collect_block_strings(b: &ast::Block, out: &mut Vec<String>) {
    for label in &b.labels {
        collect_expr_strings(label, out);
    }
    for item in &b.items {
        match item {
            Item::Field(f) => collect_expr_strings(&f.expr, out),
            Item::Block(c) => collect_block_strings(c, out),
            _ => {}
        }
    }
}

fn collect_expr_strings(e: &ast::Expr, out: &mut Vec<String>) {
    match e {
        ast::Expr::Utf8(s) | ast::Expr::Ascii(s) => push_text(s, out),
        ast::Expr::InterpolatedString { parts, .. } => {
            for part in parts {
                match part {
                    ast::TemplatePart::Literal(s) => push_text(s, out),
                    ast::TemplatePart::Expr(inner) => collect_expr_strings(inner, out),
                }
            }
        }
        ast::Expr::ListLit { elements, .. } => {
            for el in elements {
                collect_expr_strings(el, out);
            }
        }
        // A bare record literal is a value like any other — a `wdoc` block
        // field can hold a list of them (chart series, table rows), and
        // their strings are as much prose as a paragraph's.
        ast::Expr::Record { fields, .. } => {
            for f in fields {
                collect_expr_strings(&f.value, out);
            }
        }
        _ => {}
    }
}

/// Push one literal, split on its own newlines so a heredoc body arrives as
/// lines — the client shows the matching line as the hit's snippet.
fn push_text(s: &str, out: &mut Vec<String>) {
    for line in s.lines() {
        let line = line.trim();
        if !line.is_empty() {
            out.push(line.to_string());
        }
    }
}

/// The unit's content blocks, flattened one level: direct children, with
/// transparent containers (`body`, the addressable per-step `bodies`)
/// spliced so the graph shows the blocks that actually render.
fn content_blocks(
    ws: &Workspace,
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
                    out.push(block_entry(ws, c, file, sites, None));
                }
            }
        } else {
            let label = ast_label(b);
            out.push(block_entry(ws, b, file, sites, label.as_deref()));
        }
    }
    out
}

fn block_entry(
    ws: &Workspace,
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
        "file": rel(ws, file),
        "span": super::span_json(b.span),
        "views": views_map(sites, &visibility),
        "visibility": visibility,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::blocks::block_ops;
    use crate::editor::preview::Sessions;
    use crate::editor::testsupport::{
        workspace_built_by, write_mini_wskill, write_mini_wskill_nested, write_mini_wskill_training,
    };

    fn model(ws: &Workspace, sites: &str, kinds: &str) -> serde_json::Value {
        graph(ws, "main.wcl", sites, kinds).expect("graph")
    }

    fn node<'a>(v: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
        v["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == id)
            .unwrap_or_else(|| panic!("no node `{id}`: {v:#}"))
    }

    #[test]
    fn lists_units_edges_and_view_visibility() {
        let (_td, ws) = workspace_built_by(|root| {
            write_mini_wskill(root);
            // Give alpha a body with one deck-hidden paragraph.
            std::fs::write(
                root.join("data/concepts/alpha.wcl"),
                "concept alpha {\n  name = \"Alpha\"\n  body {\n    p \"Everywhere\"\n\n    @except(sites = [:deck])\n    p \"Book only\"\n  }\n}\n",
            )
            .unwrap();
            // The mini-wskill schema has no body child; extend it.
            let main = std::fs::read_to_string(root.join("main.wcl")).unwrap();
            let main = main.replace(
                "@block(\"concept\")\ntype Concept {\n  @inline(0) id: identifier\n  name: utf8\n}",
                "@block(\"body\") @schemaless\ntype UnitBody {\n}\n\n@block(\"concept\")\ntype Concept {\n  @inline(0) id: identifier\n  name: utf8\n  @child(\"body\") body: UnitBody?\n}",
            );
            std::fs::write(root.join("main.wcl"), main).unwrap();
        });

        let v = model(&ws, "book,deck", "");
        // alpha, beta, and the lang index.
        let alpha = node(&v, "alpha");
        assert_eq!(alpha["type"], "unit");
        assert_eq!(alpha["kind"], "concept");
        assert_eq!(alpha["title"], "Alpha");
        assert!(alpha["x"].is_number() && alpha["y"].is_number());
        let idx = node(&v, "lang");
        assert_eq!(idx["type"], "index");
        // Ordered pin list — the index panel edits this order.
        assert_eq!(idx["pinned"], serde_json::json!(["alpha", "beta"]));
        // Edges: the index pins alpha + beta.
        let edges = v["edges"].as_array().unwrap();
        assert!(
            edges.iter().any(|e| e["from"] == "index:lang"
                && e["to"] == "concept:alpha"
                && e["kind"] == "pin"),
            "{v:#}"
        );
        // Block-level per-view visibility: the body's paragraphs, with the
        // second hidden from the deck.
        let blocks = alpha["blocks"].as_array().unwrap();
        let hidden = blocks
            .iter()
            .find(|b| b["preview"] == "Book only")
            .unwrap_or_else(|| panic!("no body block listing: {v:#}"));
        assert_eq!(hidden["views"]["book"], true);
        assert_eq!(hidden["views"]["deck"], false);
        assert_eq!(hidden["visibility"]["custom"], false);
        let shown = blocks
            .iter()
            .find(|b| b["preview"] == "Everywhere")
            .unwrap();
        assert_eq!(shown["views"]["deck"], true);
    }

    /// The find-a-unit box searches four fields; two of them (`summary` and
    /// the body prose) exist only because the payload carries them.
    #[test]
    fn carries_the_searchable_summary_and_prose() {
        let (_td, ws) = workspace_built_by(|root| {
            write_mini_wskill(root);
            std::fs::write(
                root.join("data/concepts/alpha.wcl"),
                "concept alpha {\n  name = \"Alpha\"\n  summary = \"Spans address bytes\"\n  \
                 body {\n    p \"A span is a byte range into the source.\"\n    \
                 table {\n      row \"start | end\"\n    }\n    \
                 chart {\n      series = [{ label: \"Widened spans\" }]\n    }\n  }\n}\n",
            )
            .unwrap();
            let main = std::fs::read_to_string(root.join("main.wcl")).unwrap();
            let main = main.replace(
                "@block(\"concept\")\ntype Concept {\n  @inline(0) id: identifier\n  name: utf8\n}",
                "@block(\"body\") @schemaless\ntype UnitBody {\n}\n\n@block(\"concept\")\ntype Concept {\n  @inline(0) id: identifier\n  name: utf8\n  summary: utf8?\n  @child(\"body\") body: UnitBody?\n}",
            );
            std::fs::write(root.join("main.wcl"), main).unwrap();
        });

        let v = model(&ws, "book", "");
        let alpha = node(&v, "alpha");
        assert_eq!(alpha["summary"], "Spans address bytes");
        let text = alpha["text"].as_str().unwrap();
        // Nested block content is reachable, one line per literal.
        assert!(
            text.contains("A span is a byte range into the source."),
            "{text}"
        );
        assert!(text.contains("start | end"), "{text}");
        // Through a list of record literals, too — a chart's series.
        assert!(text.contains("Widened spans"), "{text}");
        // Field names, block kinds and identifiers are schema vocabulary,
        // not prose: searching "concept" must not match every concept.
        assert!(!text.contains("summary"), "{text}");
        assert!(!text.contains("concept"), "{text}");

        // A unit with neither reports empty strings, never a missing key —
        // the client reads all four fields off every node.
        let beta = node(&v, "beta");
        assert_eq!(beta["summary"], "");
        assert!(beta["text"].is_string());
    }

    /// A unit kind belongs to the ONE view built from it: a lesson is not book
    /// content, and a concept is not part of the course. Before this, every
    /// non-index unit reported visible in a training view, so selecting the
    /// training filter highlighted the whole graph.
    #[test]
    fn routes_units_to_the_view_that_renders_them() {
        let (_td, ws) = workspace_built_by(write_mini_wskill_training);
        let v = model(&ws, "book,course", "book=book,course=training");

        let alpha = node(&v, "alpha");
        assert_eq!(alpha["views"]["book"], true, "a concept is book content");
        assert_eq!(
            alpha["views"]["course"], false,
            "a concept is not part of the course"
        );

        let first = node(&v, "first");
        assert_eq!(first["views"]["course"], true, "a lesson is course content");
        assert_eq!(
            first["views"]["book"], false,
            "the book renders no lesson pages"
        );
        assert_eq!(
            first["organized"],
            serde_json::json!(["course"]),
            "lessons are organized structurally, not index-pinned"
        );
    }

    /// A course has no `index` blocks, so the index panel would be empty for a
    /// training view; the graph synthesizes its structure instead.
    #[test]
    fn synthesizes_a_syllabus_for_a_training_view() {
        let (_td, ws) = workspace_built_by(write_mini_wskill_training);
        let v = model(&ws, "book,course", "book=book,course=training");
        let syl = v["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["syllabus"] == true)
            .unwrap_or_else(|| panic!("no syllabus node: {v:#}"));
        assert_eq!(syl["type"], "index");
        assert_eq!(syl["pinned"], serde_json::json!(["first", "second"]));
        assert_eq!(syl["views"]["course"], true);
        assert_eq!(syl["views"]["book"], false, "the syllabus is course-only");

        // No syllabus without a training view among the sites.
        let v2 = model(&ws, "book", "book=book");
        assert!(
            !v2["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|n| n["syllabus"] == true)
        );
    }

    #[test]
    fn nested_index_children_and_pins() {
        let (_td, ws) = workspace_built_by(write_mini_wskill_nested);
        let v = model(&ws, "book", "");
        let idx = node(&v, "lang");
        assert_eq!(idx["pinned"], serde_json::json!(["alpha"]));
        let children = idx["children"].as_array().unwrap();
        assert_eq!(children.len(), 1, "{v:#}");
        assert_eq!(children[0]["id"], "lang_sub");
        assert_eq!(children[0]["title"], "Sub");
        assert_eq!(children[0]["pinned"], serde_json::json!(["beta"]));
        assert_eq!(children[0]["related_editable"], true);
        assert_eq!(children[0]["children"], serde_json::json!([]));
        // The sub-index never becomes a node of its own.
        assert!(
            !v["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|n| n["id"] == "lang_sub"),
            "{v:#}"
        );
        let edges = v["edges"].as_array().unwrap();
        assert!(
            edges.iter().any(|e| e["from"] == "index:lang"
                && e["to"] == "concept:alpha"
                && e["kind"] == "pin"
                && e["index_id"] == "lang"),
            "{v:#}"
        );
        assert!(
            edges.iter().any(|e| e["from"] == "index:lang"
                && e["to"] == "concept:beta"
                && e["kind"] == "pin"
                && e["index_id"] == "lang_sub"),
            "{v:#}"
        );

        // Pin alpha into the sub-index too: two pin edges to alpha, one
        // per owning level.
        crate::editor::nav::nav_op(
            &ws,
            &Sessions::default(),
            &serde_json::json!({
                "entry": "main.wcl", "op": "pin_unit",
                "index_id": "lang_sub", "unit_id": "alpha",
            }),
        )
        .expect("pin");
        let v = model(&ws, "book", "");
        let alpha_pins: Vec<&serde_json::Value> = v["edges"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "pin" && e["to"] == "concept:alpha")
            .collect();
        assert_eq!(alpha_pins.len(), 2, "{v:#}");
    }

    /// The graph view's edge writes: `related_add` / `related_remove` block
    /// ops on unit and index blocks, plus the `related_editable` flag.
    #[test]
    fn related_add_remove_roundtrip() {
        let (_td, ws) = workspace_built_by(|root| {
            write_mini_wskill(root);
            // Concepts need a `related` field for unit→unit edges.
            let main = std::fs::read_to_string(root.join("main.wcl")).unwrap();
            let main = main.replace(
                "@block(\"concept\")\ntype Concept {\n  @inline(0) id: identifier\n  name: utf8\n}",
                "@block(\"concept\")\ntype Concept {\n  @inline(0) id: identifier\n  name: utf8\n  related: list<identifier>?\n}",
            );
            std::fs::write(root.join("main.wcl"), main).unwrap();
        });
        let previews = Sessions::default();
        // One edge write, addressed by node id. Every commit reprints the
        // owning file, so the node's file+span are re-read from the model at
        // call time — the same re-anchoring the graph view does on refetch.
        let edge = |node_id: &str, op: &str, id: &str| {
            let v = model(&ws, "book", "");
            let n = node(&v, node_id);
            block_ops(
                &ws,
                &previews,
                &serde_json::json!({
                    "entry": "main.wcl", "file": n["file"],
                    "ops": [{ "op": op, "span": n["span"], "id": id }],
                }),
            )
        };

        let v = model(&ws, "book", "");
        assert_eq!(node(&v, "alpha")["related_editable"], true);
        assert!(
            !v["edges"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["kind"] == "related"),
            "{v:#}"
        );

        // Connect alpha → beta.
        edge("alpha", "related_add", "beta").expect("related_add");
        let text = std::fs::read_to_string(ws.root_dir().join("data/concepts/alpha.wcl")).unwrap();
        assert!(text.contains("related = [beta]"), "{text}");
        let v = model(&ws, "book", "");
        assert!(
            v["edges"].as_array().unwrap().iter().any(|e| {
                e["from"] == "concept:alpha" && e["to"] == "concept:beta" && e["kind"] == "related"
            }),
            "{v:#}"
        );

        // Duplicate, self-loop, and bad-id are refused.
        for (id, msg) in [
            ("beta", "already related"),
            ("alpha", "itself"),
            ("not an id", "not a valid"),
        ] {
            let e = edge("alpha", "related_add", id).unwrap_err();
            assert!(e.contains(msg), "{id}: {e}");
        }

        // Disconnect again; a second remove is refused.
        edge("alpha", "related_remove", "beta").expect("related_remove");
        let v = model(&ws, "book", "");
        assert!(
            !v["edges"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["kind"] == "related"),
            "{v:#}"
        );
        let e = edge("alpha", "related_remove", "beta").unwrap_err();
        assert!(e.contains("not in the related"), "{e}");

        // The same ops drive index pins: unpin beta, then re-pin it.
        let pins = |v: &serde_json::Value| {
            v["edges"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| e["kind"] == "pin")
                .count()
        };
        edge("lang", "related_remove", "beta").expect("unpin");
        assert_eq!(pins(&model(&ws, "book", "")), 1);
        edge("lang", "related_add", "beta").expect("re-pin");
        assert_eq!(pins(&model(&ws, "book", "")), 2);

        // A computed related list: flagged not-editable, and the op refuses.
        std::fs::write(
            ws.root_dir().join("data/concepts/alpha.wcl"),
            "concept alpha {\n  name = \"Alpha\"\n  related = concat([], [])\n}\n",
        )
        .unwrap();
        let v = model(&ws, "book", "");
        assert_eq!(node(&v, "alpha")["related_editable"], false, "{v:#}");
        let e = edge("alpha", "related_add", "beta").unwrap_err();
        assert!(e.contains("computed"), "{e}");
    }
}
