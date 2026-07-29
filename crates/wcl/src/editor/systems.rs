//! The Design-mode Systems view: a WAD's C4 containment forest as data.
//!
//! `GET /api/systems?entry=…&page_file=…` answers every gathered data object
//! of the document with the *parent* it hangs off, every relation edge
//! between them, and the schema metadata the canvas needs to draw and edit
//! them. Nothing about the WAD is hardcoded here — containment is derived
//! from the schema:
//!
//! - a **parent link** is a scalar `identifier` field whose NAME is another
//!   gathered kind's name (`component.container`, `container.system`,
//!   `system.boundary`, `screen.component`), plus `parent` for self-nesting
//!   (`infra_node.parent`). Declaration order is the preference order, and an
//!   instance's real parent is the first such field actually set on it — so
//!   `code_item`, which may name either a `component` or a `container`, lands
//!   under whichever it declares.
//! - a **reference** is any other `identifier` / `list<identifier>` field
//!   (`repo`, `built_by`, `supersedes`, `nav_to`); the client draws them as
//!   optional dashed edges.
//! - an **edge kind** is a gathered kind carrying both a `source` and a
//!   `destination` identifier field — `relation` in the base schema, and any
//!   extension following the same convention.
//!
//! So a kind added to `schema/base.wcl` or `schema/extensions.wcl` (and any
//! symbol added to a `kinds.wcl` vocabulary, which rides along in the shared
//! [`super::blocks::kind_entry`] field metadata) shows up in the view without
//! a change here. Writes go through the existing `/api/block/ops` and
//! `/api/unit/create` — this endpoint is read-only.
//!
//! The one curated thing is the **perspective** list (see [`PERSPECTIVES`]):
//! which slice of the model each canvas tab opens on, so the C4 drill-down
//! isn't cluttered with the people who use the system or the machines it
//! runs on.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::State;
use axum::http::Uri;
use axum::response::Response;

use wcl_lang::ast::{self, Item};
use wcl_lang::{DeclName, parse_for_edit};

use super::blocks::{
    KindInfo, bare_type, first_label, is_scalar, kind_entry, kind_links, value_string,
};
use super::data::classify_cell;
use super::{EditorState, resolve_doc_entry, run_blocking};
use crate::serve::query_param;

/// Fields tried in order for a node's display title.
const TITLE_FIELDS: &[&str] = &["name", "title", "term", "version", "activity", "path"];

/// The canvas tabs: `(id, label, seed kinds)`. A perspective is its seeds
/// plus everything that nests below them, so it follows the schema down as
/// the model grows — only the entry points are named here, and they are the
/// canonical WAD vocabulary (the same way the graph view names the wskill
/// unit kinds it routes). Seeds are EXCLUSIVE: a kind seeded by one
/// perspective is never pulled into another through containment, which is
/// what keeps `deploy_target` out of the C4 drill-down even though it names
/// a container.
///
/// A perspective whose seeds the document doesn't declare is not offered;
/// kinds no perspective claims (ADRs, specs, glossary terms) stay one
/// checkbox away in the panel, and "All" opens on everything.
const PERSPECTIVES: &[(&str, &str, &[&str])] = &[
    (
        "systems",
        "Systems",
        &["boundary", "system", "external_system"],
    ),
    ("personas", "Personas", &["persona"]),
    (
        "deployment",
        "Deployment",
        &["environment", "infra_node", "deploy_target"],
    ),
];

/// The perspectives this document can offer: each one's seeds (those it
/// actually declares) plus their containment closure, stopping at another
/// perspective's seed. Always ends with an "All" perspective over every
/// non-edge kind.
fn perspectives(infos: &[KindInfo]) -> Vec<serde_json::Value> {
    let declared = |k: &str| infos.iter().any(|i| i.kind == k && i.edge.is_none());
    let all_seeds: Vec<&str> = PERSPECTIVES
        .iter()
        .flat_map(|(_, _, s)| *s)
        .copied()
        .collect();

    let mut out: Vec<serde_json::Value> = Vec::new();
    for (id, label, seeds) in PERSPECTIVES {
        let present: Vec<&str> = seeds.iter().copied().filter(|k| declared(k)).collect();
        if present.is_empty() {
            continue;
        }
        let mut kinds: Vec<String> = present.iter().map(|k| k.to_string()).collect();
        let mut frontier = kinds.clone();
        while let Some(parent) = frontier.pop() {
            for info in infos {
                if info.edge.is_some()
                    || kinds.contains(&info.kind)
                    // Another perspective owns this kind outright.
                    || (all_seeds.contains(&info.kind.as_str()) && !present.contains(&info.kind.as_str()))
                    || info.kind == parent
                    || !info.parents.iter().any(|(_, p)| *p == parent)
                {
                    continue;
                }
                kinds.push(info.kind.clone());
                frontier.push(info.kind.clone());
            }
        }
        out.push(serde_json::json!({ "id": id, "label": label, "kinds": kinds }));
    }
    let every: Vec<&str> = infos
        .iter()
        .filter(|i| i.edge.is_none())
        .map(|i| i.kind.as_str())
        .collect();
    out.push(serde_json::json!({ "id": "all", "label": "All", "kinds": every }));
    out
}

pub(super) async fn handle_systems(State(state): State<Arc<EditorState>>, uri: Uri) -> Response {
    let entry = query_param(&uri, "entry");
    let page_file = query_param(&uri, "page_file");
    let state2 = Arc::clone(&state);
    run_blocking(move || {
        let entry = entry.ok_or("missing entry")?;
        systems(&state2, &entry, page_file.as_deref())
    })
    .await
}

/// `POST /api/systems/detail` — everything about one object, for the
/// details modal: its own properties, each family of child blocks its
/// schema declares (with the metadata to render a form per child,
/// recursively — an `api_endpoint`'s params and responses ride inside the
/// `code_item` payload), the prose body, and the relations that touch it.
///
/// Body: `{ entry, page_file?, file, span }`. The canvas already knows the
/// anchor, and addressing by span keeps this on the same footing as every
/// mutation — the modal refetches after each commit, when spans have moved.
pub(super) async fn handle_systems_detail(
    State(state): State<Arc<EditorState>>,
    body: String,
) -> Response {
    let v = match crate::serve::parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return crate::serve::json_error(axum::http::StatusCode::BAD_REQUEST, &e),
    };
    let state2 = Arc::clone(&state);
    run_blocking(move || systems_detail(&state2, &v)).await
}

fn systems_detail(state: &EditorState, v: &serde_json::Value) -> Result<serde_json::Value, String> {
    let doc_entry = super::resolve_doc_entry_from(state, v)?;
    let file_rel = crate::edit::str_field(v, "file")?;
    let file = crate::serve::sandboxed(&state.root_dir, &state.root_dir.join(file_rel))
        .ok_or_else(|| format!("file outside the served tree: {file_rel}"))?;
    let span = super::blocks::span_field(v, "span")?;
    let text = crate::edit::read(&file)?;
    let src = parse_for_edit(&text, file.display().to_string()).map_err(super::err_str)?;
    let blk = super::find_block_at(&src.items, span)
        .ok_or("no block at that span — the file changed; reopen the object")?;
    let slice = |s: wcl_lang::Span| text.get(s.start..s.end).unwrap_or_default().to_string();

    let doc = wcl_wdoc::open_doc_for_edit(&doc_entry).map_err(super::err_str)?;
    let links = kind_links(&doc);
    let info = links.iter().find(|i| i.kind == blk.kind);
    let id = super::blocks::ast_label(blk);

    // One entry per `@child` / `@children` family the schema declares, with
    // the instances present in this block — recursively, so the modal can
    // edit two levels down without another fetch. The prose body is edited
    // as source, not as a form; `block_kinds` hints at what it holds (a
    // wireframe diagram, a terminal mock-up) without evaluating anything.
    let (children, body_blk) = match info {
        Some(info) => (
            child_families(&doc, &info.schema, blk, &text, MAX_CHILD_DEPTH),
            body_block(&info.schema, blk),
        ),
        None => (Vec::new(), None),
    };
    let body = match body_blk {
        Some(b) => serde_json::json!({
            "kind": b.kind,
            "span": super::span_json(b.span),
            "source": slice(b.span),
            "block_kinds": body_block_kinds(b),
        }),
        None => serde_json::Value::Null,
    };

    // Relations with this object at either end, so a modal can show (and
    // drop) the wiring without hunting for it on the canvas.
    let mut relations: Vec<serde_json::Value> = Vec::new();
    if let Some(id) = &id {
        let title_of = |other: &str| {
            doc.blocks()
                .find(|b| {
                    links.iter().any(|i| i.kind == b.kind() && i.edge.is_none())
                        && first_label(b).as_deref() == Some(other)
                })
                .and_then(|b| TITLE_FIELDS.iter().find_map(|f| field_string(&b, f)))
        };
        for (path, b) in doc.blocks_with_source() {
            let Some(edge) = links
                .iter()
                .find(|i| i.kind == b.kind())
                .and_then(|i| i.edge.as_ref())
            else {
                continue;
            };
            let from = field_string(&b, &edge.0);
            let to = field_string(&b, &edge.1);
            let direction = match (from.as_deref(), to.as_deref()) {
                (Some(f), _) if f == id => "out",
                (_, Some(t)) if t == id => "in",
                _ => continue,
            };
            let other = if direction == "out" { to } else { from };
            let efile = path.map(Path::to_path_buf).unwrap_or_else(|| file.clone());
            relations.push(serde_json::json!({
                "kind": b.kind(),
                "id": first_label(&b),
                "direction": direction,
                "other": other,
                "other_title": other.as_deref().and_then(title_of),
                "label": field_string(&b, "label"),
                "rel_kind": field_string(&b, "kind"),
                "file": rel(state, &efile),
                "span": super::span_json(b.span()),
            }));
        }
    }

    Ok(serde_json::json!({
        "ok": true,
        "kind": blk.kind,
        "id": id,
        "file": rel(state, &file),
        "span": super::span_json(span),
        "etag": crate::edit::content_etag(&text),
        "source": slice(span),
        "cells": cells(blk),
        "schema": info.map(|i| {
            let mut e = kind_entry(&i.kind, &i.schema);
            e["type_name"] = serde_json::json!(i.schema.full_name());
            e["suggestions"] = suggestions(&doc, &i.kind, &i.schema);
            e["parents"] = serde_json::json!(
                i.parents
                    .iter()
                    .map(|(field, kind)| serde_json::json!({ "field": field, "kind": kind }))
                    .collect::<Vec<_>>()
            );
            e
        }),
        "children": children,
        "body": body,
        "relations": relations,
    }))
}

/// How deep the detail payload follows `@child` / `@children` families. The
/// WAD's deepest chain is three (`code_item` → `db_table` → `db_column`);
/// the guard exists for a pathological self-nesting schema, not for real
/// data.
const MAX_CHILD_DEPTH: usize = 4;

/// The `@child` / `@children` families `schema` declares, with the
/// instances present in `blk` — recursively: each item carries its own
/// `children`, so an `api_endpoint`'s params and responses ride inside the
/// `code_item` payload. Child schemas resolve through the FIELD's own type
/// ([`super::blocks::gather_elem_decl`]), which is namespace-correct where
/// a bare kind lookup is not. `body` families are prose, edited as source
/// at the top level only — they are skipped here.
fn child_families(
    doc: &wcl_lang::Document,
    schema: &wcl_lang::TypeDecl<'_>,
    blk: &ast::Block,
    text: &str,
    depth: usize,
) -> Vec<serde_json::Value> {
    if depth == 0 {
        return Vec::new();
    }
    let slice = |s: wcl_lang::Span| text.get(s.start..s.end).unwrap_or_default().to_string();
    let mut out: Vec<serde_json::Value> = Vec::new();
    for f in schema.effective_fields() {
        let (kind, many) = match (f.child_block_kind(), f.children_block_kind()) {
            (Some(k), _) => (k, false),
            (_, Some(k)) => (k, true),
            _ => continue,
        };
        if kind == "body" {
            continue;
        }
        let items: Vec<&ast::Block> = blk
            .items
            .iter()
            .filter_map(|it| match it {
                Item::Block(b) if b.kind == kind => Some(b),
                _ => None,
            })
            .collect();
        let child_schema = super::blocks::gather_elem_decl(&f).or_else(|| doc.block_schema(&kind));
        let schema_json = child_schema.as_ref().map(|d| {
            let mut e = kind_entry(&kind, d);
            e["suggestions"] = suggestions(doc, &kind, d);
            e
        });
        out.push(serde_json::json!({
            "field": f.name(),
            "kind": kind,
            "many": many,
            "doc": f.doc_comment(),
            "schema": schema_json,
            "items": items
                .iter()
                .map(|b| serde_json::json!({
                    "label": super::blocks::ast_label(b),
                    "span": super::span_json(b.span),
                    "source": slice(b.span),
                    "cells": cells(b),
                    "children": child_schema
                        .as_ref()
                        .map(|d| child_families(doc, d, b, text, depth - 1))
                        .unwrap_or_default(),
                }))
                .collect::<Vec<_>>(),
        }));
    }
    out
}

/// The first `body` block of `blk`, when its schema declares one.
fn body_block<'a>(schema: &wcl_lang::TypeDecl<'_>, blk: &'a ast::Block) -> Option<&'a ast::Block> {
    let declared = schema.effective_fields().into_iter().any(|f| {
        f.child_block_kind().as_deref() == Some("body")
            || f.children_block_kind().as_deref() == Some("body")
    });
    if !declared {
        return None;
    }
    blk.items.iter().find_map(|it| match it {
        Item::Block(b) if b.kind == "body" => Some(b),
        _ => None,
    })
}

/// The block kinds a prose body holds, from a flat AST walk — plus one
/// level inside `diagram` blocks so a wireframe's `wf_*` widgets show up.
/// A cheap content hint (wireframe vs terminal) for the client; nothing is
/// evaluated.
fn body_block_kinds(body: &ast::Block) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |k: &str| {
        if !out.iter().any(|s| s == k) {
            out.push(k.to_string());
        }
    };
    for it in &body.items {
        let Item::Block(b) = it else { continue };
        push(&b.kind);
        if b.kind == "diagram" {
            for nested in &b.items {
                if let Item::Block(c) = nested {
                    push(&c.kind);
                }
            }
        }
    }
    out
}

/// How many distinct values a free-text field may have and still read as a
/// vocabulary worth offering as a list.
const MAX_SUGGESTIONS: usize = 40;

/// The values already in use for each free-text field of `kind`.
///
/// A `utf8` field whose values REPEAT across instances is a taxonomy in
/// practice, even where the schema didn't spell it as a `symbol_set` — a
/// `component`'s `kind` ("module" / "handler" / "store") is the base
/// schema's own example. Offering those values back stops the editor from
/// inventing a new category out of a typo; a field whose values are all
/// distinct (every `name`, every `summary`) suggests nothing.
fn suggestions(
    doc: &wcl_lang::Document,
    kind: &str,
    schema: &wcl_lang::TypeDecl<'_>,
) -> serde_json::Value {
    let blocks: Vec<wcl_lang::Block<'_>> = doc.blocks().filter(|b| b.kind() == kind).collect();
    let mut out = serde_json::Map::new();
    for f in schema.effective_fields() {
        // The inline id names the instance; it is never a shared vocabulary.
        if f.inline_slot().is_some() || !is_scalar(&f) {
            continue;
        }
        let ty = bare_type(&f);
        if ty != "utf8" && ty != "ascii" {
            continue;
        }
        let values: Vec<String> = blocks
            .iter()
            .filter_map(|b| field_string(b, f.name()))
            .collect();
        let mut distinct: Vec<String> = values.clone();
        distinct.sort();
        distinct.dedup();
        if distinct.is_empty() || distinct.len() >= values.len() || distinct.len() > MAX_SUGGESTIONS
        {
            continue;
        }
        out.insert(f.name().to_string(), serde_json::json!(distinct));
    }
    serde_json::Value::Object(out)
}

/// A block's field value as a plain string, when it evaluates to a scalar.
fn field_string(b: &wcl_lang::Block<'_>, name: &str) -> Option<String> {
    b.field(name)
        .and_then(|f| f.value().ok().cloned())
        .as_ref()
        .map(value_string)
        .filter(|s| !s.is_empty())
}

fn systems(
    state: &EditorState,
    entry: &str,
    page_file: Option<&str>,
) -> Result<serde_json::Value, String> {
    let doc_entry = resolve_doc_entry(state, entry, page_file)?;
    let doc = wcl_wdoc::open_doc_for_edit(&doc_entry).map_err(super::err_str)?;

    let infos: Vec<KindInfo> = kind_links(&doc);
    let info_of = |kind: &str| infos.iter().find(|i| i.kind == kind);

    // Per-file AST + text cache: spans and etags come from the parse, keyed
    // by the doc view's blocks.
    let mut files: HashMap<PathBuf, (String, ast::Source)> = HashMap::new();
    let mut nodes: Vec<serde_json::Value> = Vec::new();
    let mut edges: Vec<serde_json::Value> = Vec::new();
    let mut ids: Vec<String> = Vec::new();

    for (path, b) in doc.blocks_with_source() {
        let Some(info) = info_of(b.kind()) else {
            continue;
        };
        let Some(id) = first_label(&b) else { continue };
        let file = path
            .map(Path::to_path_buf)
            .unwrap_or_else(|| doc_entry.clone());
        if !files.contains_key(&file) {
            let text = crate::edit::read(&file)?;
            let src = parse_for_edit(&text, file.display().to_string()).map_err(super::err_str)?;
            files.insert(file.clone(), (text, src));
        }
        let (text, src) = &files[&file];
        let span = b.span();
        let Some(blk) = super::find_block_at(&src.items, span) else {
            continue;
        };
        let rel = rel(state, &file);
        let etag = crate::edit::content_etag(text);

        if let Some((sf, df)) = &info.edge {
            let (Some(from), Some(to)) = (field_string(&b, sf), field_string(&b, df)) else {
                continue;
            };
            edges.push(serde_json::json!({
                "key": format!("{}:{id}", info.kind),
                "kind": info.kind,
                "id": id,
                "from": from,
                "to": to,
                "label": field_string(&b, "label"),
                "rel_kind": field_string(&b, "kind"),
                "file": rel,
                "span": super::span_json(span),
                "etag": etag,
                "cells": cells(blk),
            }));
            continue;
        }

        // EVERY parent field the instance sets, in declaration order. The
        // first is where the node nests by default; the rest let the canvas
        // fall back when that kind isn't on screen — a `deploy_target` shows
        // under its environment in a deployment view, and under its
        // container in the C4 one, from the same data.
        let parents: Vec<serde_json::Value> = info
            .parents
            .iter()
            .filter_map(|(field, pkind)| {
                let pid = field_string(&b, field)?;
                (pid != id).then(|| serde_json::json!({ "field": field, "kind": pkind, "id": pid }))
            })
            .collect();
        ids.push(id.clone());
        nodes.push(serde_json::json!({
            "key": format!("{}:{id}", info.kind),
            "kind": info.kind,
            "id": id,
            "title": TITLE_FIELDS
                .iter()
                .find_map(|f| field_string(&b, f))
                .unwrap_or_else(|| id.clone()),
            "subtitle": field_string(&b, "kind"),
            "summary": field_string(&b, "summary"),
            "parent": parents.first(),
            "parents": parents,
            "file": rel,
            "span": super::span_json(span),
            "etag": etag,
            "cells": cells(blk),
        }));
    }

    let kinds: Vec<serde_json::Value> = infos
        .iter()
        .map(|i| {
            let mut entry = kind_entry(&i.kind, &i.schema);
            entry["parents"] = serde_json::json!(
                i.parents
                    .iter()
                    .map(|(field, kind)| serde_json::json!({ "field": field, "kind": kind }))
                    .collect::<Vec<_>>()
            );
            entry["refs"] = serde_json::json!(
                i.refs
                    .iter()
                    .map(|(field, list)| serde_json::json!({ "field": field, "list": list }))
                    .collect::<Vec<_>>()
            );
            entry["edge"] = match &i.edge {
                Some((s, d)) => serde_json::json!({ "source": s, "destination": d }),
                None => serde_json::Value::Null,
            };
            // The fully-qualified schema name, echoed back by the create
            // path so a kind name shared across namespaces (a WAD
            // `container` vs wdoc's diagram shape) resolves unambiguously.
            entry["type_name"] = serde_json::json!(i.schema.full_name());
            entry["suggestions"] = suggestions(&doc, &i.kind, &i.schema);
            entry["id_field"] = serde_json::json!(
                i.schema
                    .effective_fields()
                    .into_iter()
                    .find(|f| f.inline_slot() == Some(0))
                    .map(|f| f.name().to_string())
            );
            entry
        })
        .collect();

    // The WAD model root: the file declaring the `wad` block — an aggregator
    // importing the whole data model but none of the book's templates or
    // pages. The screen editor builds its synthetic unit previews from it:
    // same content, same real-file anchors, at a fraction of the book
    // entry's parse+eval cost (no ~160 template pages to evaluate).
    let model_entry = doc
        .blocks_with_source()
        .find(|(_, b)| b.kind() == "wad")
        .map(|(path, _)| {
            path.map(Path::to_path_buf)
                .unwrap_or_else(|| doc_entry.clone())
        })
        .map(|p| rel(state, &p));

    Ok(serde_json::json!({
        "ok": true,
        "kinds": kinds,
        "perspectives": perspectives(&infos),
        "nodes": nodes,
        "edges": edges,
        "ids": ids,
        "model_entry": model_entry,
    }))
}

/// A block's own fields (and inline labels) classified for the property
/// form — the same literal / scalar / computed split Data mode's rows use.
fn cells(blk: &ast::Block) -> serde_json::Value {
    let mut map: serde_json::Map<String, serde_json::Value> = blk
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Field(f) => Some((f.name.clone(), classify_field(&f.expr))),
            _ => None,
        })
        .collect();
    for (slot, e) in blk.labels.iter().enumerate() {
        let mut cell = classify_cell(e);
        cell["slot"] = serde_json::json!(slot);
        map.insert(format!("@{slot}"), cell);
    }
    serde_json::Value::Object(map)
}

/// [`classify_cell`] plus lists of scalars: `tags = ["a", "b"]` and
/// `repos = [one, two]` report `state: "list"` with their rendered elements,
/// so the details form can edit them instead of sending the user to the
/// source. A list holding anything richer stays `computed`.
fn classify_field(e: &ast::Expr) -> serde_json::Value {
    let ast::Expr::ListLit { elements, .. } = e else {
        return classify_cell(e);
    };
    let items: Option<Vec<serde_json::Value>> = elements
        .iter()
        .map(|el| {
            let cell = classify_cell(el);
            (cell["state"] == "literal").then_some(cell)
        })
        .collect();
    match items {
        Some(items) => serde_json::json!({ "state": "list", "items": items }),
        None => serde_json::json!({ "state": "computed", "text": serde_json::Value::Null }),
    }
}

fn rel(state: &EditorState, file: &Path) -> String {
    std::fs::canonicalize(file)
        .unwrap_or_else(|_| file.to_path_buf())
        .strip_prefix(&state.root_dir)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| file.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A miniature schema exercising every derivation rule: a three-level
    /// containment chain, a two-candidate parent, a self-parent, a plain
    /// reference and an edge kind.
    const SCHEMA: &str = r#"
import <wdoc.wcl>

@block("zone")
type Zone { @inline(0) id: identifier  name: utf8 }

@block("system")
type System { @inline(0) id: identifier  name: utf8  zone: identifier?  repo: identifier? }

// Two-level child nesting (endpoint → param) plus a prose body, to
// exercise the recursive detail payload and the body content hint.
@block("wparam")
type WParam { @inline(0) id: identifier  name: utf8  @default(false) required: bool }

@block("wendpoint")
type WEndpoint {
  @inline(0) id: identifier
  path: utf8
  @children("wparam") params: list<WParam>
}

@block("part")
type Part {
  @inline(0) id: identifier
  name: utf8
  system: identifier?
  zone: identifier?
  tags: list<identifier>
  @children("wendpoint") endpoints: list<WEndpoint>
  @child("body") body: WdocAddressableBody?
}

@block("host")
type Host { @inline(0) id: identifier  name: utf8  parent: identifier? }

@block("link")
type Link { @inline(0) id: identifier  source: identifier  destination: identifier  kind: utf8? }

// The WAD root-metadata kind: its declaring file is served as
// `model_entry`, the cheap entry the screen editor builds previews from.
@block("wad")
type WadMeta { @inline(0) id: identifier  name: utf8 }

// Deployment-perspective seeds, to exercise seed exclusivity: a
// `deploy_target` names a `part` (a systems kind) but must stay out of the
// systems perspective.
@block("environment")
type Env { @inline(0) id: identifier  name: utf8 }

@block("deploy_target")
type Deploy { @inline(0) id: identifier  part: identifier?  environment: identifier? }

@document
type D {
  @children("wad")    wads:    list<WadMeta>
  @children("zone")   zones:   list<Zone>
  @children("system") systems: list<System>
  @children("part")   parts:   list<Part>
  @children("host")   hosts:   list<Host>
  @children("link")   links:   list<Link>
  @children("environment")   envs:    list<Env>
  @children("deploy_target") deploys: list<Deploy>
}
"#;

    fn model(data: &str) -> serde_json::Value {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().canonicalize().expect("canon");
        std::fs::write(root.join("schema.wcl"), SCHEMA).expect("write schema");
        std::fs::write(
            root.join("main.wcl"),
            format!("import \"./schema.wcl\"\n\n{data}"),
        )
        .expect("write main");
        let state = EditorState {
            root_dir: root,
            root_file: None,
            preview: crate::preview::Preview::new().expect("preview"),
            preview_sessions: std::sync::Mutex::new(HashMap::new()),
            review: None,
        };
        let v = systems(&state, "main.wcl", None).expect("systems");
        drop(dir);
        v
    }

    fn kind<'a>(v: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
        v["kinds"]
            .as_array()
            .expect("kinds")
            .iter()
            .find(|k| k["kind"] == name)
            .unwrap_or_else(|| panic!("no kind {name}"))
    }

    fn node<'a>(v: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
        v["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .find(|n| n["id"] == id)
            .unwrap_or_else(|| panic!("no node {id}"))
    }

    #[test]
    fn derives_parent_links_from_field_names() {
        let v = model("zone z { name = \"Z\" }\nsystem s { name = \"S\"  zone = z }\n");
        // `system.zone` names a gathered kind → containment; `system.repo`
        // does not → a plain reference.
        assert_eq!(
            kind(&v, "system")["parents"],
            serde_json::json!([{ "field": "zone", "kind": "zone" }])
        );
        assert_eq!(
            kind(&v, "system")["refs"],
            serde_json::json!([{ "field": "repo", "list": false }])
        );
        // The inline id slot is never a parent link, and list<identifier>
        // fields are references.
        assert_eq!(
            kind(&v, "part")["refs"],
            serde_json::json!([{ "field": "tags", "list": true }])
        );
        assert_eq!(node(&v, "s")["parent"]["id"], "z");
    }

    #[test]
    fn instance_picks_the_first_parent_field_it_sets() {
        let v = model(
            "zone z { name = \"Z\" }\nsystem s { name = \"S\" }\n\
             part a { name = \"A\"  system = s }\npart b { name = \"B\"  zone = z }\n",
        );
        assert_eq!(
            kind(&v, "part")["parents"],
            serde_json::json!([
                { "field": "system", "kind": "system" },
                { "field": "zone", "kind": "zone" },
            ])
        );
        assert_eq!(node(&v, "a")["parent"]["field"], "system");
        assert_eq!(node(&v, "b")["parent"]["field"], "zone");
        assert_eq!(node(&v, "b")["parent"]["id"], "z");
    }

    #[test]
    fn parent_field_self_nests() {
        let v = model("host h { name = \"H\" }\nhost c { name = \"C\"  parent = h }\n");
        assert_eq!(
            kind(&v, "host")["parents"],
            serde_json::json!([{ "field": "parent", "kind": "host" }])
        );
        assert_eq!(node(&v, "c")["parent"]["id"], "h");
        assert!(node(&v, "h")["parent"].is_null());
    }

    #[test]
    fn source_destination_kinds_become_edges() {
        let v = model(
            "system a { name = \"A\" }\nsystem b { name = \"B\" }\n\
             link l { source = a  destination = b  kind = \"calls\" }\n",
        );
        assert_eq!(
            kind(&v, "link")["edge"],
            serde_json::json!({ "source": "source", "destination": "destination" })
        );
        // An edge block is not a node, and its endpoints are wiring rather
        // than containment or references.
        assert_eq!(kind(&v, "link")["parents"], serde_json::json!([]));
        assert_eq!(kind(&v, "link")["refs"], serde_json::json!([]));
        assert_eq!(v["nodes"].as_array().expect("nodes").len(), 2);
        let edges = v["edges"].as_array().expect("edges");
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0]["from"], "a");
        assert_eq!(edges[0]["to"], "b");
        assert_eq!(edges[0]["rel_kind"], "calls");
        assert_eq!(v["ids"], serde_json::json!(["a", "b"]));
    }

    /// A first-of-its-kind object must land in a DATA file, never in the
    /// projection entry that renders it: the entry is a different namespace,
    /// where the block wouldn't even resolve to this schema.
    /// ([`super::super::blocks::place_unit`]'s neighbouring-kind fallback.)
    #[test]
    fn a_kind_with_no_instances_lands_beside_its_neighbours() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().canonicalize().expect("canon");
        std::fs::write(
            root.join("schema.wcl"),
            format!("namespace app\n{}", SCHEMA.replace("import <wdoc.wcl>", "")),
        )
        .expect("write schema");
        std::fs::write(
            root.join("data.wcl"),
            "namespace app\n\nsystem s { name = \"S\" }\npart p { name = \"P\"  system = s }\n",
        )
        .expect("write data");
        // The entry is the projection: it imports the model but declares none
        // of it, and carries no `namespace`.
        std::fs::write(
            root.join("main.wcl"),
            "import \"./schema.wcl\"\nimport \"./data.wcl\"\n",
        )
        .expect("write main");
        let entry = root.join("main.wcl");
        let doc = wcl_wdoc::open_doc_for_edit(&entry).expect("open");

        // `zone` has no instances; `system` (which nests into it) lives in
        // data.wcl, so that is where a new zone belongs.
        match super::super::blocks::place_unit(&doc, &entry, "zone").expect("placement") {
            super::super::blocks::Placement::Append { file } => {
                assert_eq!(file.file_name().and_then(|f| f.to_str()), Some("data.wcl"));
            }
            _ => panic!("expected an append placement"),
        }
    }

    #[test]
    fn a_node_reports_every_parent_field_it_sets() {
        // `part` may name a system OR a zone; setting both must report both,
        // in declaration order, so the canvas can nest it under whichever
        // kind the current perspective shows.
        let v = model(
            "zone z { name = \"Z\" }\nsystem s { name = \"S\" }\n\
             part p { name = \"P\"  system = s  zone = z }\n",
        );
        let n = node(&v, "p");
        assert_eq!(n["parent"]["field"], "system");
        assert_eq!(
            n["parents"],
            serde_json::json!([
                { "field": "system", "kind": "system", "id": "s" },
                { "field": "zone", "kind": "zone", "id": "z" },
            ])
        );
    }

    #[test]
    fn perspectives_close_over_containment_and_stop_at_other_seeds() {
        let v = model("zone z { name = \"Z\" }\n");
        let of = |id: &str| -> Vec<String> {
            v["perspectives"]
                .as_array()
                .expect("perspectives")
                .iter()
                .find(|p| p["id"] == id)
                .unwrap_or_else(|| panic!("no perspective {id}"))["kinds"]
                .as_array()
                .expect("kinds")
                .iter()
                .map(|k| k.as_str().expect("kind").to_string())
                .collect()
        };
        let ids: Vec<&str> = v["perspectives"]
            .as_array()
            .expect("perspectives")
            .iter()
            .map(|p| p["id"].as_str().expect("id"))
            .collect();
        // `personas` isn't offered — the document declares no persona kind.
        assert_eq!(ids, ["systems", "deployment", "all"]);

        let systems = of("systems");
        // Closure runs DOWNWARD from the seed: `part` nests in a system,
        // `zone` merely contains one.
        assert!(systems.contains(&"part".to_string()));
        assert!(!systems.contains(&"zone".to_string()));
        // Seed exclusivity: `deploy_target` names a `part`, but the
        // deployment perspective owns it.
        assert!(!systems.contains(&"deploy_target".to_string()));
        assert_eq!(of("deployment"), ["environment", "deploy_target"]);

        let all = of("all");
        assert!(all.contains(&"zone".to_string()));
        assert!(
            !all.contains(&"link".to_string()),
            "edge kinds are not nodes"
        );
    }

    #[test]
    fn detail_reports_children_body_and_relations() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().canonicalize().expect("canon");
        std::fs::write(root.join("schema.wcl"), SCHEMA).expect("write schema");
        let data = "system a { name = \"A\"  tags = [\"x\", \"y\"] }\n\
                    system b { name = \"B\" }\n\
                    link l { source = a  destination = b  kind = \"calls\" }\n";
        std::fs::write(
            root.join("main.wcl"),
            format!("import \"./schema.wcl\"\n\n{data}"),
        )
        .expect("write main");
        let state = EditorState {
            root_dir: root.clone(),
            root_file: None,
            preview: crate::preview::Preview::new().expect("preview"),
            preview_sessions: std::sync::Mutex::new(HashMap::new()),
            review: None,
        };
        let model = systems(&state, "main.wcl", None).expect("systems");
        let a = node(&model, "a");
        let v = systems_detail(
            &state,
            &serde_json::json!({
                "entry": "main.wcl",
                "file": a["file"],
                "span": a["span"],
            }),
        )
        .expect("detail");

        assert_eq!(v["kind"], "system");
        assert_eq!(v["id"], "a");
        assert!(
            v["source"]
                .as_str()
                .expect("source")
                .starts_with("system a"),
            "the block's own WCL comes back for the source editor"
        );
        // A list of literals is form-editable, not "computed".
        assert_eq!(v["cells"]["tags"]["state"], "list");
        assert_eq!(v["cells"]["tags"]["items"][1]["text"], "y");
        // The schema's form metadata rides along.
        assert_eq!(v["schema"]["kind"], "system");
        // Both ends of the edge are reported, from this object's viewpoint.
        let rels = v["relations"].as_array().expect("relations");
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0]["direction"], "out");
        assert_eq!(rels[0]["other"], "b");
        assert_eq!(rels[0]["other_title"], "B");
        drop(dir);
    }

    /// The detail payload follows child families down: an endpoint's params
    /// ride inside the part's payload with their own schema/cells/spans, and
    /// the body reports the block kinds it holds (one level into `diagram`,
    /// so a wireframe's widgets show up).
    #[test]
    fn detail_reports_nested_children_and_body_kinds() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().canonicalize().expect("canon");
        std::fs::write(root.join("schema.wcl"), SCHEMA).expect("write schema");
        let data = "part p {\n\
                    \x20 name = \"P\"\n\
                    \x20 wendpoint list_users {\n\
                    \x20   path = \"/users\"\n\
                    \x20   wparam limit { name = \"limit\" }\n\
                    \x20 }\n\
                    \x20 body {\n\
                    \x20   diagram { wf_button ok { } }\n\
                    \x20   terminal { }\n\
                    \x20 }\n\
                    }\n";
        std::fs::write(
            root.join("main.wcl"),
            format!("import \"./schema.wcl\"\n\n{data}"),
        )
        .expect("write main");
        let state = EditorState {
            root_dir: root,
            root_file: None,
            preview: crate::preview::Preview::new().expect("preview"),
            preview_sessions: std::sync::Mutex::new(HashMap::new()),
            review: None,
        };
        let model = systems(&state, "main.wcl", None).expect("systems");
        let p = node(&model, "p");
        let v = systems_detail(
            &state,
            &serde_json::json!({
                "entry": "main.wcl",
                "file": p["file"],
                "span": p["span"],
            }),
        )
        .expect("detail");

        let families = v["children"].as_array().expect("children");
        let endpoints = families
            .iter()
            .find(|f| f["kind"] == "wendpoint")
            .expect("endpoint family");
        let ep = &endpoints["items"][0];
        assert_eq!(ep["label"], "list_users");
        assert_eq!(ep["cells"]["path"]["text"], "/users");
        // The nested family, with its own schema metadata and anchored items.
        let nested = ep["children"].as_array().expect("nested families");
        let params = nested
            .iter()
            .find(|f| f["kind"] == "wparam")
            .expect("param family");
        assert_eq!(params["schema"]["kind"], "wparam");
        assert_eq!(params["items"][0]["label"], "limit");
        assert_eq!(params["items"][0]["cells"]["name"]["text"], "limit");
        assert!(params["items"][0]["span"]["end"].as_u64().expect("end") > 0);
        // The body is source-edited prose; `block_kinds` hints at content.
        assert_eq!(
            v["body"]["block_kinds"],
            serde_json::json!(["diagram", "wf_button", "terminal"])
        );
        drop(dir);
    }

    /// The screen editor builds its synthetic previews from the file
    /// declaring the `wad` block — the model aggregator, far cheaper to
    /// evaluate than the book entry. No `wad` instance ⇒ null (non-WAD
    /// docs fall back to the site entry).
    #[test]
    fn model_entry_is_the_wad_declaring_file() {
        let v = model("wad w { name = \"W\" }\nzone z { name = \"Z\" }\n");
        assert_eq!(v["model_entry"], "main.wcl");
        let none = model("zone z { name = \"Z\" }\n");
        assert!(none["model_entry"].is_null());
    }

    #[test]
    fn nodes_carry_editable_cells_and_anchors() {
        let v = model("system s { name = \"S\"  repo = r }\n");
        let n = node(&v, "s");
        assert_eq!(n["file"], "main.wcl");
        assert_eq!(n["title"], "S");
        assert!(n["span"]["end"].as_u64().expect("end") > 0);
        assert_eq!(n["cells"]["name"]["state"], "literal");
        assert_eq!(n["cells"]["repo"]["expr"], true);
        assert_eq!(n["cells"]["@0"]["text"], "s");
    }
}
