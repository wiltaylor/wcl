//! Cheap lookups against a parallel `(items, cells)` slice pair.
//!
//! These helpers are the building blocks for `Block::field`, `Block::block`,
//! and `Block::fields` / `Block::blocks` (plus the document-root equivalents).
//! Each constructs a typed view (`Field` / `Block` / `TableView`) by zipping
//! the AST slice with the matching cells slice and yielding the entries that
//! match a given name/kind.

use crate::ast;

use super::cells::{ItemCellKind, ItemCells};
use super::scope::Scope;
use super::{Block, Document, Field, LetView, TableView};

pub(super) fn find_field<'a>(
    items: &'a [ast::Item],
    cells: &'a [ItemCells],
    name: &str,
    doc: &'a Document,
    file_ns: &'a [String],
    scope: &Scope<'a>,
) -> Option<Field<'a>> {
    items
        .iter()
        .zip(cells)
        .find_map(|(item, cells)| match (item, &cells.kind) {
            (ast::Item::Field(f), ItemCellKind::Field(_)) if f.name == name => Some(Field {
                ast: f,
                cells,
                doc,
                file_ns,
                scope: scope.clone(),
            }),
            _ => None,
        })
}

pub(super) fn find_block<'a>(
    items: &'a [ast::Item],
    cells: &'a [ItemCells],
    kind: &str,
    doc: &'a Document,
    file_ns: &'a [String],
    scope: &Scope<'a>,
) -> Option<Block<'a>> {
    items
        .iter()
        .zip(cells)
        .find_map(|(item, cells)| match (item, &cells.kind) {
            (ast::Item::Block(b), ItemCellKind::Block { .. }) if b.kind == kind => Some(Block {
                ast: b,
                cells,
                doc,
                file_ns,
                kind_override: None,
                scope: scope.clone(),
            }),
            _ => None,
        })
}

/// Find a `let name = expr` binding by name. Mirrors [`find_field`]
/// but matches `Item::Let`; the resulting [`LetView`] resolves and
/// caches the bound value on demand.
pub(super) fn find_let<'a>(
    items: &'a [ast::Item],
    cells: &'a [ItemCells],
    name: &str,
    doc: &'a Document,
    scope: &Scope<'a>,
) -> Option<LetView<'a>> {
    items
        .iter()
        .zip(cells)
        .find_map(|(item, cells)| match (item, &cells.kind) {
            (ast::Item::Let(l), ItemCellKind::Let(cell)) if l.name == name => Some(LetView {
                ast: l,
                cell,
                doc,
                scope: scope.clone(),
            }),
            _ => None,
        })
}

pub(super) fn iter_fields<'a>(
    items: &'a [ast::Item],
    cells: &'a [ItemCells],
    doc: &'a Document,
    file_ns: &'a [String],
    scope: Scope<'a>,
) -> impl Iterator<Item = Field<'a>> + 'a {
    items
        .iter()
        .zip(cells)
        .filter_map(move |(item, cells)| match (item, &cells.kind) {
            (ast::Item::Field(f), ItemCellKind::Field(_)) => Some(Field {
                ast: f,
                cells,
                doc,
                file_ns,
                scope: scope.clone(),
            }),
            _ => None,
        })
}

pub(super) fn iter_blocks<'a>(
    items: &'a [ast::Item],
    cells: &'a [ItemCells],
    doc: &'a Document,
    file_ns: &'a [String],
    scope: Scope<'a>,
) -> impl Iterator<Item = Block<'a>> + 'a {
    items
        .iter()
        .zip(cells)
        .filter_map(move |(item, cells)| match (item, &cells.kind) {
            (ast::Item::Block(b), ItemCellKind::Block { .. }) => Some(Block {
                ast: b,
                cells,
                doc,
                file_ns,
                kind_override: None,
                scope: scope.clone(),
            }),
            _ => None,
        })
}

pub(super) fn iter_tables<'a>(
    items: &'a [ast::Item],
    doc: &'a Document,
) -> impl Iterator<Item = TableView<'a>> + 'a {
    items.iter().filter_map(move |item| match item {
        ast::Item::Table(t) => Some(TableView { ast: t, doc }),
        _ => None,
    })
}
