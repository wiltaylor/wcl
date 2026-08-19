//! The **edit path**: parse to an owned AST, find a node, hand it back.
//!
//! The counterpart of the evaluating path ([`crate::Document`]), and
//! deliberately disjoint from it. [`parse_for_edit`] returns an
//! [`ast::Source`] with fully `pub` fields and does
//! *no* evaluation, schema checking or import resolution;
//! [`find_field_by_span`] locates the node a host wants to change; the
//! host mutates it and prints the result back with
//! [`format::to_source`](crate::format::to_source). To evaluate after an
//! edit, reopen the file as a `Document`.
//!
//! [`parse_expr`] is the small companion for supplying a replacement:
//! one expression parsed from a standalone string, ready to drop into
//! the mutated tree.
//!
//! Node lookup is by byte [`Span`] equality, which works because the
//! edit path re-parses the same source bytes a [`crate::Document`] saw —
//! the positions match exactly.

use crate::ast::{self, Field, Item, Span};
use crate::diagnostics::ParseError;
use crate::parser;

/// Parse a WCL source string into an owned [`ast::Source`] for inspection
/// or mutation. The returned AST has fully `pub` fields. Hosts walk it,
/// edit it, and print it back to a `.wcl` file with
/// [`crate::format::to_source`].
///
/// This is the **edit-path** entry point. It performs *no* evaluation,
/// schema checks, or import resolution — those happen only when a
/// [`Document`](crate::Document) is opened from the (post-edit) file. The two paths are
/// deliberately disjoint so AST mutations can't invalidate a
/// Document's cached fields silently.
///
/// `name` is used for diagnostics only (it becomes the
/// `NamedSource` label on any [`ParseError`]).
pub fn parse_for_edit(source: &str, name: impl Into<String>) -> Result<ast::Source, ParseError> {
    parser::Parser::new(source, name)
        .parse_source()
        .map(|(src, _idx)| src)
}

/// Parse a single WCL expression from a standalone string. Returns the
/// parsed [`ast::Expr`] ready to drop into a host-mutated AST
/// (e.g. `field.expr = parse_expr(...)?`).
///
/// Fails if the input is empty, has trailing tokens after the
/// expression, or contains a lex/parse error. `name` is used only for
/// diagnostics — typically `"<cli>"` or `"<set value>"` when there's
/// no real source location.
///
/// Useful for CLI flows like `wcl set file path <value>`, where
/// `<value>` is a literal expression supplied on the command line.
pub fn parse_expr(source: &str, name: impl Into<String>) -> Result<ast::Expr, ParseError> {
    parser::Parser::new(source, name).parse_expr_only()
}

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
