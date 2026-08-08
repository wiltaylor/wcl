//! Node lookup for the **edit path** ([`crate::parse_for_edit`]).
//!
//! This operates on the owned, fully-public [`ast`](crate::ast) that
//! [`crate::parse_for_edit`] returns. It finds a node by its byte [`Span`].
//! The caller (`wcl set`) then replaces that node's expression in place and
//! re-prints through [`crate::format::to_source`].
//!
//! A span-equality lookup works because the edit path re-parses the same
//! source bytes a [`crate::Document`] saw. The node positions match exactly.

use crate::ast::{Field, Item, Span};

/// Walk `items` (recursing into [`Item::Block`] bodies) to find the
/// [`crate::ast::Field`] whose `span` matches `span`.
pub fn find_field_by_span(items: &mut [Item], span: Span) -> Option<&mut Field> {
    for item in items {
        match item {
            Item::Field(f) if f.span == span => return Some(f),
            Item::Block(b) => {
                if let Some(found) = find_field_by_span(&mut b.items, span) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}
