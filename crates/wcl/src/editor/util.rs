//! Small readers shared by every editor endpoint: pulling a label out of a
//! block (AST or document view) and rendering a value as the plain string
//! a form field shows. (Paths are the [workspace](super::workspace)'s
//! business, not these.)
//!
//! They live here rather than in whichever handler happened to need them
//! first: every one of them is used by three or more of the endpoint
//! modules, and none of them is about the request handling any of those
//! modules is named for.

use wcl_lang::Value;
use wcl_lang::ast::{self, Expr, Item};

/// The first inline label of an AST block when it's a plain identifier or
/// string literal.
pub(super) fn ast_label(b: &ast::Block) -> Option<String> {
    match b.labels.first()? {
        Expr::Utf8(s) | Expr::Ascii(s) => Some(s.clone()),
        Expr::Identifier(s, _) => Some(s.clone()),
        _ => None,
    }
}

/// The first `kind`-block labelled `label`, anywhere in the tree.
pub(super) fn find_block_by_kind_label<'a>(
    items: &'a mut [Item],
    kind: &str,
    label: &str,
) -> Option<&'a mut ast::Block> {
    for item in items {
        if let Item::Block(b) = item {
            if b.kind == kind && ast_label(b).as_deref() == Some(label) {
                return Some(b);
            }
            if let Some(found) = find_block_by_kind_label(&mut b.items, kind, label) {
                return Some(found);
            }
        }
    }
    None
}

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
