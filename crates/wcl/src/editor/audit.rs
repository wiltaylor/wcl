//! The Design-mode audit view — an **adapter** over [`wcl_wskill::Audit`],
//! exactly as [`graph`](super::graph) is over `wcl_wskill::Graph`.
//!
//! `GET /api/audit?entry=…&range=…` — `entry` names the wskill (its root
//! folder, its `wskill.wcl`, or any projection entry inside it) as the
//! working tree names it, and `range` is a git range in the library's own
//! spelling (`a`, `a..b`, `a...b`; default [`wcl_wskill::DEFAULT_RANGE`]).
//!
//! Nothing here diffs, lints or measures. The library says what changed,
//! what is wrong with what changed, and how the two revisions' health
//! compares; this module **places** the union graph
//! ([`wcl_wdoc::layout_graph`]), re-relativizes the model's wskill-root
//! anchors onto the served tree, and serialises.
//!
//! Two things the wire shape says that the model leaves implicit, both so
//! the client makes no decision the library has already made:
//!
//! - **`graphed`** — whether a node is drawn on the audit graph. Index
//!   nodes are navigation machinery, as in the live graph, so the union
//!   graph draws units; an index's churn is a changelog row.
//! - **`writer`** on every edge — the node whose file a link was written
//!   in, which for a nested pin is the sub-index holding it rather than the
//!   top-level index the edge is drawn from ([`EdgeDelta::writer`]). The
//!   changelog groups link churn by it.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::State;
use axum::http::Uri;
use axum::response::Response;

use wcl_wskill::audit::{Audit, EdgeDelta, Metric, NodeDelta, Range};
use wcl_wskill::lint::Finding;

use super::{EditorState, Workspace, run_blocking};
use crate::serve::query_param;

pub(super) async fn handle_audit(State(state): State<Arc<EditorState>>, uri: Uri) -> Response {
    let entry = query_param(&uri, "entry");
    let range = query_param(&uri, "range").unwrap_or_default();
    let state2 = Arc::clone(&state);
    run_blocking(move || {
        let entry = entry.ok_or("missing entry")?;
        audit(&state2.ws, &entry, &range)
    })
    .await
}

/// The kind an index node carries — what tells a navigation node from a
/// content one, read off the identity every node already has.
const INDEX_KIND: &str = "index";

fn audit(ws: &Workspace, entry: &str, range_spec: &str) -> Result<serde_json::Value, String> {
    let entry_abs = ws.abs(entry)?;
    let range = Range::parse(range_spec);
    let audit = Audit::across(&entry_abs, &range).map_err(super::err_str)?;
    Ok(audit_json(ws, &audit))
}

/// The audit as the client reads it. Split from [`audit`] so it can be
/// tested against two hand-built revisions of a fixture — the same seam the
/// library's own tests use ([`Audit::of`]), and the reason none of this
/// needs a git repo to assert.
pub(super) fn audit_json(ws: &Workspace, audit: &Audit) -> serde_json::Value {
    let placed: Vec<&NodeDelta> = audit.nodes.iter().collect();
    let sizes: Vec<(f64, f64)> = placed.iter().map(|n| box_for(&n.title)).collect();
    let slot: HashMap<String, usize> = placed
        .iter()
        .enumerate()
        .map(|(i, n)| (n.node.to_string(), i))
        .collect();
    // Laid out over units AND indexes together, as the live graph is: an
    // index pulls the units it pins toward itself even though only the
    // units are drawn.
    let layout_edges: Vec<(usize, usize)> = audit
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

    let nodes: Vec<serde_json::Value> = placed
        .iter()
        .enumerate()
        .map(|(i, n)| node_json(ws, audit, n, sizes[i], offsets[i]))
        .collect();

    serde_json::json!({
        "ok": true,
        // The two RESOLVED ends, never the spec: `HEAD~1` names a different
        // commit tomorrow, and a reader who cannot reproduce the audit
        // cannot check it. `after: null` is the working tree.
        "range": { "before": audit.before, "after": audit.after },
        "root": ws.rel(&audit.root).unwrap_or_else(|_| audit.root.display().to_string()),
        "entry": audit.entry.display().to_string(),
        "summary": audit.summary,
        "health": audit.health.iter().map(metric_json).collect::<Vec<_>>(),
        "nodes": nodes,
        "edges": audit.edges.iter().map(edge_json).collect::<Vec<_>>(),
    })
}

/// A node's box, sized to fit its title — the same sizing the live graph
/// uses, so a unit is the same width on both surfaces.
fn box_for(title: &str) -> (f64, f64) {
    (
        (title.chars().count() as f64 * 7.5 + 30.0).clamp(90.0, 260.0),
        48.0,
    )
}

fn node_json(
    ws: &Workspace,
    audit: &Audit,
    n: &NodeDelta,
    (w, h): (f64, f64),
    (x, y): (f64, f64),
) -> serde_json::Value {
    let index = n.node.kind == INDEX_KIND;
    serde_json::json!({
        "key": n.node.to_string(),
        "type": if index { "index" } else { "unit" },
        "id": n.node.id,
        "kind": n.node.kind,
        "title": n.title,
        "change": n.change,
        "changed": n.changed,
        "file": rel_file(ws, audit, &n.file),
        "span": n.span.map(super::span_json),
        "findings": n.findings.iter().map(finding_json).collect::<Vec<_>>(),
        // The union graph draws content, not navigation — see the module
        // docs. An index is still a changelog row.
        "graphed": !index,
        "news": n.is_news(),
        "x": x, "y": y, "w": w, "h": h,
    })
}

/// A finding as a row tag: what fired, how certain it is, and what it says.
/// The node it is about is the row it rides, so it is not repeated.
fn finding_json(f: &Finding) -> serde_json::Value {
    serde_json::json!({
        "severity": f.severity.as_str(),
        "rule": f.rule.slug(),
        "message": f.message,
    })
}

fn edge_json(e: &EdgeDelta) -> serde_json::Value {
    serde_json::json!({
        "from": e.from.to_string(),
        "to": e.to.to_string(),
        "kind": e.kind.as_str(),
        "index_id": e.index_id,
        // Where the link is WRITTEN, which for a nested pin is not where it
        // is drawn from.
        "writer": e.writer().to_string(),
        "change": e.change,
    })
}

/// Both ends of a metric, pre-formatted the way the metric's own kind reads
/// — two decimals for a ratio, none for a count. Formatting is the
/// library's ([`Metric::before_text`]) so the header strip and the CLI
/// cannot render one number two ways.
fn metric_json(m: &Metric) -> serde_json::Value {
    serde_json::json!({
        "key": m.key,
        "label": m.label,
        "before": m.before_text(),
        "after": m.after_text(),
        "worse": m.worse,
        "moved": m.moved(),
    })
}

/// A model path (relative to the wskill root) as the repo-relative path
/// every editor response speaks.
fn rel_file(ws: &Workspace, audit: &Audit, file: &std::path::Path) -> String {
    let abs = audit.root.join(file);
    ws.rel(&abs).unwrap_or_else(|_| abs.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::testsupport::{workspace_built_by, write_mini_wskill};
    use std::path::Path;

    /// Read the fixture, apply `edit`, read it again — the two revisions an
    /// audit compares, with no git repo in the way.
    fn audit_of(ws: &Workspace, edit: impl FnOnce(&Path)) -> serde_json::Value {
        let root = ws.root_dir().to_path_buf();
        let entry = root.join("main.wcl");
        let before = wcl_wskill::Graph::open(&entry).expect("before");
        edit(&root);
        let after = wcl_wskill::Graph::open(&entry).expect("after");
        audit_json(ws, &Audit::of(&before, &after))
    }

    fn node<'a>(v: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
        v["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == id)
            .unwrap_or_else(|| panic!("no node `{id}`: {v:#}"))
    }

    /// The union graph: a removed unit is still a node, marked removed and
    /// placed — the one thing the live graph structurally cannot show.
    #[test]
    fn a_removed_unit_is_a_placed_node_marked_removed() {
        let (_td, ws) = workspace_built_by(write_mini_wskill);
        let v = audit_of(&ws, |root| {
            std::fs::write(root.join("data/concepts/beta.wcl"), "").unwrap();
            std::fs::write(
                root.join("data/indexes.wcl"),
                "index lang {\n  name = \"Language\"\n  related = [alpha]\n}\n",
            )
            .unwrap();
        });

        let beta = node(&v, "beta");
        assert_eq!(beta["change"], "removed");
        assert_eq!(beta["type"], "unit");
        assert_eq!(beta["graphed"], true);
        assert!(beta["x"].is_number() && beta["w"].is_number(), "{beta:#}");
        // Named where it WAS written, repo-relative like every other path.
        assert_eq!(beta["file"], "data/concepts/beta.wcl");
        assert_eq!(v["summary"]["units"]["removed"], 1);

        // Its pin edge survives as a removal, attributed to the index that
        // wrote it.
        let pin = v["edges"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["to"] == "concept:beta" && e["kind"] == "pin")
            .unwrap_or_else(|| panic!("no pin edge: {v:#}"));
        assert_eq!(pin["change"], "removed");
        assert_eq!(pin["writer"], "index:lang");
    }

    /// Findings ride the changed rows, keeping their severity — a candidate
    /// must never reach the client wearing an error's badge.
    #[test]
    fn an_added_unit_carries_its_findings_with_their_severity() {
        let (_td, ws) = workspace_built_by(write_mini_wskill);
        let v = audit_of(&ws, |root| {
            std::fs::write(
                root.join("data/concepts/main.wcl"),
                "import \"./alpha.wcl\"\nimport \"./beta.wcl\"\nimport \"./gamma.wcl\"\n",
            )
            .unwrap();
            std::fs::write(
                root.join("data/concepts/gamma.wcl"),
                "concept gamma {\n  name = \"Gamma\"\n}\n",
            )
            .unwrap();
        });

        let gamma = node(&v, "gamma");
        assert_eq!(gamma["change"], "added");
        assert_eq!(gamma["news"], true);
        let findings = gamma["findings"].as_array().unwrap();
        let unindexed = findings
            .iter()
            .find(|f| f["rule"] == "unindexed")
            .unwrap_or_else(|| panic!("no unindexed finding: {gamma:#}"));
        assert_eq!(unindexed["severity"], "warn");
        assert!(unindexed["message"].as_str().unwrap().contains("index"));

        // An untouched, unchanged unit is in the model but not news.
        let alpha = node(&v, "alpha");
        assert_eq!(alpha["change"], "unchanged");
        assert_eq!(alpha["news"], false);
        assert_eq!(alpha["findings"], serde_json::json!([]));
    }

    /// A modified node names which aspects moved, and the health strip
    /// carries both ends pre-formatted.
    #[test]
    fn a_modified_node_names_its_aspects_and_health_pairs_both_ends() {
        let (_td, ws) = workspace_built_by(write_mini_wskill);
        let v = audit_of(&ws, |root| {
            std::fs::write(
                root.join("data/concepts/alpha.wcl"),
                "concept alpha {\n  name = \"Alpha, renamed\"\n}\n",
            )
            .unwrap();
        });
        let alpha = node(&v, "alpha");
        assert_eq!(alpha["change"], "modified");
        assert_eq!(alpha["changed"], serde_json::json!(["title"]));

        let health = v["health"].as_array().unwrap();
        assert_eq!(health.len(), 8);
        let links = health
            .iter()
            .find(|m| m["key"] == "edges_per_unit")
            .unwrap();
        // Formatted by the library, so the header and the CLI agree.
        assert_eq!(links["before"], "0.00");
        assert_eq!(links["after"], "0.00");
        assert_eq!(links["worse"], false);
        assert!(health.iter().all(|m| m["label"].is_string()));
    }

    /// Index nodes are navigation, not content: they are changelog rows and
    /// never drawn on the union graph.
    #[test]
    fn an_index_is_a_row_and_not_a_graph_node() {
        let (_td, ws) = workspace_built_by(write_mini_wskill);
        let v = audit_of(&ws, |root| {
            std::fs::write(
                root.join("data/indexes.wcl"),
                "index lang {\n  name = \"Language\"\n  related = [beta, alpha]\n}\n",
            )
            .unwrap();
        });
        let lang = node(&v, "lang");
        assert_eq!(lang["type"], "index");
        assert_eq!(lang["graphed"], false);
        assert_eq!(lang["change"], "modified");
        assert_eq!(lang["changed"], serde_json::json!(["related"]));
    }

    /// Every path the payload names is repo-relative to the served tree,
    /// even when the wskill lives in a sub-directory of it.
    #[test]
    fn files_are_repo_relative_even_when_the_wskill_is_nested() {
        let (_td, ws) = workspace_built_by(|root| {
            let skill = root.join("docs/skill");
            std::fs::create_dir_all(&skill).unwrap();
            write_mini_wskill(&skill);
            std::fs::write(
                skill.join(wcl_wskill::ROOT_MARKER),
                "topic mini {\n  name = \"Mini\"\n}\n",
            )
            .unwrap();
        });
        let entry = ws.root_dir().join("docs/skill/main.wcl");
        let before = wcl_wskill::Graph::open(&entry).expect("before");
        std::fs::write(
            ws.root_dir().join("docs/skill/data/concepts/alpha.wcl"),
            "concept alpha {\n  name = \"Alpha!\"\n}\n",
        )
        .unwrap();
        let after = wcl_wskill::Graph::open(&entry).expect("after");
        let v = audit_json(&ws, &Audit::of(&before, &after));

        assert_eq!(v["root"], "docs/skill");
        assert_eq!(
            node(&v, "alpha")["file"],
            "docs/skill/data/concepts/alpha.wcl"
        );
    }
}
