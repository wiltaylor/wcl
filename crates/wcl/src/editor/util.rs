//! Small readers shared by every editor endpoint: pulling a label out of a
//! block (AST or document view) and rendering a value as the plain string
//! a form field shows. (Paths are the [workspace](super::workspace)'s
//! business, not these.)
//!
//! They live here rather than in whichever handler happened to need them
//! first: every one of them has callers in several endpoint modules, and
//! none of them is about the request handling any of those modules is
//! named for. What is NOT here is the block-source classification
//! (`classify_expr`, `visibility_json`): those define the shape
//! `/api/block/source` answers with, so they stay with that endpoint even
//! though other modules read the same shape.

use std::path::Path;

use wcl_lang::{Span, Value};

/// The first inline label of an AST block when it's a plain identifier or
/// string literal. How a block is *named* is a language fact, not an editor
/// one — this and [`find_block_by_kind_label`] are `wcl_lang::edit`'s, kept
/// here under the names the editor already reads them by.
pub(super) use wcl_lang::edit::{block_label as ast_label, find_block_by_kind_label};

/// The first label of a document-view block as a plain string.
pub(super) fn first_label(b: &wcl_lang::Block<'_>) -> Option<String> {
    b.labels()
        .ok()
        .and_then(|ls| ls.first().map(value_string))
        .filter(|s| !s.is_empty())
}

/// The first positional argument of a decorator, as a string.
pub(super) fn dec_first_string(d: &wcl_lang::Decorator<'_>) -> Option<String> {
    d.positional().ok()?.first().map(|v| match v {
        Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) => s.clone(),
        other => format!("{other:?}"),
    })
}

/// A document-view block's field value as a plain string, when it
/// evaluates to a scalar. Forces the field's evaluation.
pub(super) fn field_string(b: &wcl_lang::Block<'_>, name: &str) -> Option<String> {
    b.field(name)
        .and_then(|f| f.value().ok().cloned())
        .as_ref()
        .map(value_string)
        .filter(|s| !s.is_empty())
}

/// A scalar value as the plain string a form field shows.
pub(super) fn value_string(v: &Value) -> String {
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

/// A `{start, end}` byte span from a JSON object field.
pub(super) fn span_field(v: &serde_json::Value, key: &str) -> Result<Span, String> {
    let s = v.get(key).ok_or_else(|| format!("missing `{key}`"))?;
    let num = |k: &str| {
        s.get(k)
            .and_then(serde_json::Value::as_u64)
            .map(|n| n as usize)
            .ok_or_else(|| format!("missing `{key}.{k}`"))
    };
    Ok(Span::new(num("start")?, num("end")?))
}

/// What every span-addressed endpoint says when the span no longer names a
/// block: the file moved under the client, so re-anchor and retry.
pub(super) fn stale_span() -> String {
    "no block at that span — the file changed; rebuild the preview".to_string()
}

/// Whether `s` can be written as a bare WCL identifier — the lexer's own
/// rule, so what the editor accepts is what the lexer will read back.
pub(super) use wcl_lang::is_identifier;

/// A [`wcl_wskill`] anchor as the repo-relative `{file, span}` binding every
/// editor response speaks and every write endpoint takes back.
///
/// The model anchors relative to the **wskill root**, which is a
/// sub-directory of the served tree whenever a wskill is edited inside a
/// larger repo — so both adapters over the model need this translation, and
/// needing it twice is how two answers for one file's name appear.
pub(super) fn anchor_json(
    ws: &super::Workspace,
    root: &Path,
    anchor: &wcl_wskill::Anchor,
) -> serde_json::Value {
    serde_json::json!({
        "file": anchor_file(ws, root, anchor),
        "span": super::span_json(anchor.span),
    })
}

/// [`anchor_json`]'s path half, for a payload that carries `file` and `span`
/// as separate keys.
pub(super) fn anchor_file(
    ws: &super::Workspace,
    root: &Path,
    anchor: &wcl_wskill::Anchor,
) -> String {
    rel_file(ws, root, &anchor.file)
}

/// The translation itself, for a model path that arrives without an anchor
/// around it — an audit's [`NodeDelta::file`](wcl_wskill::audit::NodeDelta),
/// where a removal's span belongs to the revision it was deleted from and
/// so does not travel with the path.
pub(super) fn rel_file(ws: &super::Workspace, root: &Path, file: &Path) -> String {
    let abs = root.join(file);
    ws.rel(&abs).unwrap_or_else(|_| abs.display().to_string())
}
