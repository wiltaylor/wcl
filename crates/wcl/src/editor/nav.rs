//! Site-navigation model endpoints for the editor's Design mode.
//!
//! `GET /api/nav` projects the selected site's menu structure into an
//! editable tree where every entry carries its **source binding** (declaring
//! file + byte span), and `POST /api/nav/op` applies structural edits
//! through the same parse → mutate → [`crate::edit::commit`] pipeline as
//! `/api/block/ops`. The model is site-type-aware:
//!
//! - **wskill book** — built from the data model, not the toc repeaters:
//!   `index` blocks are the sections, an index's `related` id list is its
//!   child entries (source order = menu order), nested `index` children are
//!   sub-branches. Ops rewrite `index` blocks and `related` lists.
//! - **plain book** — the `site.toc` `chapter` tree, literal chapters only;
//!   a `wdoc_repeater` shows as a read-only synthetic entry.
//! - **website** — the `site.menu` `item` tree, same mechanics.
//! - **presentation** — the `site.deck` `section` / `slide` grid.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::State;
use axum::http::{StatusCode, Uri};
use axum::response::Response;

use wcl_lang::ast::{self, Expr, Item};
use wcl_lang::{Document, Span, Value, edit as ast_edit, format as wcl_format, parse_for_edit};

use super::kinds::{self, KindModel};
use super::preview::Sessions;
use super::util::{first_label, value_string};
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
    let model = KindModel::new(&doc);
    let wskill = kinds::is_wskill(&doc);
    let site_type = kinds::site_kind(&doc, site);

    let pages = declared_pages(ws, &doc, &entry_abs);
    if wskill && site_type == "book" {
        let (nav, units) = wskill_nav(ws, &model, &doc, &entry_abs)?;
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
// The wskill model: index blocks + related lists
// ---------------------------------------------------------------------------

/// The page-name prefix a wskill book's repeaters use for a unit kind.
fn page_prefix(kind: &str) -> &str {
    match kind {
        // The book template names procedure pages `process_<id>`.
        "procedure" => "process",
        other => other,
    }
}

fn wskill_nav(
    ws: &Workspace,
    model: &KindModel<'_>,
    doc: &Document,
    entry_abs: &Path,
) -> Result<(Vec<serde_json::Value>, Vec<serde_json::Value>), String> {
    // Unit registry: id → (kind, title, file, span). Kinds come from the
    // same kind model the add-unit palette reads.
    let kind_names: Vec<String> = model.unit_kind_names();
    let mut units: HashMap<String, (String, String, PathBuf, Span)> = HashMap::new();
    let mut unit_order: Vec<String> = Vec::new();
    for (path, b) in doc.blocks_with_source() {
        let kind = b.kind().to_string();
        if !kind_names.contains(&kind) {
            continue;
        }
        let Some(id) = first_label(&b) else { continue };
        let title = ["name", "title"]
            .iter()
            .find_map(|f| b.field(f).and_then(|f| f.value().ok().cloned()))
            .as_ref()
            .map(value_string)
            .unwrap_or_else(|| id.clone());
        let file = path
            .map(Path::to_path_buf)
            .unwrap_or_else(|| entry_abs.to_path_buf());
        if !units.contains_key(&id) {
            unit_order.push(id.clone());
        }
        units.insert(id.clone(), (kind, title, file, b.span()));
    }

    let mut nav: Vec<serde_json::Value> = Vec::new();
    for (path, b) in doc.blocks_with_source() {
        if b.kind() != "index" {
            continue;
        }
        let file = path.unwrap_or(entry_abs);
        nav.push(index_entry(ws, &b, file, &units));
    }

    let units_json: Vec<serde_json::Value> = unit_order
        .iter()
        .filter_map(|id| {
            let (kind, title, file, span) = units.get(id)?;
            Some(serde_json::json!({
                "id": id,
                "kind": kind,
                "title": title,
                "page": format!("{}_{id}", page_prefix(kind)),
                "source": source_binding(ws, file, *span),
            }))
        })
        .collect();
    Ok((nav, units_json))
}

fn index_entry(
    ws: &Workspace,
    b: &wcl_lang::Block<'_>,
    file: &Path,
    units: &HashMap<String, (String, String, PathBuf, Span)>,
) -> serde_json::Value {
    let id = first_label(b).unwrap_or_default();
    let title = b
        .field("name")
        .and_then(|f| f.value().ok().cloned())
        .as_ref()
        .map(value_string)
        .unwrap_or_else(|| id.clone());
    // A content index (with a body) is its own page; a nav index is a
    // heading whose children are the pinned unit pages.
    let has_body = b.blocks().any(|c| c.kind() == "body");
    let mut children: Vec<serde_json::Value> = Vec::new();
    if let Some(related) = b.field("related").and_then(|f| f.value().ok().cloned())
        && let Value::List(ids) = related
    {
        for rid in ids.iter().map(value_string) {
            children.push(match units.get(&rid) {
                Some((kind, title, ufile, uspan)) => serde_json::json!({
                    "kind": "unit",
                    "unit": { "kind": kind, "id": rid },
                    "title": title,
                    "page": format!("{}_{rid}", page_prefix(kind)),
                    "source": source_binding(ws, ufile, *uspan),
                }),
                None => serde_json::json!({
                    "kind": "unit",
                    "unit": { "kind": null, "id": rid },
                    "title": rid,
                    "page": null,
                    "missing": true,
                }),
            });
        }
    }
    for child in b.blocks().filter(|c| c.kind() == "index") {
        children.push(index_entry(ws, &child, file, units));
    }
    serde_json::json!({
        "kind": "index",
        "id": id,
        "title": title,
        "page": has_body.then(|| format!("index_{id}")),
        "source": source_binding(ws, file, b.span()),
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
/// - `pin_unit { index_id, unit_id }` / `unpin_unit { index_id, unit_id }`
/// - `reorder_children { index_id, order: [ids] }` — rewrite a `related`
///   list to exactly `order`
///
/// The wskill model's `index` blocks are structural, so they have their own
/// id-addressed op family (spans shift under every reformat; ids don't):
///
/// - `create_index { id, name, parent_id? }` — a new `index` block, either
///   placed by convention beside the existing ones or nested in `parent_id`
/// - `delete_index { index_id }` — remove it and its subtree
/// - `move_index { index_id, dir }` — swap with the adjacent `index` sibling
/// - `promote_index { index_id }` — a sub-index becomes its parent's next
///   sibling; `demote_index { index_id }` nests it under the index above
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
            let down = match crate::edit::str_field(v, "dir")? {
                "down" => true,
                "up" => false,
                other => return Err(format!("bad move dir `{other}`")),
            };
            edit_file(&entry_abs, &file, |src| {
                if !ast_edit::move_block_by_span(&mut src.items, span, down) {
                    return Err("the entry is already at the edge".into());
                }
                Ok(())
            })
        }
        "add_section" => add_section(ws, &entry_abs, v),
        "add_page" => add_page(&entry_abs, v),
        // A training view's levels are the course itself, ordered by each
        // lesson's `n` rather than pinned by a `related` list — so they
        // reorder by rewriting `n`, and have nothing to pin or unpin.
        "pin_unit" | "unpin_unit" if is_syllabus_level(&entry_abs, v)? => Err(
            "a course has no pins — a lesson belongs to it by existing; reorder it instead"
                .to_string(),
        ),
        "pin_unit" => related_op(&entry_abs, v, RelatedOp::Pin),
        "unpin_unit" => related_op(&entry_abs, v, RelatedOp::Unpin),
        "reorder_children" if is_syllabus_level(&entry_abs, v)? => syllabus_reorder(&entry_abs, v),
        "reorder_children" => related_op(&entry_abs, v, RelatedOp::Reorder),
        "create_index" => create_index(ws, &entry_abs, v),
        "delete_index" => delete_index(&entry_abs, v),
        "move_index" => move_index(&entry_abs, v),
        "promote_index" => promote_index(&entry_abs, v),
        "demote_index" => demote_index(&entry_abs, v),
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
        // Wskill: a new `index` block appended to the target file.
        Some(id) => {
            let file = crate::edit::str_field(v, "file")?;
            let file_abs = ws.abs(file)?;
            let block = ast_edit::build_block(
                "index",
                &[],
                vec![Expr::Identifier(id.to_string(), Span::new(0, 0))],
                vec![("name".to_string(), ast_edit::string_literal_expr(title))],
            );
            edit_file(entry_abs, &file_abs, |src| {
                ast_edit::append_top_level_block(src, block);
                Ok(())
            })
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

enum RelatedOp {
    Pin,
    Unpin,
    Reorder,
}

/// Does this block's subtree contain an `index` with the given id?
/// (Sub-indexes nest inside their parent block, so the owning *file* must
/// be found by searching recursively — the block itself is then relocated
/// by the equally recursive `find_block_by_kind_label`.)
fn subtree_has_index(b: &wcl_lang::Block<'_>, id: &str) -> bool {
    (b.kind() == "index" && first_label(b).as_deref() == Some(id))
        || b.blocks().any(|c| subtree_has_index(&c, id))
}

/// The file declaring the `index` with `id` — sub-indexes nest inside their
/// parent block, so the search recurses (the block itself is relocated by the
/// equally recursive [`super::util::find_block_by_kind_label`]). Index ids
/// are assumed document-unique; first match wins.
fn index_file_in(doc: &Document, entry_abs: &Path, index_id: &str) -> Result<PathBuf, String> {
    doc.blocks_with_source()
        .find(|(_, b)| subtree_has_index(b, index_id))
        .map(|(p, _)| {
            p.map(Path::to_path_buf)
                .unwrap_or_else(|| entry_abs.to_path_buf())
        })
        .ok_or_else(|| format!("no `index` with id `{index_id}`"))
}

/// [`index_file_in`] for a caller with no document open yet.
fn index_file(entry_abs: &Path, index_id: &str) -> Result<PathBuf, String> {
    let doc = wcl_wdoc::open_doc_for_edit(entry_abs).map_err(super::err_str)?;
    index_file_in(&doc, entry_abs, index_id)
}

/// Rewrite an index's `related` list: pin (append), unpin (remove), or
/// reorder (replace with the posted order, which must be a permutation).
/// `index_id` may name an index at any nesting depth.
fn related_op(
    entry_abs: &Path,
    v: &serde_json::Value,
    op: RelatedOp,
) -> Result<serde_json::Value, String> {
    let index_id = crate::edit::str_field(v, "index_id")?;
    let ifile = index_file(entry_abs, index_id)?;

    let ident = |s: &str| Expr::Identifier(s.to_string(), Span::new(0, 0));
    edit_file(entry_abs, &ifile, |src| {
        let block = super::util::find_block_by_kind_label(&mut src.items, "index", index_id)
            .ok_or_else(|| format!("could not relocate index `{index_id}`"))?;
        let current: Vec<String> = block
            .items
            .iter()
            .find_map(|it| match it {
                Item::Field(f) if f.name == "related" => Some(&f.expr),
                _ => None,
            })
            .map(|e| match e {
                Expr::ListLit { elements, .. } => elements
                    .iter()
                    .filter_map(|e| match e {
                        Expr::Identifier(s, _) => Some(s.clone()),
                        Expr::Utf8(s) | Expr::Ascii(s) => Some(s.clone()),
                        _ => None,
                    })
                    .collect(),
                _ => Vec::new(),
            })
            .unwrap_or_default();
        let next: Vec<String> = match op {
            RelatedOp::Pin => {
                let unit_id = crate::edit::str_field(v, "unit_id")?;
                if current.iter().any(|s| s == unit_id) {
                    return Err(format!("`{unit_id}` is already pinned"));
                }
                let mut n = current;
                n.push(unit_id.to_string());
                n
            }
            RelatedOp::Unpin => {
                let unit_id = crate::edit::str_field(v, "unit_id")?;
                if !current.iter().any(|s| s == unit_id) {
                    return Err(format!("`{unit_id}` is not pinned here"));
                }
                current.into_iter().filter(|s| s != unit_id).collect()
            }
            RelatedOp::Reorder => {
                let order: Vec<String> = v
                    .get("order")
                    .and_then(serde_json::Value::as_array)
                    .ok_or("missing `order`")?
                    .iter()
                    .filter_map(|s| s.as_str().map(str::to_string))
                    .collect();
                let mut sorted_a = current.clone();
                let mut sorted_b = order.clone();
                sorted_a.sort();
                sorted_b.sort();
                if sorted_a != sorted_b {
                    return Err("`order` must be a permutation of the current list".into());
                }
                order
            }
        };
        ast_edit::set_or_insert_field(
            block,
            "related",
            Expr::ListLit {
                elem_trivia: next.iter().map(|_| Default::default()).collect(),
                elements: next.iter().map(|s| ident(s)).collect(),
                trailing_trivia: Vec::new(),
                span: Span::new(0, 0),
            },
        );
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Index structure: create / delete / reorder / promote / demote
// ---------------------------------------------------------------------------
//
// The pin ops above edit an index's CONTENTS; these edit the index tree
// itself — the wskill book's sidebar headings. All are id-addressed (spans
// shift under every reformat) and rewrite exactly one file: an index and its
// sub-indexes are one block subtree, so nesting never crosses files.

/// Is this AST item the `index` block labelled `id`?
fn is_index_item(it: &Item, id: &str) -> bool {
    matches!(it, Item::Block(b)
        if b.kind == "index" && super::util::ast_label(b).as_deref() == Some(id))
}

/// Does this AST block's subtree declare an `index` with `id`?
fn ast_subtree_has_index(b: &ast::Block, id: &str) -> bool {
    (b.kind == "index" && super::util::ast_label(b).as_deref() == Some(id))
        || b.items
            .iter()
            .any(|it| matches!(it, Item::Block(c) if ast_subtree_has_index(c, id)))
}

/// The items list OWNING the `index` block with `id`, and its position in it
/// — the handle every structural op needs (its siblings are right there).
fn index_slot<'a>(items: &'a mut Vec<Item>, id: &str) -> Option<(&'a mut Vec<Item>, usize)> {
    if let Some(i) = items.iter().position(|it| is_index_item(it, id)) {
        return Some((items, i));
    }
    // Descend into the one child whose subtree holds it (chosen immutably,
    // so the mutable reborrow below is the only live borrow).
    let child = items
        .iter()
        .position(|it| matches!(it, Item::Block(b) if ast_subtree_has_index(b, id)))?;
    match &mut items[child] {
        Item::Block(b) => index_slot(&mut b.items, id),
        _ => None,
    }
}

/// The id of the `index` block whose DIRECT children include `id`, if any —
/// `None` means `id` is a top-level index.
fn parent_index_id(items: &[Item], id: &str) -> Option<String> {
    for it in items {
        let Item::Block(b) = it else { continue };
        if b.kind == "index"
            && b.items.iter().any(|c| is_index_item(c, id))
            && let Some(label) = super::util::ast_label(b)
        {
            return Some(label);
        }
        if let Some(found) = parent_index_id(&b.items, id) {
            return Some(found);
        }
    }
    None
}

/// The position of the adjacent `index` sibling in `dir`, skipping anything
/// else at that level (fields, a `body` block, units sharing the file).
fn index_sibling(items: &[Item], pos: usize, down: bool) -> Option<usize> {
    let is_index = |i: &usize| matches!(&items[*i], Item::Block(b) if b.kind == "index");
    if down {
        (pos + 1..items.len()).find(is_index)
    } else {
        (0..pos).rev().find(is_index)
    }
}

fn relocate_err(id: &str) -> String {
    format!("could not relocate index `{id}`")
}

/// A new `index` block: nested inside `parent_id`, else placed by the same
/// convention `unit_create` uses (beside the existing indexes).
fn create_index(
    ws: &Workspace,
    entry_abs: &Path,
    v: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let id = crate::edit::str_field(v, "id")?;
    if !super::util::is_identifier(id) {
        return Err(format!(
            "`{id}` is not a valid id (letters, digits, `_`, not starting with a digit)"
        ));
    }
    let name = crate::edit::str_field(v, "name")?;
    let doc = wcl_wdoc::open_doc_for_edit(entry_abs).map_err(super::err_str)?;
    if index_file_in(&doc, entry_abs, id).is_ok() {
        return Err(format!("an `index` with id `{id}` already exists"));
    }
    let block = ast_edit::build_block(
        "index",
        &[],
        vec![Expr::Identifier(id.to_string(), Span::new(0, 0))],
        vec![("name".to_string(), ast_edit::string_literal_expr(name))],
    );

    match v
        .get("parent_id")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
    {
        Some(parent) => {
            drop(doc);
            let file = index_file(entry_abs, parent)?;
            edit_file(entry_abs, &file, |src| {
                if let Some(gp) = parent_index_id(&src.items, parent) {
                    return Err(format!(
                        "`{parent}` is itself nested under `{gp}` — sub-indexes nest one level deep"
                    ));
                }
                let pblock = super::util::find_block_by_kind_label(&mut src.items, "index", parent)
                    .ok_or_else(|| relocate_err(parent))?;
                pblock.items.push(Item::Block(block));
                Ok(())
            })
        }
        None => {
            // Placed and staged inside this scope so the model's borrow
            // ends here; the document itself is released just below, before
            // the commit rewrites the files it was read from.
            let (file, changes) = {
                let model = KindModel::new(&doc);
                let placement = super::placement::place_unit(&model, &doc, entry_abs, "index")?;
                let mut changes: Vec<(PathBuf, String)> = Vec::new();
                let file = super::placement::write_new_block(
                    placement,
                    id,
                    block,
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
    }
}

/// Remove an index and everything nested in it. Its pins are just ids in a
/// `related` list, so nothing dangles elsewhere — the units stay put.
fn delete_index(entry_abs: &Path, v: &serde_json::Value) -> Result<serde_json::Value, String> {
    let id = crate::edit::str_field(v, "index_id")?;
    let file = index_file(entry_abs, id)?;
    edit_file(entry_abs, &file, |src| {
        let (items, pos) = index_slot(&mut src.items, id).ok_or_else(|| relocate_err(id))?;
        items.remove(pos);
        Ok(())
    })
}

/// Swap an index with its adjacent `index` sibling. Top-level order is
/// document order, so an index at the edge of ITS file can't move further
/// here — the files' import order decides that.
fn move_index(entry_abs: &Path, v: &serde_json::Value) -> Result<serde_json::Value, String> {
    let id = crate::edit::str_field(v, "index_id")?;
    let down = match crate::edit::str_field(v, "dir")? {
        "down" => true,
        "up" => false,
        other => return Err(format!("bad move dir `{other}`")),
    };
    let file = index_file(entry_abs, id)?;
    edit_file(entry_abs, &file, |src| {
        let (items, pos) = index_slot(&mut src.items, id).ok_or_else(|| relocate_err(id))?;
        let target = index_sibling(items, pos, down).ok_or_else(|| {
            format!(
                "`{id}` is already the {} index at its level",
                if down { "last" } else { "first" }
            )
        })?;
        items.swap(pos, target);
        Ok(())
    })
}

/// Lift a sub-index out to its parent's level, placed right after it.
fn promote_index(entry_abs: &Path, v: &serde_json::Value) -> Result<serde_json::Value, String> {
    let id = crate::edit::str_field(v, "index_id")?;
    let file = index_file(entry_abs, id)?;
    edit_file(entry_abs, &file, |src| {
        let parent = parent_index_id(&src.items, id)
            .ok_or_else(|| format!("`{id}` is already a top-level index"))?;
        let (items, pos) = index_slot(&mut src.items, id).ok_or_else(|| relocate_err(id))?;
        let block = items.remove(pos);
        let (pitems, ppos) =
            index_slot(&mut src.items, &parent).ok_or_else(|| relocate_err(&parent))?;
        pitems.insert(ppos + 1, block);
        Ok(())
    })
}

/// Nest an index under the one above it. Sub-indexes render one level deep,
/// so an already-nested index refuses rather than building a tree nothing
/// projects.
fn demote_index(entry_abs: &Path, v: &serde_json::Value) -> Result<serde_json::Value, String> {
    let id = crate::edit::str_field(v, "index_id")?;
    let file = index_file(entry_abs, id)?;
    edit_file(entry_abs, &file, |src| {
        if let Some(parent) = parent_index_id(&src.items, id) {
            return Err(format!(
                "`{id}` is already nested under `{parent}` — sub-indexes nest one level deep"
            ));
        }
        let (items, pos) = index_slot(&mut src.items, id).ok_or_else(|| relocate_err(id))?;
        let target = index_sibling(items, pos, false).ok_or_else(|| {
            format!("no index above `{id}` to nest it under — move it down first")
        })?;
        let block = items.remove(pos);
        match &mut items[target] {
            Item::Block(b) => b.items.push(block),
            _ => return Err(relocate_err(id)),
        }
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// The training syllabus: course order as `n`, not a `related` list
// ---------------------------------------------------------------------------

/// Whether `index_id` names a syllabus level — the synthetic course node, or a
/// `module` block. Both order their lessons by `n`.
fn is_syllabus_level(entry_abs: &Path, v: &serde_json::Value) -> Result<bool, String> {
    let index_id = crate::edit::str_field(v, "index_id")?;
    if index_id == super::graph::SYLLABUS_ID {
        return Ok(true);
    }
    let doc = wcl_wdoc::open_doc_for_edit(entry_abs).map_err(super::err_str)?;
    Ok(doc
        .blocks()
        .any(|b| b.kind() == "module" && first_label(&b).as_deref() == Some(index_id)))
}

/// Reorder a syllabus level by rewriting its lessons' `n` to `1..=len` in the
/// posted order. `order` must be a permutation of the level's current lessons,
/// so a stale client can't drop one. Lessons may live in several files; every
/// touched file lands in ONE commit, so the course is never half-renumbered.
fn syllabus_reorder(entry_abs: &Path, v: &serde_json::Value) -> Result<serde_json::Value, String> {
    let index_id = crate::edit::str_field(v, "index_id")?;
    let order: Vec<String> = v
        .get("order")
        .and_then(serde_json::Value::as_array)
        .ok_or("missing `order`")?
        .iter()
        .filter_map(|s| s.as_str().map(str::to_string))
        .collect();

    let doc = wcl_wdoc::open_doc_for_edit(entry_abs).map_err(super::err_str)?;
    let (top, modules) = super::graph::course_structure(&doc);
    let current: Vec<String> = if index_id == super::graph::SYLLABUS_ID {
        top
    } else {
        modules
            .into_iter()
            .find(|m| m.id == index_id)
            .map(|m| m.lessons)
            .ok_or_else(|| format!("no `module` with id `{index_id}`"))?
    };
    let (mut a, mut b) = (current.clone(), order.clone());
    a.sort();
    b.sort();
    if a != b {
        return Err("`order` must be a permutation of the level's lessons".into());
    }

    // Every lesson's declaring file + span, so the renumber can address them
    // wherever they were authored.
    let mut sites: HashMap<String, (PathBuf, Span)> = HashMap::new();
    for (path, blk) in doc.blocks_with_source() {
        if blk.kind() != "lesson" {
            continue;
        }
        if let Some(id) = first_label(&blk) {
            let file = path
                .map(Path::to_path_buf)
                .unwrap_or_else(|| entry_abs.to_path_buf());
            sites.entry(id).or_insert((file, blk.span()));
        }
    }

    // Group the renumbering by file, then mutate each file once.
    let mut per_file: HashMap<PathBuf, Vec<(Span, u32)>> = HashMap::new();
    for (i, id) in order.iter().enumerate() {
        let (file, span) = sites
            .get(id)
            .ok_or_else(|| format!("no `lesson` with id `{id}`"))?;
        per_file
            .entry(file.clone())
            .or_default()
            .push((*span, i as u32 + 1));
    }

    let mut writes: Vec<(PathBuf, String)> = Vec::new();
    for (file, mut spans) in per_file {
        let text = crate::edit::read(&file)?;
        let mut src = parse_for_edit(&text, file.display().to_string()).map_err(super::err_str)?;
        // Descending span order keeps every span valid while mutating.
        spans.sort_by_key(|(sp, _)| std::cmp::Reverse(sp.start));
        for (span, n) in spans {
            let blk = ast_edit::find_block_by_span(&mut src.items, span)
                .ok_or("a lesson moved on disk — reload the graph")?;
            ast_edit::set_or_insert_field(blk, "n", Expr::U32(n));
        }
        writes.push((file, wcl_format::to_source(&src)));
    }
    crate::edit::commit(entry_abs, writes)?;
    Ok(serde_json::json!({ "ok": true }))
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
