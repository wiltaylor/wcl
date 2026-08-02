//! Site-navigation model endpoints for the editor's Design mode.
//!
//! `GET /api/nav` projects the selected site's menu structure into an
//! editable tree where every entry carries its **source binding** (declaring
//! file + byte span), and `POST /api/nav/op` applies structural edits
//! through the same parse → mutate → [`crate::edit::commit`] pipeline as
//! `/api/block/ops`. The model is site-type-aware:
//!
//! - **wskill book** — an **adapter** over [`wcl_wskill`]: the library says
//!   what the indexes, their pins and the units are ([`wcl_wskill::Graph`])
//!   and owns every structural op ([`wcl_wskill::ops`]); this module adds
//!   the book projection's page names, the source bindings, and the commit.
//! - **plain book** — the `site.toc` `chapter` tree, literal chapters only;
//!   a `wdoc_repeater` shows as a read-only synthetic entry.
//! - **website** — the `site.menu` `item` tree, same mechanics.
//! - **presentation** — the `site.deck` `section` / `slide` grid.
//!
//! The static-site half is genuinely this module's: a `toc` / `menu` / `deck`
//! is wdoc page structure, not wskill data, and its ops are span-addressed
//! because its entries have no ids to be named by.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::State;
use axum::http::{StatusCode, Uri};
use axum::response::Response;

use wcl_lang::ast::{self, Expr, Item};
use wcl_lang::{Document, Span, edit as ast_edit, format as wcl_format, parse_for_edit};
use wcl_wskill::ops::{self as wops, Dir, IndexHome, Op};

use super::graph::open_graph;
use super::kinds::{self, KindModel};
use super::preview::Sessions;
use super::util::{anchor_json, first_label, value_string};
use super::{EditorState, Workspace, run_blocking};
use crate::serve::{json_error, parse_json_body, query_param};

// ---------------------------------------------------------------------------
// `GET /api/nav`
// ---------------------------------------------------------------------------

/// Query: `entry`, `site?` → `{ site_type, wskill, nav, units?, pages,
/// container? }`. `nav` is the editable tree (entries carry `source`
/// bindings); `units` (wskill) are the pin candidates; `pages` are the
/// declared page blocks; `container` is the binding of the `toc` / `menu` /
/// `deck` block new entries insert into (absent for the wskill model).
pub(super) async fn handle_nav(State(state): State<Arc<EditorState>>, uri: Uri) -> Response {
    let entry = query_param(&uri, "entry");
    let site = query_param(&uri, "site");
    let state2 = Arc::clone(&state);
    run_blocking(move || {
        let entry = entry.ok_or("missing entry")?;
        nav(&state2.ws, &entry, site.as_deref())
    })
    .await
}

fn nav(ws: &Workspace, entry: &str, site: Option<&str>) -> Result<serde_json::Value, String> {
    let entry_abs = ws.abs(entry)?;
    let doc = wcl_wdoc::open_doc_for_edit(&entry_abs).map_err(super::err_str)?;
    let wskill = kinds::is_wskill(&doc);
    let site_type = kinds::site_kind(&doc, site);

    let pages = declared_pages(ws, &doc, &entry_abs);
    if wskill && site_type == "book" {
        let (nav, units) = wskill_nav(ws, &open_graph(&doc, &entry_abs)?)?;
        return Ok(serde_json::json!({
            "ok": true,
            "site_type": site_type,
            "wskill": true,
            "nav": nav,
            "units": units,
            "pages": pages,
        }));
    }

    let (nav, container) = static_site_nav(ws, &entry_abs, site, site_type)?;
    Ok(serde_json::json!({
        "ok": true,
        "site_type": site_type,
        "wskill": wskill,
        "nav": nav,
        "pages": pages,
        "container": container,
    }))
}

/// Every declared `page` block (repeater-generated pages are projections,
/// not blocks — they don't appear here).
fn declared_pages(ws: &Workspace, doc: &Document, entry_abs: &Path) -> Vec<serde_json::Value> {
    doc.blocks_with_source()
        .filter(|(_, b)| b.kind() == "page")
        .filter_map(|(path, b)| {
            let name = first_label(&b)?;
            let title = b
                .field("title")
                .and_then(|f| f.value().ok().cloned())
                .as_ref()
                .map(value_string);
            let file = path.unwrap_or(entry_abs);
            Some(serde_json::json!({
                "name": name,
                "title": title,
                "source": source_binding(ws, file, b.span()),
            }))
        })
        .collect()
}

fn source_binding(ws: &Workspace, file: &Path, span: Span) -> serde_json::Value {
    let rel = std::fs::canonicalize(file)
        .unwrap_or_else(|_| file.to_path_buf())
        .strip_prefix(ws.root_dir())
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| file.display().to_string());
    serde_json::json!({
        "file": rel,
        "span": super::span_json(span),
    })
}

// ---------------------------------------------------------------------------
// The wskill model — an adapter over `wcl_wskill::Graph`
// ---------------------------------------------------------------------------

/// The page-name prefix a wskill book's repeaters use for a unit kind. The
/// one piece of *projection* knowledge in this branch: the model says what a
/// unit is, the book template decides what its page is called.
fn page_prefix(kind: &str) -> &str {
    match kind {
        // The book template names procedure pages `process_<id>`.
        "procedure" => "process",
        other => other,
    }
}

fn wskill_nav(
    ws: &Workspace,
    model: &wcl_wskill::Graph,
) -> Result<(Vec<serde_json::Value>, Vec<serde_json::Value>), String> {
    let nav: Vec<serde_json::Value> = model
        .indexes
        .iter()
        .map(|i| index_entry(ws, model, i))
        .collect();
    let units: Vec<serde_json::Value> = model
        .units
        .iter()
        .map(|u| {
            serde_json::json!({
                "id": u.id,
                "kind": u.kind,
                "title": u.title,
                "page": format!("{}_{}", page_prefix(&u.kind), u.id),
                "source": anchor_json(ws, &model.root, &u.anchor),
            })
        })
        .collect();
    Ok((nav, units))
}

fn index_entry(
    ws: &Workspace,
    model: &wcl_wskill::Graph,
    index: &wcl_wskill::Index,
) -> serde_json::Value {
    // A pinned id that names nothing is shown as missing rather than
    // dropped — a dangling link is the author's to see and fix.
    let mut children: Vec<serde_json::Value> = index
        .pinned
        .iter()
        .map(|rid| match model.unit(rid) {
            Some(u) => serde_json::json!({
                "kind": "unit",
                "unit": { "kind": u.kind, "id": rid },
                "title": u.title,
                "page": format!("{}_{rid}", page_prefix(&u.kind)),
                "source": anchor_json(ws, &model.root, &u.anchor),
            }),
            None => serde_json::json!({
                "kind": "unit",
                "unit": { "kind": null, "id": rid },
                "title": rid,
                "page": null,
                "missing": true,
            }),
        })
        .collect();
    children.extend(index.children.iter().map(|c| index_entry(ws, model, c)));
    serde_json::json!({
        "kind": "index",
        "id": index.id,
        "title": index.title,
        // A content index (with a body) is its own page; a nav index is a
        // heading whose children are the pinned unit pages.
        "page": index.has_body().then(|| format!("index_{}", index.id)),
        "source": anchor_json(ws, &model.root, &index.anchor),
        "children": children,
    })
}

// ---------------------------------------------------------------------------
// The static model: toc / menu / deck read off the entry AST
// ---------------------------------------------------------------------------

/// Which child block of `site` holds the nav for each site type.
fn container_kind(site_type: &str) -> &'static str {
    match site_type {
        "website" => "menu",
        "presentation" => "deck",
        _ => "toc",
    }
}

fn static_site_nav(
    ws: &Workspace,
    entry_abs: &Path,
    site: Option<&str>,
    site_type: &str,
) -> Result<(Vec<serde_json::Value>, serde_json::Value), String> {
    let text = crate::edit::read(entry_abs)?;
    let src = parse_for_edit(&text, entry_abs.display().to_string()).map_err(super::err_str)?;
    let site_block = src
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Block(b) if b.kind == "site" => Some(b),
            _ => None,
        })
        .find(|b| match site {
            Some(name) => super::util::ast_label(b).as_deref() == Some(name),
            None => true,
        })
        .ok_or("no site block in the entry document")?;
    let wanted = container_kind(site_type);
    let container = site_block
        .items
        .iter()
        .find_map(|it| match it {
            Item::Block(b) if b.kind == wanted => Some(b),
            _ => None,
        })
        .ok_or_else(|| format!("the site has no `{wanted}` block"))?;
    let nav = container
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Block(b) => Some(static_entry(ws, b, entry_abs)),
            _ => None,
        })
        .collect();
    Ok((nav, source_binding(ws, entry_abs, container.span)))
}

/// One literal nav entry (`chapter` / `item` / `section` / `slide`) with its
/// source binding; a `wdoc_repeater` becomes a read-only synthetic entry.
fn static_entry(ws: &Workspace, b: &ast::Block, entry_abs: &Path) -> serde_json::Value {
    if b.kind == "wdoc_repeater" {
        return serde_json::json!({
            "kind": "generated",
            "title": "(generated entries)",
            "synthetic": true,
            "source": source_binding(ws, entry_abs, b.span),
        });
    }
    let title = super::util::ast_label(b);
    let page = b.items.iter().find_map(|it| match it {
        Item::Field(f) if f.name == "page" => match &f.expr {
            Expr::Identifier(s, _) => Some(s.clone()),
            _ => None,
        },
        _ => None,
    });
    let href = b.items.iter().find_map(|it| match it {
        Item::Field(f) if f.name == "href" => match &f.expr {
            Expr::Utf8(s) | Expr::Ascii(s) => Some(s.clone()),
            _ => None,
        },
        _ => None,
    });
    // A `slide` names its page as the inline label, not a field.
    let page = match (b.kind.as_str(), page, &title) {
        ("slide", None, Some(t)) => Some(t.clone()),
        (_, p, _) => p,
    };
    let children: Vec<serde_json::Value> = b
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Block(c) => Some(static_entry(ws, c, entry_abs)),
            _ => None,
        })
        .collect();
    let synthetic = title.is_none();
    serde_json::json!({
        "kind": b.kind,
        "title": title.unwrap_or_else(|| "(computed)".to_string()),
        "page": page,
        "href": href,
        "synthetic": synthetic,
        "source": source_binding(ws, entry_abs, b.span),
        "children": children,
    })
}

// ---------------------------------------------------------------------------
// `POST /api/nav/op`
// ---------------------------------------------------------------------------

/// Body: `{ entry, site?, op, ... }`. Structural nav edits; each op names
/// its targets by source binding (`file` + `span`) or, for the wskill
/// `related`-list ops, by ids (spans inside a `related` list aren't
/// block-addressable). Ops:
///
/// - `rename { file, span, kind, title }` — retitle an entry (`index` →
///   the `name` field; others → inline label slot 0)
/// - `remove { file, span }` / `move { file, span, dir }` — like block ops
/// - `add_section { file, container_span?, title, id? }` — a new `index`
///   (wskill: `id` required, appended to `file`) or `chapter` / `item` /
///   `section` (inserted into the container / parent span)
/// - `add_page { name, title, nav: { file, container_span, kind } }` — a
///   new top-level `page` block in the entry plus its nav entry
/// - `create_index { id, name, parent_id? }` — a new `index` block, either
///   placed by convention beside the existing ones or nested in `parent_id`
///
/// Anything else the wskill vocabulary names ([`wops::is_op`] — `pin_unit`,
/// `unpin_unit`, `reorder_children`, `delete_index`, `move_index`,
/// `promote_index`, `demote_index`, `related_add`, `related_remove`) is
/// **decoded by the library** ([`wops::from_json`]) rather than translated
/// here: the request body IS an op in the one JSON dialect `wcl wskill op`
/// reads, id-addressed because spans shift under every reformat and ids
/// don't. This module commits what the op returns and marks the previews
/// stale.
pub(super) async fn handle_nav_op(State(state): State<Arc<EditorState>>, body: String) -> Response {
    let v = match parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    let state2 = Arc::clone(&state);
    run_blocking(move || nav_op(&state2.ws, &state2.sessions, &v)).await
}

pub(super) fn nav_op(
    ws: &Workspace,
    previews: &Sessions,
    v: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let entry = crate::edit::str_field(v, "entry")?;
    let entry_abs = ws.abs(entry)?;
    let op = crate::edit::str_field(v, "op")?;
    let result = match op {
        "rename" => {
            let (file, span) = op_target(ws, v)?;
            let title = crate::edit::str_field(v, "title")?;
            edit_file(&entry_abs, &file, |src| {
                let block = ast_edit::find_block_by_span(&mut src.items, span)
                    .ok_or_else(super::util::stale_span)?;
                if block.kind == "index" {
                    ast_edit::set_or_insert_field(
                        block,
                        "name",
                        ast_edit::string_literal_expr(title),
                    );
                } else if !ast_edit::set_label(block, 0, ast_edit::string_literal_expr(title)) {
                    return Err("the entry has no title slot".into());
                }
                Ok(())
            })
        }
        "remove" => {
            let (file, span) = op_target(ws, v)?;
            edit_file(&entry_abs, &file, |src| {
                if !ast_edit::remove_block_by_span(&mut src.items, span) {
                    return Err(super::util::stale_span());
                }
                Ok(())
            })
        }
        "move" => {
            let (file, span) = op_target(ws, v)?;
            let down = move_dir(v)? == Dir::Down;
            edit_file(&entry_abs, &file, |src| {
                if !ast_edit::move_block_by_span(&mut src.items, span, down) {
                    return Err("the entry is already at the edge".into());
                }
                Ok(())
            })
        }
        "add_section" => add_section(ws, &entry_abs, v),
        "add_page" => add_page(&entry_abs, v),
        "create_index" => create_index(ws, &entry_abs, v),
        // Everything the library's vocabulary names is decoded and applied by
        // the library: the request IS an op, in the one JSON dialect
        // `wcl wskill op` reads, so the panel and the curator send the same
        // thing rather than two spellings of it.
        other if wops::is_op(other) => {
            wskill_op(&entry_abs, wops::from_json(v).map_err(|e| e.to_string())?)
        }
        other => Err(format!("unknown nav op `{other}`")),
    };
    if result.is_ok() {
        // Every nav op rewrites source: the disk moved under every built
        // preview.
        previews.invalidate();
    }
    result
}

/// The op's target binding: `file` (repo-relative) + `span`.
fn op_target(ws: &Workspace, v: &serde_json::Value) -> Result<(PathBuf, Span), String> {
    let file_abs = ws.abs(crate::edit::str_field(v, "file")?)?;
    let span = super::util::span_field(v, "span")?;
    Ok((file_abs, span))
}

/// Parse `file`, apply `mutate`, print, commit against `entry_abs`.
fn edit_file(
    entry_abs: &Path,
    file: &Path,
    mutate: impl FnOnce(&mut ast::Source) -> Result<(), String>,
) -> Result<serde_json::Value, String> {
    let text = crate::edit::read(file)?;
    let mut src = parse_for_edit(&text, file.display().to_string()).map_err(super::err_str)?;
    mutate(&mut src)?;
    let new_text = wcl_format::to_source(&src);
    crate::edit::commit(entry_abs, vec![(file.to_path_buf(), new_text)])?;
    Ok(serde_json::json!({ "ok": true }))
}

fn add_section(
    ws: &Workspace,
    entry_abs: &Path,
    v: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let title = crate::edit::str_field(v, "title")?;
    match v.get("id").and_then(serde_json::Value::as_str) {
        // Wskill: a new `index` block appended to the target file — the same
        // library op `create_index` runs, with the panel naming the file
        // rather than placement deriving it.
        Some(id) => {
            let file_abs = ws.abs(crate::edit::str_field(v, "file")?)?;
            wskill_op(
                entry_abs,
                Op::CreateIndex {
                    id: id.to_string(),
                    name: title.to_string(),
                    home: IndexHome::InFile(file_abs),
                },
            )
        }
        // Static sites: a `chapter` / `item` / `section` inserted into the
        // container (or a parent entry) span.
        None => {
            let (file, span) = op_target(ws, v)?;
            let kind = crate::edit::str_field(v, "kind")?;
            if !matches!(kind, "chapter" | "item" | "section") {
                return Err(format!("`{kind}` is not a nav section kind"));
            }
            let block = ast_edit::build_block(
                kind,
                &[],
                vec![ast_edit::string_literal_expr(title)],
                Vec::new(),
            );
            edit_file(entry_abs, &file, |src| {
                let parent = ast_edit::find_block_by_span(&mut src.items, span)
                    .ok_or_else(super::util::stale_span)?;
                ast_edit::insert_block_at_index(&mut parent.items, usize::MAX, block);
                Ok(())
            })
        }
    }
}

fn add_page(entry_abs: &Path, v: &serde_json::Value) -> Result<serde_json::Value, String> {
    let name = crate::edit::str_field(v, "name")?;
    if !super::util::is_identifier(name) {
        return Err(format!("`{name}` is not a valid page name"));
    }
    let title = crate::edit::str_field(v, "title")?;
    let mut fields = Vec::new();
    if let Some(sites) = page_sites_field(entry_abs, v)? {
        fields.push(sites);
    }
    fields.push(("title".to_string(), ast_edit::string_literal_expr(title)));
    let page = ast_edit::build_block(
        "page",
        &[],
        vec![Expr::Identifier(name.to_string(), Span::new(0, 0))],
        fields,
    );
    // The nav entry that links the page (optional — a page can be added
    // unlinked): `{ file, container_span, kind }`.
    let nav = v.get("nav").filter(|n| !n.is_null());
    edit_file(entry_abs, entry_abs, |src| {
        ast_edit::append_top_level_block(src, page);
        if let Some(nav) = nav {
            let span = super::util::span_field(nav, "container_span")?;
            let kind = crate::edit::str_field(nav, "kind")?;
            let entry_block = match kind {
                "chapter" | "item" => {
                    let mut b = ast_edit::build_block(
                        kind,
                        &[],
                        vec![ast_edit::string_literal_expr(title)],
                        Vec::new(),
                    );
                    b.items.push(Item::Field(ast::Field {
                        name: "page".to_string(),
                        expr: Expr::Identifier(name.to_string(), Span::new(0, 0)),
                        decorators: Vec::new(),
                        span: Span::new(0, 0),
                        leading_trivia: Vec::new(),
                        trailing_comment: None,
                    }));
                    b
                }
                "slide" => ast_edit::build_block(
                    "slide",
                    &[],
                    vec![Expr::Identifier(name.to_string(), Span::new(0, 0))],
                    Vec::new(),
                ),
                other => return Err(format!("`{other}` is not a nav entry kind")),
            };
            let parent = ast_edit::find_block_by_span(&mut src.items, span)
                .ok_or_else(super::util::stale_span)?;
            ast_edit::insert_block_at_index(&mut parent.items, usize::MAX, entry_block);
        }
        Ok(())
    })
}

/// The `sites = [:name]` field a new page must carry, or `None` when the
/// entry declares at most one site (where the field is optional and every
/// page is a member anyway).
///
/// A document declaring more than one site rejects an untagged page at
/// build time — the site chooses the page's template — so the op tags the
/// new page with the site whose nav it is editing, which is the `site`
/// every nav op already carries. Reading the entry's own top-level `site`
/// blocks matches `static_site_nav`: the client's `site` came from the nav
/// model, which addresses exactly those.
fn page_sites_field(
    entry_abs: &Path,
    v: &serde_json::Value,
) -> Result<Option<(String, Expr)>, String> {
    let text = crate::edit::read(entry_abs)?;
    let src = parse_for_edit(&text, entry_abs.display().to_string()).map_err(super::err_str)?;
    let declared: Vec<Option<String>> = src
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Block(b) if b.kind == "site" => Some(super::util::ast_label(b)),
            _ => None,
        })
        .collect();
    if declared.len() < 2 {
        return Ok(None);
    }
    let names: Vec<&str> = declared.iter().flatten().map(String::as_str).collect();
    let site = v.get("site").and_then(serde_json::Value::as_str);
    let Some(site) = site.filter(|s| names.contains(s)) else {
        return Err(format!(
            "this document declares {} sites, so a new page must name the one \
             it belongs to (declared: {})",
            declared.len(),
            names.join(", ")
        ));
    };
    Ok(Some((
        "sites".to_string(),
        Expr::ListLit {
            elements: vec![Expr::Symbol(site.to_string())],
            elem_trivia: vec![Default::default()],
            trailing_trivia: Vec::new(),
            span: Span::new(0, 0),
        },
    )))
}

// ---------------------------------------------------------------------------
// The wskill op vocabulary — decoded by the library, applied, committed
// ---------------------------------------------------------------------------

fn move_dir(v: &serde_json::Value) -> Result<Dir, String> {
    match crate::edit::str_field(v, "dir")? {
        "down" => Ok(Dir::Down),
        "up" => Ok(Dir::Up),
        other => Err(format!("bad move dir `{other}`")),
    }
}

/// Apply one library op against the wskill behind `entry_abs` and commit the
/// files it rewrites. The document is dropped before the commit, which
/// rewrites the very files it was read from.
fn wskill_op(entry_abs: &Path, op: Op) -> Result<serde_json::Value, String> {
    let changes = {
        let doc = wcl_wdoc::open_doc_for_edit(entry_abs).map_err(super::err_str)?;
        let model = open_graph(&doc, entry_abs)?;
        wops::apply(&model, &op).map_err(|e| e.to_string())?
    };
    crate::edit::commit(
        entry_abs,
        changes.into_iter().map(|c| (c.file, c.text)).collect(),
    )?;
    Ok(serde_json::json!({ "ok": true }))
}

/// A new `index` block: nested inside `parent_id` (the library op), else
/// placed by the same convention `unit_create` uses — where a first-of-its-
/// kind block LANDS is an editor convention, so the adapter answers it and
/// the library still says what an index is and validates the id.
fn create_index(
    ws: &Workspace,
    entry_abs: &Path,
    v: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let id = crate::edit::str_field(v, "id")?;
    let name = crate::edit::str_field(v, "name")?;
    // A named parent makes this an ordinary library op, so it is decoded as
    // one rather than rebuilt here. Only the no-parent case is the editor's
    // to answer.
    if v.get("parent_id")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|s| !s.is_empty())
    {
        return wskill_op(entry_abs, wops::from_json(v).map_err(|e| e.to_string())?);
    }

    let doc = wcl_wdoc::open_doc_for_edit(entry_abs).map_err(super::err_str)?;
    wops::check_new_index_id(&open_graph(&doc, entry_abs)?, id).map_err(|e| e.to_string())?;
    // Placed and staged inside this scope so the model's borrow ends here;
    // the document itself is released just below, before the commit rewrites
    // the files it was read from.
    let (file, changes) = {
        let model = KindModel::new(&doc);
        let placement = super::placement::place_unit(&model, &doc, entry_abs, "index")?;
        let mut changes: Vec<(PathBuf, String)> = Vec::new();
        let file = super::placement::write_new_block(
            placement,
            id,
            wops::index_block(id, name),
            entry_abs,
            &mut changes,
        )?;
        (file, changes)
    };
    drop(doc);
    crate::edit::commit(entry_abs, changes)?;
    Ok(serde_json::json!({
        "ok": true,
        "id": id,
        "file": ws.rel(&file)?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::preview::Sessions;
    use crate::editor::testsupport::{
        workspace_built_by, write_mini_wskill, write_mini_wskill_nested, write_mini_wskill_training,
    };

    /// Nav ops are id- or span-addressed writes; nothing here needs a
    /// preview scratch tree, only the handle that marks one stale.
    fn op(ws: &Workspace, body: serde_json::Value) -> Result<serde_json::Value, String> {
        nav_op(ws, &Sessions::default(), &body)
    }

    fn read(ws: &Workspace, rel: &str) -> String {
        std::fs::read_to_string(ws.root_dir().join(rel)).unwrap()
    }

    /// A pinned id naming nothing is shown as a missing entry rather than
    /// dropped — a dangling link is the author's to see and fix — and an
    /// index with a `body` is a page of its own.
    #[test]
    fn wskill_nav_reports_dangling_pins_and_body_pages() {
        let (_td, ws) = workspace_built_by(|root| {
            write_mini_wskill(root);
            let main = std::fs::read_to_string(root.join("main.wcl")).unwrap();
            let main = main.replace(
                "@block(\"index\")\ntype Index {\n  @inline(0) id: identifier\n  name: utf8\n  related: list<identifier>?\n}",
                "@block(\"body\") @schemaless\ntype UnitBody {\n}\n\n@block(\"index\")\ntype Index {\n  @inline(0) id: identifier\n  name: utf8\n  related: list<identifier>?\n  @child(\"body\") body: UnitBody?\n}",
            );
            std::fs::write(root.join("main.wcl"), main).unwrap();
            std::fs::write(
                root.join("data/indexes.wcl"),
                "index lang {\n  name = \"Language\"\n  related = [alpha, ghost]\n\n  \
                 body {\n    p \"Read these in order.\"\n  }\n}\n",
            )
            .unwrap();
        });

        let v = nav(&ws, "main.wcl", Some("book")).expect("nav");
        let lang = &v["nav"][0];
        assert_eq!(lang["page"], "index_lang", "a body makes it a page");
        let children = lang["children"].as_array().unwrap();
        assert_eq!(children[0]["title"], "Alpha");
        assert_eq!(children[1]["title"], "ghost");
        assert_eq!(children[1]["missing"], true, "{v:#}");
        assert_eq!(children[1]["unit"]["kind"], serde_json::Value::Null);
    }

    /// Creating a top-level index is the one wskill nav op the adapter still
    /// answers half of: the library validates the id and builds the block,
    /// placement (an editor convention) picks the file.
    #[test]
    fn create_index_places_a_new_top_level_index() {
        let (_td, ws) = workspace_built_by(write_mini_wskill);

        let res = op(
            &ws,
            serde_json::json!({
                "entry": "main.wcl", "op": "create_index",
                "id": "usage", "name": "Using it",
            }),
        )
        .expect("create_index");
        // Beside the index it learnt from — a one-per-file layout, so its
        // own file — and never in the projection entry.
        assert_eq!(res["file"], "data/usage.wcl");
        assert!(read(&ws, "data/usage.wcl").contains("index usage"));
        assert!(read(&ws, "data/usage.wcl").contains("name = \"Using it\""));

        // The library's validation still applies on the adapter's path.
        for (id, msg) in [("lang", "already exists"), ("9bad", "not a valid id")] {
            let e = op(
                &ws,
                serde_json::json!({
                    "entry": "main.wcl", "op": "create_index",
                    "id": id, "name": "Nope",
                }),
            )
            .unwrap_err();
            assert!(e.contains(msg), "{id}: {e}");
        }
    }

    /// The index tree's structural ops, driven through the endpoint: they are
    /// the library's, addressed by id, and each rewrites one file.
    #[test]
    fn index_tree_ops_nest_and_unnest() {
        let (_td, ws) = workspace_built_by(write_mini_wskill_nested);
        let ids = || {
            let v = nav(&ws, "main.wcl", Some("book")).expect("nav");
            v["nav"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n["id"].as_str().unwrap().to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(), ["lang"], "the sub-index is nested to begin with");

        op(
            &ws,
            serde_json::json!({
                "entry": "main.wcl", "op": "promote_index", "index_id": "lang_sub",
            }),
        )
        .expect("promote");
        assert_eq!(ids(), ["lang", "lang_sub"]);

        op(
            &ws,
            serde_json::json!({
                "entry": "main.wcl", "op": "demote_index", "index_id": "lang_sub",
            }),
        )
        .expect("demote");
        assert_eq!(ids(), ["lang"]);

        // Nesting is one level deep, and the refusal says so.
        let e = op(
            &ws,
            serde_json::json!({
                "entry": "main.wcl", "op": "create_index",
                "id": "deep", "name": "Deep", "parent_id": "lang_sub",
            }),
        )
        .unwrap_err();
        assert!(e.contains("one level deep"), "{e}");

        op(
            &ws,
            serde_json::json!({
                "entry": "main.wcl", "op": "delete_index", "index_id": "lang",
            }),
        )
        .expect("delete");
        assert!(ids().is_empty(), "the subtree goes with it");
    }

    /// A `related` list written as an expression is readable but must never
    /// be rewritten — the nav ops used to overwrite it silently.
    #[test]
    fn a_computed_pin_list_refuses_every_write() {
        let (_td, ws) = workspace_built_by(|root| {
            write_mini_wskill(root);
            std::fs::write(
                root.join("data/indexes.wcl"),
                "index lang {\n  name = \"Language\"\n  related = concat([alpha], [beta])\n}\n",
            )
            .unwrap();
        });
        for body in [
            serde_json::json!({
                "entry": "main.wcl", "op": "pin_unit",
                "index_id": "lang", "unit_id": "alpha",
            }),
            serde_json::json!({
                "entry": "main.wcl", "op": "reorder_children",
                "index_id": "lang", "order": ["beta", "alpha"],
            }),
        ] {
            let e = op(&ws, body.clone()).unwrap_err();
            assert!(e.contains("computed"), "{body}: {e}");
        }
        assert!(read(&ws, "data/indexes.wcl").contains("concat([alpha], [beta])"));
    }

    #[test]
    fn wskill_nav_model_and_related_ops() {
        let (_td, ws) = workspace_built_by(write_mini_wskill);

        let v = nav(&ws, "main.wcl", Some("book")).expect("nav");
        assert_eq!(v["wskill"], true);
        assert_eq!(v["site_type"], "book");
        let model = v["nav"].as_array().unwrap();
        assert_eq!(model.len(), 1, "{v:#}");
        assert_eq!(model[0]["kind"], "index");
        assert_eq!(model[0]["title"], "Language");
        let children = model[0]["children"].as_array().unwrap();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0]["title"], "Alpha");
        assert_eq!(children[0]["page"], "concept_alpha");
        assert_eq!(children[0]["source"]["file"], "data/concepts/alpha.wcl");
        assert_eq!(v["units"].as_array().unwrap().len(), 2);

        // Reorder, unpin, re-pin.
        op(
            &ws,
            serde_json::json!({
                "entry": "main.wcl", "op": "reorder_children",
                "index_id": "lang", "order": ["beta", "alpha"],
            }),
        )
        .expect("reorder");
        assert!(
            read(&ws, "data/indexes.wcl").contains("related = [beta, alpha]"),
            "{}",
            read(&ws, "data/indexes.wcl")
        );
        op(
            &ws,
            serde_json::json!({
                "entry": "main.wcl", "op": "unpin_unit",
                "index_id": "lang", "unit_id": "alpha",
            }),
        )
        .expect("unpin");
        assert!(read(&ws, "data/indexes.wcl").contains("related = [beta]"));
        op(
            &ws,
            serde_json::json!({
                "entry": "main.wcl", "op": "pin_unit",
                "index_id": "lang", "unit_id": "alpha",
            }),
        )
        .expect("pin");
        assert!(
            read(&ws, "data/indexes.wcl").contains("related = [beta, alpha]"),
            "{}",
            read(&ws, "data/indexes.wcl")
        );

        // A bad permutation is rejected.
        let e = op(
            &ws,
            serde_json::json!({
                "entry": "main.wcl", "op": "reorder_children",
                "index_id": "lang", "order": ["beta"],
            }),
        )
        .unwrap_err();
        assert!(e.contains("permutation"), "{e}");
    }

    #[test]
    fn static_book_nav_and_ops() {
        let doc = "import <wdoc.wcl>\n\nsite docs {\n  title = \"The Docs\"\n  root = true\n  toc {\n    chapter \"Start\" {\n      page = index\n    }\n    chapter \"Guides\" {\n      chapter \"Deep\" {\n        page = deep\n      }\n    }\n  }\n}\n\npage index {\n  title = \"Hi\"\n\n  h1 \"Hello\"\n}\n\npage deep {\n  title = \"Deep\"\n\n  h1 \"Deep\"\n}\n";
        let (_td, ws) =
            workspace_built_by(|root| std::fs::write(root.join("main.wcl"), doc).unwrap());
        // Every commit reprints the file, so a nav entry's span moves — both
        // resolve against the model as it stands, exactly as the nav panel
        // does when it refetches after a write.
        let entry = |title: &str| {
            let v = nav(&ws, "main.wcl", Some("docs")).expect("nav");
            v["nav"]
                .as_array()
                .unwrap()
                .iter()
                .find(|n| n["title"] == title)
                .unwrap_or_else(|| panic!("no `{title}` entry: {v:#}"))["source"]
                .clone()
        };
        let container = || nav(&ws, "main.wcl", Some("docs")).expect("nav")["container"].clone();

        let v = nav(&ws, "main.wcl", Some("docs")).expect("nav");
        assert_eq!(v["site_type"], "book");
        let model = v["nav"].as_array().unwrap();
        assert_eq!(model.len(), 2);
        assert_eq!(model[0]["kind"], "chapter");
        assert_eq!(model[0]["title"], "Start");
        assert_eq!(model[0]["page"], "index");
        assert_eq!(model[1]["children"][0]["title"], "Deep");
        assert!(v["container"]["span"]["start"].is_u64());
        assert_eq!(v["pages"].as_array().unwrap().len(), 2);

        // Rename a chapter, move it, then add a page linked into the toc.
        let start = entry("Start");
        op(
            &ws,
            serde_json::json!({
                "entry": "main.wcl", "op": "rename", "kind": "chapter",
                "file": start["file"], "span": start["span"], "title": "Begin",
            }),
        )
        .expect("rename");
        assert!(read(&ws, "main.wcl").contains("chapter \"Begin\""));

        let begin = entry("Begin");
        op(
            &ws,
            serde_json::json!({
                "entry": "main.wcl", "op": "move", "dir": "down",
                "file": begin["file"], "span": begin["span"],
            }),
        )
        .expect("move");
        let text = read(&ws, "main.wcl");
        assert!(
            text.find("chapter \"Guides\"").unwrap() < text.find("chapter \"Begin\"").unwrap(),
            "{text}"
        );

        // Add a page + its chapter entry in one op.
        op(
            &ws,
            serde_json::json!({
                "entry": "main.wcl", "op": "add_page",
                "name": "faq", "title": "FAQ",
                "nav": { "container_span": container()["span"], "kind": "chapter" },
            }),
        )
        .expect("add_page");
        let text = read(&ws, "main.wcl");
        assert!(text.contains("page faq"), "{text}");
        assert!(text.contains("chapter \"FAQ\""), "{text}");
        // The new page + entry are in the model.
        let v = nav(&ws, "main.wcl", Some("docs")).expect("nav");
        assert!(
            v["nav"]
                .as_array()
                .unwrap()
                .iter()
                .any(|n| n["title"] == "FAQ" && n["page"] == "faq"),
            "{v:#}"
        );
    }

    /// A multi-site document requires `sites` on every page, so `add_page`
    /// tags the new one with the site whose nav it is editing — the `site`
    /// every nav op already carries. Without it the op would leave the
    /// document unbuildable, and every later preview build would fail.
    #[test]
    fn add_page_tags_the_site_in_a_multi_site_document() {
        let doc = "import <wdoc.wcl>\n\nsite docs {\n  title = \"Docs\"\n  root = true\n  \
                   toc {\n    chapter \"Start\" {\n      page = index\n    }\n  }\n}\n\n\
                   site blog {\n  title = \"Blog\"\n  toc {\n    chapter \"Posts\" {\n      \
                   page = post\n    }\n  }\n}\n\npage index {\n  sites = [:docs]\n  \
                   title = \"Hi\"\n\n  h1 \"Hello\"\n}\n\npage post {\n  sites = [:blog]\n  \
                   title = \"Post\"\n\n  h1 \"Post\"\n}\n";
        let (_td, ws) = workspace_built_by(|root| {
            std::fs::write(root.join("main.wcl"), doc).unwrap();
        });
        let container = |site: &str| nav(&ws, "main.wcl", Some(site)).unwrap()["container"].clone();

        op(
            &ws,
            serde_json::json!({
                "entry": "main.wcl", "op": "add_page", "site": "blog",
                "name": "faq", "title": "FAQ",
                "nav": { "container_span": container("blog")["span"], "kind": "chapter" },
            }),
        )
        .expect("add_page");
        let text = read(&ws, "main.wcl");
        assert!(
            text.contains("page faq {\n  sites = [:blog]"),
            "the new page joins the site whose nav was edited:\n{text}"
        );

        // The document still builds — which is the whole point of tagging.
        assert!(
            wcl_wdoc::open_doc_for_edit_with_overlay(
                &ws.root_dir().join("main.wcl"),
                Default::default()
            )
            .is_ok(),
            "the entry should still open cleanly:\n{text}"
        );

        // No site named ⇒ refused with the choices, rather than writing a
        // page the build will reject.
        let e = op(
            &ws,
            serde_json::json!({
                "entry": "main.wcl", "op": "add_page",
                "name": "loose", "title": "Loose",
            }),
        )
        .expect_err("an untagged page must be refused");
        assert!(e.contains("docs") && e.contains("blog"), "{e}");
        assert!(!read(&ws, "main.wcl").contains("page loose"), "not written");
    }

    /// A single-site document is unaffected — no `sites` field is written.
    #[test]
    fn add_page_writes_no_sites_in_a_single_site_document() {
        let doc = "import <wdoc.wcl>\n\nsite docs {\n  title = \"Docs\"\n  toc {\n    \
                   chapter \"Start\" {\n      page = index\n    }\n  }\n}\n\npage index {\n  \
                   title = \"Hi\"\n\n  h1 \"Hello\"\n}\n";
        let (_td, ws) = workspace_built_by(|root| {
            std::fs::write(root.join("main.wcl"), doc).unwrap();
        });
        op(
            &ws,
            serde_json::json!({
                "entry": "main.wcl", "op": "add_page", "site": "docs",
                "name": "faq", "title": "FAQ",
            }),
        )
        .expect("add_page");
        let text = read(&ws, "main.wcl");
        assert!(text.contains("page faq"), "{text}");
        assert!(!text.contains("sites = "), "no sites field:\n{text}");
    }

    /// The id-addressed related ops reach nested sub-indexes (the owning
    /// file used to be resolved with a top-level-only scan, so these all
    /// errored for sub-index ids).
    #[test]
    fn op_targets_sub_index() {
        let (_td, ws) = workspace_built_by(write_mini_wskill_nested);

        for body in [
            serde_json::json!({
                "entry": "main.wcl", "op": "pin_unit",
                "index_id": "lang_sub", "unit_id": "alpha",
            }),
            serde_json::json!({
                "entry": "main.wcl", "op": "reorder_children",
                "index_id": "lang_sub", "order": ["alpha", "beta"],
            }),
            serde_json::json!({
                "entry": "main.wcl", "op": "unpin_unit",
                "index_id": "lang_sub", "unit_id": "beta",
            }),
        ] {
            op(&ws, body.clone()).unwrap_or_else(|e| panic!("{body}: {e}"));
        }

        // The writes landed on the NESTED list; the top-level one is
        // untouched.
        let text = read(&ws, "data/indexes.wcl");
        assert!(text.contains("related = [alpha]\n"), "{text}");
        assert!(
            text.matches("related = [alpha]").count() == 2,
            "both levels hold exactly `alpha`: {text}"
        );
    }

    /// Reordering the syllabus rewrites each lesson's `n` — the course has no
    /// `related` list to permute — and pinning has no meaning there.
    #[test]
    fn syllabus_reorder_rewrites_lesson_order() {
        let (_td, ws) = workspace_built_by(write_mini_wskill_training);

        op(
            &ws,
            serde_json::json!({
                "entry": "main.wcl",
                "op": "reorder_children",
                "index_id": "__course",
                "order": ["second", "first"],
            }),
        )
        .expect("reorder");
        // `n` carries the order, so the blocks stay where they were authored.
        let text = read(&ws, "data/lessons.wcl");
        let at = |id: &str| text.find(id).unwrap();
        assert!(
            at("lesson first") < at("lesson second"),
            "source order is untouched"
        );
        let n_of = |id: &str| {
            let rest = &text[at(&format!("lesson {id}"))..];
            let at_n = rest.find("n = ").expect("an n field");
            rest[at_n + 4..].trim_start().chars().next().unwrap()
        };
        assert_eq!(n_of("second"), '1', "second moved to the front: {text}");
        assert_eq!(n_of("first"), '2', "first moved to the back: {text}");

        // A permutation is required, so a stale client can't drop a lesson.
        assert!(
            op(
                &ws,
                serde_json::json!({
                    "entry": "main.wcl", "op": "reorder_children",
                    "index_id": "__course", "order": ["first"],
                }),
            )
            .is_err()
        );

        // Pinning into a course is meaningless — every lesson is already in it.
        assert!(
            op(
                &ws,
                serde_json::json!({
                    "entry": "main.wcl", "op": "pin_unit",
                    "index_id": "__course", "unit_id": "first",
                }),
            )
            .is_err()
        );
    }
}
