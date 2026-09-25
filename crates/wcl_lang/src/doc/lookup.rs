//! Cheap lookups against a parallel `(items, cells)` slice pair.
//!
//! These helpers are the building blocks for `Block::field`, `Block::block`,
//! and `Block::fields` / `Block::blocks` (plus the document-root equivalents).
//! Each constructs a typed view (`Field` / `Block` / `TableView`) by zipping
//! the AST slice with the matching cells slice and yielding the entries that
//! match a given name/kind.
//!
//! The by-name finders resolve through a [`NameIndex`] built once per slice,
//! so a body with many items answers each reference in constant time rather
//! than rescanning every item.

use std::collections::HashMap;

use crate::ast;

use super::cells::{ItemCellKind, ItemCells};
use super::imports::BlockSlice;
use super::scope::Scope;
use super::{Block, Document, Field, LetView, TableView};

/// Slices shorter than this are scanned directly: hashing the name costs
/// more than comparing it against a handful of items, and most block
/// bodies are this small.
const INDEX_MIN_ITEMS: usize = 16;

/// Where the first field, `let` and block of each name sits in one
/// `(items, cells)` slice. Only the first occurrence is kept, which is
/// the item a front-to-back scan would find, so a later duplicate stays
/// shadowed exactly as before.
#[derive(Debug, Default)]
pub(crate) struct NameIndex {
    /// Field name to item index.
    fields: HashMap<String, usize>,
    /// `let` name to item index.
    lets: HashMap<String, usize>,
    /// Block kind to the index of the first block of that kind.
    blocks: HashMap<String, usize>,
}

impl NameIndex {
    /// Index `items`, counting an item only when its cell has the
    /// matching kind — the same pairing the finders check.
    fn build(items: &[ast::Item], cells: &[ItemCells]) -> Self {
        let mut index = Self::default();
        for (i, (item, cells)) in items.iter().zip(cells).enumerate() {
            match (item, &cells.kind) {
                (ast::Item::Field(f), ItemCellKind::Field(_)) => {
                    index.fields.entry(f.name.clone()).or_insert(i);
                }
                (ast::Item::Let(l), ItemCellKind::Let(_)) => {
                    index.lets.entry(l.name.clone()).or_insert(i);
                }
                (ast::Item::Block(b), ItemCellKind::Block { .. }) => {
                    index.blocks.entry(b.kind.clone()).or_insert(i);
                }
                _ => {}
            }
        }
        index
    }
}

/// The index of the first item `matches` accepts: a direct scan for a
/// short slice, else a lookup in the slice's [`NameIndex`] (built on
/// first use). `pick` selects the index's map for the item kind sought.
fn position(
    src: &BlockSlice<'_>,
    name: &str,
    pick: impl Fn(&NameIndex) -> &HashMap<String, usize>,
    matches: impl Fn(&ast::Item, &ItemCells) -> bool,
) -> Option<usize> {
    if src.items.len() < INDEX_MIN_ITEMS {
        return src
            .items
            .iter()
            .zip(src.cells)
            .position(|(item, cells)| matches(item, cells));
    }
    let index = src
        .index
        .get_or_init(|| NameIndex::build(src.items, src.cells));
    pick(index).get(name).copied()
}

/// Find a field by name in one source slice.
pub(super) fn find_field<'a>(
    src: &BlockSlice<'a>,
    name: &str,
    doc: &'a Document,
    scope: &Scope<'a>,
) -> Option<Field<'a>> {
    let i = position(
        src,
        name,
        |index| &index.fields,
        |item, cells| {
            matches!((item, &cells.kind),
                (ast::Item::Field(f), ItemCellKind::Field(_)) if f.name == name)
        },
    )?;
    let ast::Item::Field(f) = &src.items[i] else {
        unreachable!("field position points at a Field item")
    };
    Some(Field {
        ast: f,
        cells: &src.cells[i],
        doc,
        file_ns: src.file_ns,
        scope: scope.clone(),
    })
}

/// Find the first block of a kind in one source slice.
pub(super) fn find_block<'a>(
    src: &BlockSlice<'a>,
    kind: &str,
    doc: &'a Document,
    scope: &Scope<'a>,
) -> Option<Block<'a>> {
    let i = position(
        src,
        kind,
        |index| &index.blocks,
        |item, cells| {
            matches!((item, &cells.kind),
                (ast::Item::Block(b), ItemCellKind::Block { .. }) if b.kind == kind)
        },
    )?;
    let ast::Item::Block(b) = &src.items[i] else {
        unreachable!("block position points at a Block item")
    };
    Some(Block {
        ast: b,
        cells: &src.cells[i],
        doc,
        file_ns: src.file_ns,
        kind_override: None,
        scope: scope.clone(),
    })
}

/// Find a `let name = expr` binding by name. Mirrors [`find_field`]
/// but matches `Item::Let`; the resulting [`LetView`] resolves and
/// caches the bound value on demand.
pub(super) fn find_let<'a>(
    src: &BlockSlice<'a>,
    name: &str,
    doc: &'a Document,
    scope: &Scope<'a>,
) -> Option<LetView<'a>> {
    let i = position(
        src,
        name,
        |index| &index.lets,
        |item, cells| {
            matches!((item, &cells.kind),
                (ast::Item::Let(l), ItemCellKind::Let(_)) if l.name == name)
        },
    )?;
    let (ast::Item::Let(l), ItemCellKind::Let(cell)) = (&src.items[i], &src.cells[i].kind) else {
        unreachable!("let position points at a Let item")
    };
    Some(LetView {
        ast: l,
        cell,
        doc,
        scope: scope.clone(),
    })
}

/// Iterate every field across the given source slices.
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

/// Iterate every block across the given source slices.
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

/// Iterate every table item across the given source slices.
pub(super) fn iter_tables<'a>(
    items: &'a [ast::Item],
    doc: &'a Document,
) -> impl Iterator<Item = TableView<'a>> + 'a {
    items.iter().filter_map(move |item| match item {
        ast::Item::Table(t) => Some(TableView { ast: t, doc }),
        _ => None,
    })
}
