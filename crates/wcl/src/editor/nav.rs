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

use super::blocks::{first_label, is_wskill, site_kind, unit_kinds, value_string};
use super::{EditorState, run_blocking};
use crate::serve::{json_error, parse_json_body, query_param, sandboxed};

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
        nav(&state2, &entry, site.as_deref())
    })
    .await
}

fn nav(state: &EditorState, entry: &str, site: Option<&str>) -> Result<serde_json::Value, String> {
    let entry_abs = sandboxed(&state.root_dir, &state.root_dir.join(entry))
        .ok_or_else(|| format!("file outside the served tree: {entry}"))?;
    let doc = wcl_wdoc::open_doc_for_edit(&entry_abs).map_err(|e| e.to_string())?;
    let wskill = is_wskill(&doc);
    let site_type = site_kind(&doc, site);

    let pages = declared_pages(state, &doc, &entry_abs);
    if wskill && site_type == "book" {
        let (nav, units) = wskill_nav(state, &doc, &entry_abs)?;
        return Ok(serde_json::json!({
            "ok": true,
            "site_type": site_type,
            "wskill": true,
            "nav": nav,
            "units": units,
            "pages": pages,
        }));
    }

    let (nav, container) = static_site_nav(state, &entry_abs, site, site_type)?;
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
fn declared_pages(state: &EditorState, doc: &Document, entry_abs: &Path) -> Vec<serde_json::Value> {
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
                "source": source_binding(state, file, b.span()),
            }))
        })
        .collect()
}

fn source_binding(state: &EditorState, file: &Path, span: Span) -> serde_json::Value {
    let rel = std::fs::canonicalize(file)
        .unwrap_or_else(|_| file.to_path_buf())
        .strip_prefix(&state.root_dir)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| file.display().to_string());
    serde_json::json!({
        "file": rel,
        "span": { "start": span.start, "end": span.end },
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
    state: &EditorState,
    doc: &Document,
    entry_abs: &Path,
) -> Result<(Vec<serde_json::Value>, Vec<serde_json::Value>), String> {
    // Unit registry: id → (kind, title, file, span). Kinds come from the
    // same palette introspection the add-unit form uses.
    let kind_names: Vec<String> = unit_kinds(doc)
        .iter()
        .filter_map(|k| k.get("kind").and_then(serde_json::Value::as_str))
        .filter(|k| *k != "index")
        .map(str::to_string)
        .collect();
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
        nav.push(index_entry(state, &b, file, &units));
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
                "source": source_binding(state, file, *span),
            }))
        })
        .collect();
    Ok((nav, units_json))
}

fn index_entry(
    state: &EditorState,
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
                    "source": source_binding(state, ufile, *uspan),
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
        children.push(index_entry(state, &child, file, units));
    }
    serde_json::json!({
        "kind": "index",
        "id": id,
        "title": title,
        "page": has_body.then(|| format!("index_{id}")),
        "source": source_binding(state, file, b.span()),
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
    state: &EditorState,
    entry_abs: &Path,
    site: Option<&str>,
    site_type: &str,
) -> Result<(Vec<serde_json::Value>, serde_json::Value), String> {
    let text = crate::edit::read(entry_abs)?;
    let src = parse_for_edit(&text, entry_abs.display().to_string()).map_err(|e| e.to_string())?;
    let site_block = src
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Block(b) if b.kind == "site" => Some(b),
            _ => None,
        })
        .find(|b| match site {
            Some(name) => super::blocks::ast_label(b).as_deref() == Some(name),
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
            Item::Block(b) => Some(static_entry(state, b, entry_abs)),
            _ => None,
        })
        .collect();
    Ok((nav, source_binding(state, entry_abs, container.span)))
}

/// One literal nav entry (`chapter` / `item` / `section` / `slide`) with its
/// source binding; a `wdoc_repeater` becomes a read-only synthetic entry.
fn static_entry(state: &EditorState, b: &ast::Block, entry_abs: &Path) -> serde_json::Value {
    if b.kind == "wdoc_repeater" {
        return serde_json::json!({
            "kind": "generated",
            "title": "(generated entries)",
            "synthetic": true,
            "source": source_binding(state, entry_abs, b.span),
        });
    }
    let title = super::blocks::ast_label(b);
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
            Item::Block(c) => Some(static_entry(state, c, entry_abs)),
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
        "source": source_binding(state, entry_abs, b.span),
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
pub(super) async fn handle_nav_op(State(state): State<Arc<EditorState>>, body: String) -> Response {
    let v = match parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    let state2 = Arc::clone(&state);
    run_blocking(move || nav_op(&state2, &v)).await
}

fn nav_op(state: &EditorState, v: &serde_json::Value) -> Result<serde_json::Value, String> {
    let entry = crate::edit::str_field(v, "entry")?;
    let entry_abs = sandboxed(&state.root_dir, &state.root_dir.join(entry))
        .ok_or_else(|| format!("file outside the served tree: {entry}"))?;
    let op = crate::edit::str_field(v, "op")?;
    match op {
        "rename" => {
            let (file, span) = op_target(state, v)?;
            let title = crate::edit::str_field(v, "title")?;
            edit_file(&entry_abs, &file, |src| {
                let block = ast_edit::find_block_by_span(&mut src.items, span)
                    .ok_or_else(super::blocks::stale_span)?;
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
            let (file, span) = op_target(state, v)?;
            edit_file(&entry_abs, &file, |src| {
                if !ast_edit::remove_block_by_span(&mut src.items, span) {
                    return Err(super::blocks::stale_span());
                }
                Ok(())
            })
        }
        "move" => {
            let (file, span) = op_target(state, v)?;
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
        "add_section" => add_section(state, &entry_abs, v),
        "add_page" => add_page(&entry_abs, v),
        "pin_unit" => related_op(&entry_abs, v, RelatedOp::Pin),
        "unpin_unit" => related_op(&entry_abs, v, RelatedOp::Unpin),
        "reorder_children" => related_op(&entry_abs, v, RelatedOp::Reorder),
        other => Err(format!("unknown nav op `{other}`")),
    }
}

/// The op's target binding: `file` (repo-relative) + `span`.
fn op_target(state: &EditorState, v: &serde_json::Value) -> Result<(PathBuf, Span), String> {
    let file = crate::edit::str_field(v, "file")?;
    let file_abs = sandboxed(&state.root_dir, &state.root_dir.join(file))
        .ok_or_else(|| format!("file outside the served tree: {file}"))?;
    let span = super::blocks::span_field(v, "span")?;
    Ok((file_abs, span))
}

/// Parse `file`, apply `mutate`, print, commit against `entry_abs`.
fn edit_file(
    entry_abs: &Path,
    file: &Path,
    mutate: impl FnOnce(&mut ast::Source) -> Result<(), String>,
) -> Result<serde_json::Value, String> {
    let text = crate::edit::read(file)?;
    let mut src = parse_for_edit(&text, file.display().to_string()).map_err(|e| e.to_string())?;
    mutate(&mut src)?;
    let new_text = wcl_format::to_source(&src);
    crate::edit::commit(entry_abs, vec![(file.to_path_buf(), new_text)])?;
    Ok(serde_json::json!({ "ok": true }))
}

fn add_section(
    state: &EditorState,
    entry_abs: &Path,
    v: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let title = crate::edit::str_field(v, "title")?;
    match v.get("id").and_then(serde_json::Value::as_str) {
        // Wskill: a new `index` block appended to the target file.
        Some(id) => {
            let file = crate::edit::str_field(v, "file")?;
            let file_abs = sandboxed(&state.root_dir, &state.root_dir.join(file))
                .ok_or_else(|| format!("file outside the served tree: {file}"))?;
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
            let (file, span) = op_target(state, v)?;
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
                    .ok_or_else(super::blocks::stale_span)?;
                ast_edit::insert_block_at_index(&mut parent.items, usize::MAX, block);
                Ok(())
            })
        }
    }
}

fn add_page(entry_abs: &Path, v: &serde_json::Value) -> Result<serde_json::Value, String> {
    let name = crate::edit::str_field(v, "name")?;
    if !super::blocks::is_identifier(name) {
        return Err(format!("`{name}` is not a valid page name"));
    }
    let title = crate::edit::str_field(v, "title")?;
    let page = ast_edit::build_block(
        "page",
        &[],
        vec![Expr::Identifier(name.to_string(), Span::new(0, 0))],
        vec![("title".to_string(), ast_edit::string_literal_expr(title))],
    );
    // The nav entry that links the page (optional — a page can be added
    // unlinked): `{ file, container_span, kind }`.
    let nav = v.get("nav").filter(|n| !n.is_null());
    edit_file(entry_abs, entry_abs, |src| {
        ast_edit::append_top_level_block(src, page);
        if let Some(nav) = nav {
            let span = super::blocks::span_field(nav, "container_span")?;
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
                .ok_or_else(super::blocks::stale_span)?;
            ast_edit::insert_block_at_index(&mut parent.items, usize::MAX, entry_block);
        }
        Ok(())
    })
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

/// Rewrite an index's `related` list: pin (append), unpin (remove), or
/// reorder (replace with the posted order, which must be a permutation).
/// `index_id` may name an index at any nesting depth; index ids are
/// assumed document-unique (first match wins).
fn related_op(
    entry_abs: &Path,
    v: &serde_json::Value,
    op: RelatedOp,
) -> Result<serde_json::Value, String> {
    let index_id = crate::edit::str_field(v, "index_id")?;
    let doc = wcl_wdoc::open_doc_for_edit(entry_abs).map_err(|e| e.to_string())?;
    let ifile = doc
        .blocks_with_source()
        .find(|(_, b)| subtree_has_index(b, index_id))
        .map(|(p, _)| {
            p.map(Path::to_path_buf)
                .unwrap_or_else(|| entry_abs.to_path_buf())
        })
        .ok_or_else(|| format!("no `index` with id `{index_id}`"))?;
    drop(doc);

    let ident = |s: &str| Expr::Identifier(s.to_string(), Span::new(0, 0));
    edit_file(entry_abs, &ifile, |src| {
        let block = super::blocks::find_block_by_kind_label(&mut src.items, "index", index_id)
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
