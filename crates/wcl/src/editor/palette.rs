//! `GET /api/palette` — what the add-block UI can insert here.
//!
//! Unit and diagram kinds come straight off the [kind
//! model](super::kinds); body kinds are a curated list of wdoc content
//! blocks, static because most render via Rust fundamentals and there is no
//! WCL schema rich enough to introspect an insertion template from;
//! components are the `wdoc_component` declarations authored inside the
//! served tree, with the slots that drive their property form.

use std::sync::Arc;

use axum::extract::State;
use axum::http::Uri;
use axum::response::Response;

use wcl_lang::Document;

use super::kinds::{Kind, KindModel, is_wad, is_wskill, site_kind};
use super::util::{first_label, value_string};
use super::{EditorState, Workspace, run_blocking};
use crate::serve::query_param;

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

/// Query: `entry`, `site?`, `page_file?` → `{ site_type, wskill, wad,
/// unit_kinds, diagram_kinds, body_kinds, components }`. Unit and diagram
/// kinds come from the kind model (the generated create and property forms
/// are built from their `fields`); body kinds are the curated wdoc content
/// blocks with canonical insertion snippets; components are the
/// `wdoc_component` declarations authored inside the served tree, with
/// their slots.
///
/// Nothing here mines suggestions: the palette must open promptly on a
/// large model, and suggestions are the one member that evaluates every
/// instance.
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

fn palette(
    ws: &Workspace,
    entry: &str,
    site: Option<&str>,
    page_file: Option<&str>,
) -> Result<serde_json::Value, String> {
    let doc_entry = ws.doc_entry(entry, page_file)?;
    let doc = wcl_wdoc::open_doc_for_edit(&doc_entry).map_err(super::err_str)?;
    let model = KindModel::new(&doc);

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
        "unit_kinds": model.unit_kinds().map(Kind::json).collect::<Vec<_>>(),
        "diagram_kinds": model.diagram_kinds(),
        "body_kinds": body_kinds,
        "components": components(ws, &doc),
    }))
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
    use crate::editor::testsupport::{OBJECT_DOC, workspace_with};

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
        let wireframe_grid = shapes
            .iter()
            .find(|k| k["kind"] == "wf_grid")
            .unwrap_or_else(|| panic!("no wireframe grid kind: {v:#}"));
        assert_eq!(wireframe_grid["accepts_children"], true, "{v:#}");
        let wireframe_button = shapes
            .iter()
            .find(|k| k["kind"] == "wf_button")
            .unwrap_or_else(|| panic!("no wireframe button kind: {v:#}"));
        assert_eq!(wireframe_button["accepts_children"], false, "{v:#}");
        // Content-output blocks don't extend SvgBlock merely because a page
        // slot accepts them.
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
