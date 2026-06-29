//! WYSIWYG editor backend for `wcl wdoc serve --edit`.
//!
//! The dev server injects a JS client ([`EDIT_CLIENT_JS`], served at
//! `/__wdoc_edit.js`) that lets the author edit a rendered page in place and
//! manage schema-defined data objects. The client talks to the read/write
//! endpoints implemented here; every write goes through the same edit pipeline
//! `wcl set` uses — [`parse_for_edit`] → mutate the owned AST by byte span →
//! [`wcl_format::to_source`] → [`crate::verify_reparses`] → [`crate::write_atomic`]
//! — so a save produces a real `.wcl` edit that the watcher rebuilds and the
//! reload script picks up.
//!
//! Reads (schema introspection, object discovery) reopen the document the way
//! the build does, via [`wcl_wdoc::open_doc_for_edit`], so the editor sees the
//! same `@block` / `@table` / `@wdoc.file` schemas the renderer does.
//!
//! Logic lives here; the thin axum handlers in [`crate::serve`] parse the
//! request, call one of these functions, and map `Err(msg)` to a JSON 400.

use std::path::{Path, PathBuf};

use wcl_lang::edit as ast_edit;
use wcl_lang::{
    BuiltinType, DeclName, Document, Span, TypeDecl, TypeField, TypeRef, Value, ast,
    format as wcl_format, parse_expr, parse_for_edit,
};

/// Loads the WYSIWYG editor client.
pub(crate) const EDIT_SCRIPT_TAG: &str = "<script src=\"/__wdoc_edit.js\"></script>";

// The client script is kept in its own file for readability.
pub(crate) const EDIT_CLIENT_JS: &str = include_str!("edit_client.js");

// ---------------------------------------------------------------------------
// Read endpoints
// ---------------------------------------------------------------------------

/// `GET /__wdoc_object_kinds` — every editable data-object schema in the
/// document: a `@block` / `@table` type that is not the `@document` root.
pub(crate) fn object_kinds(root_file: &Path) -> Result<serde_json::Value, String> {
    let doc = wcl_wdoc::open_doc_for_edit(root_file).map_err(render_err)?;
    let mut kinds = Vec::new();
    for decl in doc.type_decls() {
        let Some(kind) = object_kind(&decl) else {
            continue;
        };
        let placement = file_placement(&decl);
        let full = decl.full_name();
        // Namespace = the type's full name minus its last segment (empty for a
        // root-namespace user type), so the client can group/filter by it.
        let namespace = full
            .rsplit_once('.')
            .map(|(ns, _)| ns)
            .unwrap_or("")
            .to_string();
        kinds.push((
            kind.clone(),
            serde_json::json!({
                "kind": kind,
                "type_name": full,
                "namespace": namespace,
                "is_imported": decl.is_imported(),
                "file_default": placement.map(|(p, folder)| serde_json::json!({
                    "path": p, "folder": folder,
                })),
            }),
        ));
    }
    // Sort kinds alphabetically (by kind name).
    kinds.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(serde_json::Value::Array(
        kinds.into_iter().map(|(_, v)| v).collect(),
    ))
}

/// `GET /__wdoc_schema?kind=` — the form descriptor for a block kind: one entry
/// per schema field plus the kind's allowed child kinds and `@wdoc.file`
/// default. Drives both the page side-panel form and the object editor form.
pub(crate) fn schema_descriptor(root_file: &Path, kind: &str) -> Result<serde_json::Value, String> {
    let doc = wcl_wdoc::open_doc_for_edit(root_file).map_err(render_err)?;
    let decl = doc
        .block_schema(kind)
        .ok_or_else(|| format!("no schema for block kind `{kind}`"))?;
    let fields: Vec<serde_json::Value> = decl
        .effective_fields()
        .iter()
        // Skip function-typed fields (e.g. a block's `lower` rendering fn) — they
        // are schema machinery, never edited as data.
        .filter(|f| !is_function_type(f.type_ref()))
        .map(|f| field_descriptor(&doc, f))
        .collect();
    let placement = file_placement(&decl);
    Ok(serde_json::json!({
        "kind": kind,
        "type_name": decl.full_name(),
        "is_imported": decl.is_imported(),
        "allowed_child_kinds": decl.allowed_child_kinds(),
        "file_default": placement.map(|(p, folder)| serde_json::json!({ "path": p, "folder": folder })),
        "fields": fields,
    }))
}

/// `GET /__wdoc_objects?kind=` — every existing instance of a block kind, with
/// its home file, source span, and a display label.
pub(crate) fn object_instances(root_file: &Path, kind: &str) -> Result<serde_json::Value, String> {
    let doc = wcl_wdoc::open_doc_for_edit(root_file).map_err(render_err)?;
    let mut out = Vec::new();
    for (path, block) in doc.blocks_with_source() {
        if block.kind() != kind {
            continue;
        }
        let span = block.span();
        let file = path
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| root_file.display().to_string());
        let label = block
            .labels()
            .ok()
            .and_then(|ls| ls.first().map(value_label))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| kind.to_string());
        out.push((
            label.clone(),
            serde_json::json!({
                "label": label,
                "file": file,
                "span": format!("{}:{}", span.start, span.end),
            }),
        ));
    }
    // Sort instances alphabetically by label.
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(serde_json::Value::Array(
        out.into_iter().map(|(_, v)| v).collect(),
    ))
}

/// `GET /__wdoc_object` — the current source-rendered value of each schema
/// field of one block instance (located by file + span).
pub(crate) fn read_object(
    root_file: &Path,
    file: &Path,
    span: Span,
    kind: &str,
) -> Result<serde_json::Value, String> {
    let doc = wcl_wdoc::open_doc_for_edit(root_file).map_err(render_err)?;
    let decl = doc
        .block_schema(kind)
        .ok_or_else(|| format!("no schema for block kind `{kind}`"))?;
    let src = std::fs::read_to_string(file).map_err(|e| format!("read {}: {e}", file.display()))?;
    let mut ast = parse_for_edit(&src, file.display().to_string()).map_err(render_err)?;
    let block = ast_edit::find_block_by_span(&mut ast.items, span)
        .ok_or_else(|| "object not found at the given location".to_string())?;
    let mut values = serde_json::Map::new();
    for f in decl.effective_fields() {
        let name = f.name();
        let expr = if let Some(slot) = f.inline_slot() {
            block.labels.get(slot as usize)
        } else {
            block.items.iter().find_map(|it| match it {
                ast::Item::Field(fl) if fl.name == name => Some(&fl.expr),
                _ => None,
            })
        };
        // Mirror the write channel: a plain string literal round-trips as `raw`
        // (so a text input shows unquoted text), everything else as an `expr`.
        if let Some(expr) = expr {
            let entry = match expr {
                ast::Expr::Utf8(s) | ast::Expr::Ascii(s) => {
                    serde_json::json!({ "kind": "raw", "value": s })
                }
                other => {
                    serde_json::json!({ "kind": "expr", "value": wcl_format::to_source_expr(other) })
                }
            };
            values.insert(name.to_string(), entry);
        }
    }
    Ok(serde_json::Value::Object(values))
}

/// `GET /__wdoc_object_source?file=&span=` — the raw WCL source text of one
/// block instance (the bytes its span covers), for the object editor's text box.
pub(crate) fn read_object_source(file: &Path, span: Span) -> Result<serde_json::Value, String> {
    let src = std::fs::read_to_string(file).map_err(|e| format!("read {}: {e}", file.display()))?;
    let text = src
        .get(span.start..span.end)
        .ok_or_else(|| "object span is out of range (stale — rebuild and retry)".to_string())?;
    Ok(serde_json::json!({ "text": text }))
}

/// `GET /__wdoc_object_template?kind=` — a starter WCL block for a new object of
/// `kind` (placeholder values per the schema), plus its `@wdoc.file` default, for
/// the "new object" text box.
pub(crate) fn object_template(root_file: &Path, kind: &str) -> Result<serde_json::Value, String> {
    let doc = wcl_wdoc::open_doc_for_edit(root_file).map_err(render_err)?;
    let decl = doc
        .block_schema(kind)
        .ok_or_else(|| format!("no schema for block kind `{kind}`"))?;
    // Inline-slot fields become labels (in slot order); the rest become
    // `name = <placeholder>` items. Skip function-typed schema machinery.
    let mut labels: Vec<(u64, ast::Expr)> = Vec::new();
    let mut named: Vec<(String, ast::Expr)> = Vec::new();
    for f in decl.effective_fields() {
        if is_function_type(f.type_ref()) {
            continue;
        }
        let ph = placeholder_expr(field_widget(&doc, &f).0, f.name());
        match f.inline_slot() {
            Some(slot) => labels.push((slot, ph)),
            None => named.push((f.name().to_string(), ph)),
        }
    }
    labels.sort_by_key(|(s, _)| *s);
    let label_exprs = labels.into_iter().map(|(_, e)| e).collect();
    let block = ast_edit::build_block(kind, &[], label_exprs, named);
    let src = ast::Source {
        items: vec![ast::Item::Block(block)],
        trailing_trivia: Vec::new(),
    };
    let text = wcl_format::to_source(&src);
    let placement = file_placement(&decl);
    Ok(serde_json::json!({
        "text": text,
        "file_default": placement.map(|(p, folder)| serde_json::json!({ "path": p, "folder": folder })),
    }))
}

// ---------------------------------------------------------------------------
// Write endpoints
// ---------------------------------------------------------------------------

/// `POST /__wdoc_edit/field` — set (or insert) a block's field/label. The block
/// is located by `file` + `span`; `inline_slot` (when present) routes the value
/// to that label slot instead of a `name = value` item. The value arrives as
/// either `raw` (literal text → a string literal) or `expr` (a WCL expression).
pub(crate) fn field_edit(
    watch_root: &Path,
    root_file: &Path,
    body: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let (file, span) = block_target(watch_root, body)?;
    let name = str_field(body, "name")?;
    let inline_slot = body.get("inline_slot").and_then(serde_json::Value::as_u64);
    let expr = value_expr(body)?;

    let src = read(&file)?;
    let mut ast = parse_for_edit(&src, file.display().to_string()).map_err(render_err)?;
    let block = ast_edit::find_block_by_span(&mut ast.items, span)
        .ok_or_else(|| "target block not found".to_string())?;
    match inline_slot {
        Some(slot) => {
            if !ast_edit::set_label(block, slot as usize, expr) {
                return Err(format!("inline slot {slot} is out of range for this block"));
            }
        }
        None => ast_edit::set_or_insert_field(block, name, expr),
    }
    commit(root_file, vec![(file, wcl_format::to_source(&ast))])
}

/// `POST /__wdoc_edit/add` — insert a new block, built from `kind` + field
/// inputs, as the `index`-th block child of `parent` (or top-level when
/// `parent` is absent), in `file`.
pub(crate) fn add_block(
    watch_root: &Path,
    root_file: &Path,
    body: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let file_str = str_field(body, "file")?;
    let file = crate::serve::sandboxed(watch_root, Path::new(file_str))
        .ok_or_else(|| format!("file outside the served tree: {file_str}"))?;
    let kind = str_field(body, "kind")?;
    let index = body
        .get("index")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as usize;

    let doc = wcl_wdoc::open_doc_for_edit(root_file).map_err(render_err)?;
    let block = build_block_from_inputs(&doc, kind, body.get("fields"))?;
    drop(doc);

    let src = read(&file)?;
    let mut ast = parse_for_edit(&src, file.display().to_string()).map_err(render_err)?;
    // Placement, most-specific first:
    //   after_span  → insert right after the selected block (in its parent)
    //   parent_span → insert as the index-th block child of that parent
    //   neither     → insert at top level by index
    if let Some(after) = body.get("after_span").and_then(serde_json::Value::as_str) {
        let aspan = parse_span(after).ok_or("bad after_span")?;
        if !ast_edit::insert_block_after_span(&mut ast.items, aspan, block) {
            return Err("anchor block not found".to_string());
        }
    } else if let Some(ps) = body.get("parent_span").and_then(serde_json::Value::as_str) {
        let pspan = parse_span(ps).ok_or("bad parent_span")?;
        let parent = ast_edit::find_block_by_span(&mut ast.items, pspan)
            .ok_or_else(|| "parent block not found".to_string())?;
        ast_edit::insert_block_at_index(&mut parent.items, index, block);
    } else {
        ast_edit::insert_block_at_index(&mut ast.items, index, block);
    }
    commit(root_file, vec![(file, wcl_format::to_source(&ast))])
}

/// `POST /__wdoc_edit/delete` — remove the block at `file` + `span`.
pub(crate) fn delete_block(
    watch_root: &Path,
    root_file: &Path,
    body: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let (file, span) = block_target(watch_root, body)?;
    let src = read(&file)?;
    let mut ast = parse_for_edit(&src, file.display().to_string()).map_err(render_err)?;
    if !ast_edit::remove_block_by_span(&mut ast.items, span) {
        return Err("block not found".to_string());
    }
    commit(root_file, vec![(file, wcl_format::to_source(&ast))])
}

/// `POST /__wdoc_edit/move` — swap the block at `file` + `span` with its
/// adjacent block sibling in `direction` (`"up"` / `"down"`).
pub(crate) fn move_block(
    watch_root: &Path,
    root_file: &Path,
    body: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let (file, span) = block_target(watch_root, body)?;
    let down = match str_field(body, "direction")? {
        "down" => true,
        "up" => false,
        other => return Err(format!("bad direction `{other}`")),
    };
    let src = read(&file)?;
    let mut ast = parse_for_edit(&src, file.display().to_string()).map_err(render_err)?;
    if !ast_edit::move_block_by_span(&mut ast.items, span, down) {
        return Err("cannot move block (already at edge?)".to_string());
    }
    commit(root_file, vec![(file, wcl_format::to_source(&ast))])
}

/// `POST /__wdoc_object` — create / save / delete a data object, dispatched on
/// the `op` field. `create` and `save` carry the object's raw WCL `text` (edited
/// in the object editor's text box).
pub(crate) fn object_post(
    watch_root: &Path,
    root_file: &Path,
    body: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    match str_field(body, "op")? {
        "create" => create_object(watch_root, root_file, body),
        "save" => save_object_source(watch_root, root_file, body),
        "delete" => delete_block(watch_root, root_file, body),
        other => Err(format!("unknown object op `{other}`")),
    }
}

/// Replace one object's source with the edited `text` — a byte-splice of the
/// block's span in its file, so the user's exact formatting is kept (the rest of
/// the file is untouched). `commit` re-parses + schema-checks before writing.
fn save_object_source(
    watch_root: &Path,
    root_file: &Path,
    body: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let (file, span) = block_target(watch_root, body)?;
    let text = str_field(body, "text")?;
    let src = read(&file)?;
    let before = src
        .get(..span.start)
        .ok_or_else(|| "object span is out of range (stale — rebuild and retry)".to_string())?;
    let after = src
        .get(span.end..)
        .ok_or_else(|| "object span is out of range (stale — rebuild and retry)".to_string())?;
    let new = format!("{before}{text}{after}");
    commit(root_file, vec![(file, new)])
}

/// Create a new object from raw `text`: resolve the target file (UI override →
/// `@wdoc.file` default → the document's own file), append the text there
/// (creating the file and wiring an `import` into the root document when new),
/// and commit. The text is appended verbatim (no reformat).
fn create_object(
    watch_root: &Path,
    root_file: &Path,
    body: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let kind = str_field(body, "kind")?;
    let text = str_field(body, "text")?;
    let doc = wcl_wdoc::open_doc_for_edit(root_file).map_err(render_err)?;
    let decl = doc
        .block_schema(kind)
        .ok_or_else(|| format!("no schema for block kind `{kind}`"))?;
    let placement = file_placement(&decl);
    drop(doc);

    // Parse the text to validate it carries a block and to derive a folder id.
    let parsed = parse_for_edit(text, "<new object>").map_err(render_err)?;
    let block = parsed
        .items
        .iter()
        .find_map(|it| match it {
            ast::Item::Block(b) => Some(b),
            _ => None,
        })
        .ok_or_else(|| "the new object text must contain a block".to_string())?;

    // Resolve the target path (relative to the document root = watch_root);
    // `None` ⇒ the document's own file.
    let override_path = body
        .get("target_file")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty());
    let rel: Option<String> = match (override_path, &placement) {
        (Some(p), _) => Some(p.to_string()),
        (None, Some((path, folder))) => Some(if *folder {
            format!(
                "{}/{}.wcl",
                path.trim_end_matches('/'),
                first_label_slug(block)
            )
        } else {
            path.clone()
        }),
        (None, None) => None,
    };

    let target = match &rel {
        Some(r) => file_target(watch_root, r)?,
        None => root_file.to_path_buf(),
    };

    // Append the text verbatim to the target file (created if absent).
    let target_src = std::fs::read_to_string(&target).unwrap_or_default();
    let mut changes = vec![(target.clone(), append_block_text(&target_src, text))];

    // Wire an `import "<rel>"` into the root document if the object landed in a
    // separate, not-yet-imported file.
    if let Some(r) = &rel
        && !same_file(&target, root_file)
    {
        let root_src = read(root_file)?;
        let mut root_ast =
            parse_for_edit(&root_src, root_file.display().to_string()).map_err(render_err)?;
        if ast_edit::ensure_import(&mut root_ast, r) {
            changes.push((root_file.to_path_buf(), wcl_format::to_source(&root_ast)));
        }
    }

    commit(root_file, changes)
}

/// Append a block's source `text` to existing file contents, separated by a
/// blank line (and ensuring a trailing newline).
fn append_block_text(existing: &str, text: &str) -> String {
    let mut out = existing.trim_end().to_string();
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(text.trim_end());
    out.push('\n');
    out
}

/// Whether two paths point at the same file (canonicalized). `false` if either
/// can't be canonicalized (e.g. a target that doesn't exist yet).
fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Schema → form descriptor
// ---------------------------------------------------------------------------

/// The editable block / table kind a type declares, or `None` when it isn't a
/// data-object schema (no `@block`/`@table`, or it's the `@document` root).
fn object_kind(decl: &TypeDecl<'_>) -> Option<String> {
    let mut is_document = false;
    let mut kind = None;
    for d in decl.decorators() {
        match d.name() {
            "document" => is_document = true,
            "block" | "table" => {
                if let Ok(args) = d.positional()
                    && let Some(v) = args.first()
                {
                    kind = Some(value_label(v));
                }
            }
            _ => {}
        }
    }
    if is_document { None } else { kind }
}

/// The `@wdoc.file` (or bare `@file`) placement on a schema: `(path, folder)`.
fn file_placement(decl: &TypeDecl<'_>) -> Option<(String, bool)> {
    let d = decl
        .decorators()
        .find(|d| d.name() == "file" || d.full_name() == "wdoc.file")?;
    let path = d.positional().ok()?.first().map(value_label)?;
    let folder = matches!(d.named_arg("folder"), Some(Ok(Value::Bool(true))));
    Some((path, folder))
}

/// One form-field descriptor for the JS client.
fn field_descriptor(doc: &Document, f: &TypeField<'_>) -> serde_json::Value {
    let widget = field_widget(doc, f);
    let mut desc = serde_json::json!({
        "name": f.name(),
        "doc": f.doc_comment(),
        "optional": f.optional(),
        "type": type_ref_name(f.type_ref()),
        "widget": widget.0,
        "inline_slot": f.inline_slot(),
        "default": f.default_value().map(|v| value_label(&v)),
    });
    if let Some(extra) = widget.1
        && let (serde_json::Value::Object(d), serde_json::Value::Object(e)) = (&mut desc, extra)
    {
        for (k, v) in e {
            d.insert(k, v);
        }
    }
    desc
}

/// Map a field's declared type (and decorators) to a UI widget tag plus any
/// extra descriptor keys (enum variants, union variants, child kind, ref kind).
fn field_widget(doc: &Document, f: &TypeField<'_>) -> (&'static str, Option<serde_json::Value>) {
    // A `@ref("kind")` field is a dropdown over existing instances of that kind.
    if let Some(kind) = f.ref_block_kind() {
        return ("ref", Some(serde_json::json!({ "ref_kind": kind })));
    }
    // `@child` / `@children` are nested blocks — surfaced for the page editor.
    if let Some(ck) = f.child_kind_or_union() {
        return (
            "child",
            Some(serde_json::json!({ "child_kind": child_kind_label(&ck) })),
        );
    }
    if let Some(ck) = f.children_kind_or_union() {
        return (
            "children",
            Some(serde_json::json!({
                "child_kind": child_kind_label(&ck),
                "min": f.children_min(),
                "max": f.children_max(),
            })),
        );
    }
    widget_for_type(doc, f.type_ref())
}

fn widget_for_type(doc: &Document, ty: &TypeRef) -> (&'static str, Option<serde_json::Value>) {
    match ty {
        TypeRef::Builtin(b) => match b {
            BuiltinType::Bool => ("bool", None),
            BuiltinType::Utf8 | BuiltinType::Ascii | BuiltinType::Utf16 | BuiltinType::Utf32 => {
                ("text", None)
            }
            BuiltinType::Symbol | BuiltinType::Identifier => ("symbol", None),
            n if n.is_numeric() => ("number", None),
            _ => ("text", None),
        },
        TypeRef::Reference(inner) => widget_for_type(doc, inner),
        TypeRef::Named(segs) => {
            let fqn = segs.join(".");
            if let Some(ss) = doc.symbol_set(&fqn) {
                let variants: Vec<String> = ss.symbols().map(|s| s.name().to_string()).collect();
                return ("enum", Some(serde_json::json!({ "variants": variants })));
            }
            if let Some(u) = doc.union_decl(&fqn) {
                let variants: Vec<serde_json::Value> = u
                    .variants()
                    .map(|v| {
                        let fields: Vec<serde_json::Value> =
                            v.fields().map(|vf| field_descriptor(doc, &vf)).collect();
                        serde_json::json!({ "name": v.name(), "fields": fields })
                    })
                    .collect();
                return ("union", Some(serde_json::json!({ "variants": variants })));
            }
            // A nested type/interface — edit as a raw expression for now.
            ("expr", None)
        }
        // Lists / tensors / functions: raw expression fallback.
        _ => ("expr", None),
    }
}

fn child_kind_label(ck: &wcl_lang::ChildKind<'_>) -> String {
    match ck {
        wcl_lang::ChildKind::Kind(s) => s.clone(),
        wcl_lang::ChildKind::Union(u) => u.full_name(),
        wcl_lang::ChildKind::Interface(i) => i.full_name(),
    }
}

// ---------------------------------------------------------------------------
// Block construction from form inputs
// ---------------------------------------------------------------------------

/// Build an [`ast::Block`] of `kind` from the JSON `fields` array
/// (`[{name, inline_slot?, raw?|expr?}, …]`), routing `@inline(slot)` fields to
/// label positions (in slot order) and the rest to `name = value` items.
fn build_block_from_inputs(
    doc: &Document,
    kind: &str,
    fields: Option<&serde_json::Value>,
) -> Result<ast::Block, String> {
    // Map field name → inline slot from the schema (so the client need not know).
    let decl = doc.block_schema(kind);
    let slot_of = |name: &str| -> Option<u64> {
        decl.as_ref()
            .and_then(|d| d.effective_field(name))
            .and_then(|f| f.inline_slot())
    };

    let inputs = fields
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut labels: Vec<(u64, ast::Expr)> = Vec::new();
    let mut named: Vec<(String, ast::Expr)> = Vec::new();
    for item in &inputs {
        let name = item
            .get("name")
            .and_then(serde_json::Value::as_str)
            .ok_or("field input missing name")?;
        let expr = value_expr(item)?;
        let slot = item
            .get("inline_slot")
            .and_then(serde_json::Value::as_u64)
            .or_else(|| slot_of(name));
        match slot {
            Some(s) => labels.push((s, expr)),
            None => named.push((name.to_string(), expr)),
        }
    }
    // Order the labels by slot and require them contiguous from 0.
    labels.sort_by_key(|(s, _)| *s);
    let mut label_exprs = Vec::with_capacity(labels.len());
    for (i, (slot, expr)) in labels.into_iter().enumerate() {
        if slot as usize != i {
            return Err(format!(
                "inline label slots must be contiguous from 0; missing slot {i}"
            ));
        }
        label_exprs.push(expr);
    }
    Ok(ast_edit::build_block(kind, &[], label_exprs, named))
}

// ---------------------------------------------------------------------------
// Commit pipeline (write → validate → rollback)
// ---------------------------------------------------------------------------

/// Write every `(path, contents)` change atomically, then reopen the root
/// document and run schema validation. If anything fails to re-parse or the
/// document has schema errors, restore the originals and return the message —
/// so a constraint violation surfaces as an error and never lands on disk.
fn commit(root_file: &Path, changes: Vec<(PathBuf, String)>) -> Result<serde_json::Value, String> {
    use std::collections::HashSet;

    // Syntax gate before touching disk.
    for (path, content) in &changes {
        crate::verify_reparses(content).map_err(|e| {
            format!(
                "internal: produced unparseable WCL for {}: {e}",
                path.display()
            )
        })?;
    }
    // Pre-existing schema errors (unrelated to this edit) must not block it —
    // capture them so we only reject errors the edit *introduces*.
    let baseline: HashSet<String> = wcl_wdoc::open_doc_for_edit(root_file)
        .map(|d| d.schema_errors().iter().map(|e| e.to_string()).collect())
        .unwrap_or_default();
    // Back up originals (None = file did not exist → rollback deletes it).
    let backups: Vec<(PathBuf, Option<String>)> = changes
        .iter()
        .map(|(p, _)| (p.clone(), std::fs::read_to_string(p).ok()))
        .collect();
    for (path, content) in &changes {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = crate::write_atomic(path, content) {
            restore(&backups);
            return Err(format!("write {}: {e}", path.display()));
        }
    }
    // Semantic gate: reopen + validate; roll everything back if the edit added
    // any schema error not already present at baseline.
    match wcl_wdoc::open_doc_for_edit(root_file) {
        Ok(doc) => {
            let introduced: Vec<String> = doc
                .schema_errors()
                .iter()
                .map(|e| e.to_string())
                .filter(|m| !baseline.contains(m))
                .collect();
            if !introduced.is_empty() {
                restore(&backups);
                return Err(introduced.join("; "));
            }
        }
        Err(e) => {
            restore(&backups);
            return Err(render_err(e));
        }
    }
    Ok(serde_json::json!({ "ok": true }))
}

fn restore(backups: &[(PathBuf, Option<String>)]) {
    for (path, original) in backups {
        match original {
            Some(content) => {
                let _ = crate::write_atomic(path, content);
            }
            None => {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

/// Resolve `{file, span}` from a request body to a sandboxed existing path and
/// its parsed span.
fn block_target(watch_root: &Path, body: &serde_json::Value) -> Result<(PathBuf, Span), String> {
    let file_str = str_field(body, "file")?;
    let file = crate::serve::sandboxed(watch_root, Path::new(file_str))
        .ok_or_else(|| format!("file outside the served tree: {file_str}"))?;
    let span = body
        .get("span")
        .and_then(serde_json::Value::as_str)
        .and_then(parse_span)
        .ok_or("missing or bad span")?;
    Ok((file, span))
}

/// Resolve a client-supplied target path (relative to the document root) to an
/// absolute path under `watch_root`, rejecting `..` escapes. Works for files
/// that don't exist yet (unlike the canonicalizing [`crate::serve::sandboxed`]).
fn file_target(watch_root: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel = rel.trim_start_matches("./");
    if Path::new(rel).is_absolute() || rel.split('/').any(|seg| seg == ".." || seg.is_empty()) {
        return Err(format!("invalid target path: {rel}"));
    }
    Ok(watch_root.join(rel))
}

/// A field value from a request: `raw` (literal text) or `expr` (a WCL
/// expression). Raw wins when both are present.
fn value_expr(body: &serde_json::Value) -> Result<ast::Expr, String> {
    if let Some(raw) = body.get("raw").and_then(serde_json::Value::as_str) {
        return Ok(ast_edit::string_literal_expr(raw));
    }
    if let Some(expr) = body.get("expr").and_then(serde_json::Value::as_str) {
        return parse_expr(expr, "<edit value>").map_err(render_err);
    }
    Err("field value missing `raw` or `expr`".to_string())
}

fn str_field<'a>(body: &'a serde_json::Value, key: &str) -> Result<&'a str, String> {
    body.get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("missing `{key}`"))
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))
}

/// Parse a `"start:end"` span string.
fn parse_span(s: &str) -> Option<Span> {
    let (a, b) = s.split_once(':')?;
    Some(Span::new(a.parse().ok()?, b.parse().ok()?))
}

/// A short, plain rendering of a scalar value (labels, defaults, enum names).
fn value_label(v: &Value) -> String {
    match v {
        Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) => s.clone(),
        Value::Symbol(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::I64(n) => n.to_string(),
        Value::U64(n) => n.to_string(),
        Value::F64(n) => n.to_string(),
        other => format!("{other:?}"),
    }
}

/// A filesystem-safe slug derived from a block's first label (for `folder=true`
/// placement). Falls back to `"object"` when there's no usable label.
fn first_label_slug(block: &ast::Block) -> String {
    let raw = block
        .labels
        .first()
        .map(wcl_format::to_source_expr)
        .unwrap_or_default();
    let slug: String = raw
        .trim_matches('"')
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "object".to_string()
    } else {
        slug
    }
}

/// A placeholder expression for a schema field, used to seed the "new object"
/// template. `name` is the field name (a readable stand-in for identifier/label
/// slots the author then renames).
fn placeholder_expr(widget: &str, name: &str) -> ast::Expr {
    match widget {
        "text" => ast::Expr::Utf8(String::new()),
        "bool" => ast::Expr::Bool(false),
        "number" => ast::Expr::I64(0),
        // symbol widget covers both `symbol` and `identifier`; a bare identifier
        // placeholder reads as a name the author edits (valid in label position).
        "symbol" => ast::Expr::Identifier(name.to_string(), Span::new(0, 0)),
        _ => ast::Expr::None,
    }
}

/// Whether a field's declared type is a function (or a reference to one) — such
/// fields are schema machinery (e.g. a block's `lower` fn), not editable data.
fn is_function_type(ty: &TypeRef) -> bool {
    match ty {
        TypeRef::Function { .. } => true,
        TypeRef::Reference(inner) => is_function_type(inner),
        _ => false,
    }
}

fn type_ref_name(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Builtin(b) => b.name().to_string(),
        TypeRef::Named(segs) => segs.join("."),
        TypeRef::Reference(inner) => format!("&{}", type_ref_name(inner)),
        TypeRef::List(inner) => format!("list<{}>", type_ref_name(inner)),
        TypeRef::Tensor { element, .. } => format!("tensor<{}>", type_ref_name(element)),
        TypeRef::Function { .. } => "fn".to_string(),
    }
}

fn render_err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
