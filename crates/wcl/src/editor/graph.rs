//! The Design-mode unit graph — an **adapter** over [`wcl_wskill::Graph`].
//!
//! `GET /api/graph?entry=…&sites=book,deck,…&kinds=book=book,deck=presentation,…`
//! — `sites` is the wskill's view site-name list and `kinds` maps each site
//! to its artifact kind (both from the grouped `/api/sites` payload).
//!
//! Nothing here reads the wskill format. The library answers what the units,
//! indexes, edges and per-view membership *are* ([`wcl_wskill::Graph`],
//! [`Unit::shows_in`](wcl_wskill::Unit::shows_in)); this module places the
//! nodes ([`wcl_wdoc::layout_graph`]), rewrites the model's wskill-relative
//! anchors as the repo-relative paths every editor response speaks, and
//! serialises. The one thing it synthesizes is the training **syllabus**
//! node — a course has no `index` block, so the panel that edits index trees
//! is handed the course shaped like one.
//!
//! The site list arrives on the query rather than being taken from the
//! model's registry because the client picks which views it is looking at;
//! a site whose artifact kind the caller didn't send keeps plain visibility
//! behaviour (the model's audience routing needs the kind to route by).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use axum::extract::State;
use axum::http::Uri;
use axum::response::Response;

use wcl_lang::{Document, Span};
use wcl_wskill::{ContentBlock, Course, Edge, EdgeKind, Index, Unit, View, Visibility};

use super::util::anchor_file as rel;
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
/// for an `index`) — the id the nav ops address a course's top level by.
pub(super) const SYLLABUS_ID: &str = wcl_wskill::ops::COURSE_ID;

/// The wskill model behind `entry`, read from an already-open document.
///
/// The wskill root is the library's ([`wcl_wskill::root_for`]) so the two
/// endpoints and the CLI can't disagree about which folder a projection
/// entry belongs to.
pub(super) fn open_graph(doc: &Document, entry_abs: &Path) -> Result<wcl_wskill::Graph, String> {
    let root = wcl_wskill::root_for(entry_abs);
    wcl_wskill::Graph::from_document(doc, &root, entry_abs).map_err(|e| e.to_string())
}

/// One view the client asked about: the site name a visibility decorator
/// uses, and the artifact kind it projects (absent when the caller didn't
/// send a `kinds` mapping).
struct QueryView {
    site: String,
    /// The model's view for this site, when the kind is known — routing by
    /// `audience` is only answerable against an artifact kind.
    view: Option<View>,
}

impl QueryView {
    fn shows_unit(&self, unit: &Unit) -> bool {
        match &self.view {
            Some(v) => unit.shows_in(v),
            None => unit.visibility.shows_in(&self.site),
        }
    }

    fn shows_index(&self, index: &Index) -> bool {
        match &self.view {
            Some(v) => index.shows_in(v),
            None => index.visibility.shows_in(&self.site),
        }
    }

    fn kind(&self) -> Option<&str> {
        self.view.as_ref().map(|v| v.kind.as_str())
    }
}

/// `sites=a,b` + `kinds=a=book,b=training` as the views to report on.
fn query_views(sites_csv: &str, kinds_csv: &str) -> Vec<QueryView> {
    let kinds: HashMap<&str, &str> = kinds_csv
        .split(',')
        .filter_map(|pair| {
            let (site, kind) = pair.trim().split_once('=')?;
            (!site.is_empty()).then_some((site, kind))
        })
        .collect();
    sites_csv
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|site| QueryView {
            view: kinds.get(site).map(|kind| View {
                id: site.to_string(),
                kind: (*kind).to_string(),
                entry: String::new(),
                site: Some(site.to_string()),
            }),
            site: site.to_string(),
        })
        .collect()
}

fn graph(
    ws: &Workspace,
    entry: &str,
    sites_csv: &str,
    kinds_csv: &str,
) -> Result<serde_json::Value, String> {
    let entry_abs = ws.abs(entry)?;
    let doc = wcl_wdoc::open_doc_for_edit(&entry_abs).map_err(super::err_str)?;
    let model = open_graph(&doc, &entry_abs)?;
    let views = query_views(sites_csv, kinds_csv);
    let sites: Vec<&str> = views.iter().map(|v| v.site.as_str()).collect();

    // Layout runs over units and indexes together — an index pulls the units
    // it pins toward itself, even though the canvas draws only the units —
    // and in DECLARATION order, so the arrangement follows how the wskill is
    // written rather than which of the model's two lists a node landed in.
    let mut placed: Vec<Placed<'_>> = model
        .units
        .iter()
        .map(Placed::unit)
        .chain(model.indexes.iter().map(Placed::index))
        .collect();
    placed.sort_by(|a, b| a.anchor().cmp(&b.anchor()));

    let sizes: Vec<(f64, f64)> = placed.iter().map(|p| box_for(p.title())).collect();
    let slot: HashMap<String, usize> = placed
        .iter()
        .enumerate()
        .map(|(i, p)| (p.key(), i))
        .collect();
    let layout_edges: Vec<(usize, usize)> = model
        .edges
        .iter()
        .filter_map(|e| {
            Some((
                *slot.get(&e.from.to_string())?,
                *slot.get(&e.to.to_string())?,
            ))
        })
        .collect();
    let offsets = wcl_wdoc::layout_graph(&sizes, &layout_edges);

    let mut nodes: Vec<serde_json::Value> = placed
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let node = match p {
                Placed::Unit(u) => GraphNode {
                    node_type: "unit",
                    id: &u.id,
                    kind: &u.kind,
                    title: &u.title,
                    summary: u.summary.as_deref().unwrap_or_default(),
                    text: &u.text,
                    file: rel(ws, &model.root, &u.anchor),
                    span: super::span_json(u.anchor.span),
                    visibility: &u.visibility,
                    audience: &u.audience,
                    views: map_over(&views, |v| v.shows_unit(u)),
                    organized: organizing_sites(&views, &u.kind),
                    blocks: blocks_json(ws, &model.root, &u.blocks, &sites),
                    related_editable: u.related_editable,
                    ..GraphNode::default()
                },
                Placed::Index(x) => GraphNode {
                    node_type: "index",
                    id: &x.id,
                    kind: "index",
                    title: &x.title,
                    summary: x.summary.as_deref().unwrap_or_default(),
                    text: &x.text,
                    file: rel(ws, &model.root, &x.anchor),
                    span: super::span_json(x.anchor.span),
                    visibility: &x.visibility,
                    audience: &x.audience,
                    views: map_over(&views, |v| v.shows_index(x)),
                    blocks: blocks_json(ws, &model.root, &x.blocks, &sites),
                    related_editable: x.related_editable,
                    pinned: x.pinned.clone(),
                    children: sub_indexes(&x.children),
                    ..GraphNode::default()
                },
            };
            node.json(sizes[i], offsets[i])
        })
        .collect();
    nodes.extend(syllabus_node(&model, &views, ws, &entry_abs));

    Ok(serde_json::json!({
        "ok": true,
        "sites": sites,
        "nodes": nodes,
        "edges": model.edges.iter().map(edge_json).collect::<Vec<_>>(),
    }))
}

/// A node the force solver places: a unit or a top-level index, before it
/// becomes JSON. Both are laid out in one pass, so both need one handle.
enum Placed<'a> {
    Unit(&'a Unit),
    Index(&'a Index),
}

impl<'a> Placed<'a> {
    fn unit(u: &'a Unit) -> Self {
        Placed::Unit(u)
    }

    fn index(i: &'a Index) -> Self {
        Placed::Index(i)
    }

    fn anchor(&self) -> (&Path, usize) {
        let a = match self {
            Placed::Unit(u) => &u.anchor,
            Placed::Index(i) => &i.anchor,
        };
        (a.file.as_path(), a.span.start)
    }

    fn title(&self) -> &str {
        match self {
            Placed::Unit(u) => &u.title,
            Placed::Index(i) => &i.title,
        }
    }

    fn key(&self) -> String {
        match self {
            Placed::Unit(u) => u.key().to_string(),
            Placed::Index(i) => i.key().to_string(),
        }
    }
}

/// A node's box, sized to fit its title.
fn box_for(title: &str) -> (f64, f64) {
    (
        (title.chars().count() as f64 * 7.5 + 30.0).clamp(90.0, 260.0),
        48.0,
    )
}

/// One graph node's wire shape, stated once.
///
/// Three things become a node — a unit, an index, and the synthetic syllabus
/// — and they differ in a handful of fields out of eighteen. Building them
/// through one struct is what stops a key being added to one and silently
/// missing from another, which the client would read as `undefined`.
struct GraphNode<'a> {
    node_type: &'static str,
    id: &'a str,
    kind: &'a str,
    title: &'a str,
    /// The find-a-unit box's search fields, beyond id and title.
    summary: &'a str,
    text: &'a str,
    file: String,
    span: serde_json::Value,
    visibility: &'a Visibility,
    audience: &'a str,
    views: serde_json::Value,
    /// The views whose own navigation carries this node structurally.
    organized: Vec<String>,
    blocks: Vec<serde_json::Value>,
    related_editable: bool,
    pinned: Vec<String>,
    children: Vec<serde_json::Value>,
    /// Ordered by `n` rather than by a `related` list — see [`syllabus_node`].
    syllabus: bool,
}

impl Default for GraphNode<'_> {
    fn default() -> Self {
        GraphNode {
            node_type: "unit",
            id: "",
            kind: "",
            title: "",
            summary: "",
            text: "",
            file: String::new(),
            span: super::span_json(Span::new(0, 0)),
            visibility: EMPTY_VISIBILITY,
            audience: wcl_wskill::DEFAULT_AUDIENCE,
            views: serde_json::Value::Object(serde_json::Map::new()),
            organized: Vec::new(),
            blocks: Vec::new(),
            related_editable: true,
            pinned: Vec::new(),
            children: Vec::new(),
            syllabus: false,
        }
    }
}

/// The visibility a synthesized node reports: it has no block, so nothing
/// declares any.
static EMPTY_VISIBILITY: &Visibility = &Visibility {
    except_sites: Vec::new(),
    custom: false,
};

impl GraphNode<'_> {
    fn json(self, (w, h): (f64, f64), (x, y): (f64, f64)) -> serde_json::Value {
        serde_json::json!({
            "key": format!("{}:{}", self.kind, self.id),
            "type": self.node_type,
            "id": self.id,
            "kind": self.kind,
            "title": self.title,
            "summary": self.summary,
            "text": self.text,
            "file": self.file,
            "span": self.span,
            "x": x, "y": y, "w": w, "h": h,
            "visibility": visibility_json(self.visibility),
            "audience": self.audience,
            "views": self.views,
            "organized": self.organized,
            "blocks": self.blocks,
            "related_editable": self.related_editable,
            "pinned": self.pinned,
            "children": self.children,
            "syllabus": self.syllabus,
        })
    }
}

/// The queried sites whose own navigation carries a unit of `kind`
/// structurally rather than by pinning — a lesson in a training view, a
/// presentation in its deck. Which kinds those are is the library's.
fn organizing_sites(views: &[QueryView], kind: &str) -> Vec<String> {
    let Some(owner) = wcl_wskill::structural_view_kind(kind) else {
        return Vec::new();
    };
    views
        .iter()
        .filter(|v| v.kind() == Some(owner))
        .map(|v| v.site.clone())
        .collect()
}

fn edge_json(e: &Edge) -> serde_json::Value {
    match e.kind {
        EdgeKind::Related => serde_json::json!({
            "from": e.from.to_string(), "to": e.to.to_string(), "kind": "related",
        }),
        // `index_id` is the level whose `related` list holds the pin — the
        // sub-index itself for nested pins (edge writes target it).
        EdgeKind::Pin => serde_json::json!({
            "from": e.from.to_string(), "to": e.to.to_string(), "kind": "pin",
            "index_id": e.index_id,
        }),
    }
}

/// The nested sub-index tree the index panel shows as indented sub-headings.
/// Sub-indexes are not graph nodes; their pins ride the top-level index's
/// edges with an `index_id` attribution.
fn sub_indexes(children: &[Index]) -> Vec<serde_json::Value> {
    children
        .iter()
        .map(|c| {
            serde_json::json!({
                "id": c.id,
                "title": c.title,
                "pinned": c.pinned,
                "related_editable": c.related_editable,
                "children": sub_indexes(&c.children),
            })
        })
        .collect()
}

/// The training view's syllabus, shaped as an index node so the index panel
/// can show and reorder it.
///
/// A course has no `index` blocks — its structure IS the data: `module`s and
/// `lesson`s ordered by `n`. Without this the panel is empty for a training
/// view. One top-level node ("Course") carries the ungrouped lessons as pins
/// and each module as a sub-level, mirroring the index / sub-index tree.
///
/// `syllabus: true` marks the levels as ordered-by-`n` rather than pinned by
/// a `related` list: reordering rewrites the lessons' `n`, and there is
/// nothing to pin or unpin — a lesson belongs to the course by existing.
/// Emitted after layout with zero geometry, since index nodes never render
/// on the canvas.
fn syllabus_node(
    model: &wcl_wskill::Graph,
    views: &[QueryView],
    ws: &Workspace,
    entry_abs: &Path,
) -> Vec<serde_json::Value> {
    let Some(course) = model.course.as_ref() else {
        return Vec::new();
    };
    // Which projection a course belongs to is the library's fact, read off
    // the kind whose data builds it.
    let owner = wcl_wskill::structural_view_kind("lesson");
    let in_course = |v: &QueryView| owner.is_some() && v.kind() == owner;
    if !views.iter().any(in_course) {
        return Vec::new();
    }
    let Course { lessons, modules } = course;
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
    vec![
        GraphNode {
            node_type: "index",
            id: SYLLABUS_ID,
            kind: "index",
            title: "Course",
            // Synthesized from the lesson data — no block, so no prose.
            file: ws
                .rel(entry_abs)
                .unwrap_or_else(|_| entry_abs.display().to_string()),
            views: map_over(views, in_course),
            pinned: lessons.clone(),
            children,
            syllabus: true,
            ..GraphNode::default()
        }
        .json((0.0, 0.0), (0.0, 0.0)),
    ]
}

/// `{ site: <pred> }` over the queried views.
fn map_over(views: &[QueryView], pred: impl Fn(&QueryView) -> bool) -> serde_json::Value {
    serde_json::Value::Object(
        views
            .iter()
            .map(|v| (v.site.clone(), pred(v).into()))
            .collect(),
    )
}

fn visibility_json(v: &Visibility) -> serde_json::Value {
    serde_json::json!({ "except_sites": v.except_sites, "custom": v.custom })
}

/// How much of a block's label the list shows. The model carries the whole
/// label — how much of it fits in a row is the reader's business.
const PREVIEW_CHARS: usize = 60;

fn blocks_json(
    ws: &Workspace,
    root: &Path,
    blocks: &[ContentBlock],
    sites: &[&str],
) -> Vec<serde_json::Value> {
    blocks
        .iter()
        .map(|b| {
            let views: serde_json::Map<String, serde_json::Value> = sites
                .iter()
                .map(|s| ((*s).to_string(), b.visibility.shows_in(s).into()))
                .collect();
            let preview: String = b.preview.chars().take(PREVIEW_CHARS).collect();
            serde_json::json!({
                "kind": b.kind,
                "preview": preview,
                "file": rel(ws, root, &b.anchor),
                "span": super::span_json(b.anchor.span),
                "views": serde_json::Value::Object(views),
                "visibility": visibility_json(&b.visibility),
            })
        })
        .collect()
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
        assert_eq!(alpha["file"], "data/concepts/alpha.wcl");
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
        assert_eq!(shown["file"], "data/concepts/alpha.wcl");
    }

    /// The model anchors every node relative to the **wskill root**, which
    /// is a sub-directory of the served tree for any wskill living inside a
    /// larger repo. Every file the payload names is what the write endpoints
    /// take: repo-relative to the served directory.
    #[test]
    fn files_are_repo_relative_even_when_the_wskill_is_nested() {
        let (_td, ws) = workspace_built_by(|root| {
            let skill = root.join("docs/skill");
            std::fs::create_dir_all(&skill).unwrap();
            write_mini_wskill(&skill);
            // The marker is what makes `docs/skill` the wskill root, and so
            // what the model's anchors become relative to.
            std::fs::write(
                skill.join(wcl_wskill::ROOT_MARKER),
                "topic mini {\n  name = \"Mini\"\n}\n",
            )
            .unwrap();
        });

        let v = graph(&ws, "docs/skill/main.wcl", "book", "book=book").expect("graph");
        assert_eq!(
            node(&v, "alpha")["file"],
            "docs/skill/data/concepts/alpha.wcl"
        );
        assert_eq!(node(&v, "lang")["file"], "docs/skill/data/indexes.wcl");
        // And a nav op addressed by id lands on the right file.
        crate::editor::nav::nav_op(
            &ws,
            &Sessions::default(),
            &serde_json::json!({
                "entry": "docs/skill/main.wcl", "op": "unpin_unit",
                "index_id": "lang", "unit_id": "alpha",
            }),
        )
        .expect("unpin");
        let text =
            std::fs::read_to_string(ws.root_dir().join("docs/skill/data/indexes.wcl")).unwrap();
        assert!(text.contains("related = [beta]"), "{text}");
    }

    /// Every node — a unit, an index, the synthetic syllabus — answers with
    /// the same key set, because they are all built through one shape. A key
    /// present on one and missing from another reads as `undefined` client
    /// side, which is why this is asserted rather than left to review.
    #[test]
    fn every_node_carries_the_same_keys() {
        let (_td, ws) = workspace_built_by(write_mini_wskill_training);
        let v = model(&ws, "book,course", "book=book,course=training");
        let nodes = v["nodes"].as_array().unwrap();
        let keys = |n: &serde_json::Value| {
            let mut k: Vec<String> = n.as_object().unwrap().keys().cloned().collect();
            k.sort();
            k
        };
        let expected = keys(&nodes[0]);
        assert!(expected.contains(&"syllabus".to_string()), "{expected:?}");
        for n in nodes {
            assert_eq!(keys(n), expected, "node {} differs: {n:#}", n["key"]);
        }
        // Including the one with no block behind it at all.
        assert!(nodes.iter().any(|n| n["syllabus"] == true));
    }

    /// Layout is seeded in DECLARATION order — where each node is written,
    /// not which of the model's two lists it came from. An index declared
    /// between two units sits between them.
    #[test]
    fn nodes_are_ordered_by_where_they_are_declared() {
        let (_td, ws) = workspace_built_by(|root| {
            write_mini_wskill(root);
            // One file declaring a unit, then an index, then a unit.
            std::fs::write(
                root.join("data/indexes.wcl"),
                "concept early {\n  name = \"Early\"\n}\n\n\
                 index lang {\n  name = \"Language\"\n  related = [alpha]\n}\n\n\
                 concept late {\n  name = \"Late\"\n}\n",
            )
            .unwrap();
        });
        let v = model(&ws, "book", "");
        let order: Vec<&str> = v["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|n| n["file"] == "data/indexes.wcl")
            .map(|n| n["id"].as_str().unwrap())
            .collect();
        assert_eq!(order, ["early", "lang", "late"], "{v:#}");
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
