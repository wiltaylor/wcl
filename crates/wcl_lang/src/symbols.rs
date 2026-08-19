use std::collections::HashMap;

use crate::ast::Span;

/// Index of named identifiers discovered while parsing a WCL document.
///
/// Built incrementally by the parser: each top-level declaration (and its
/// immediate members) is registered as soon as the parser finishes
/// constructing it, so the index reflects exactly what has been parsed
/// so far. The index keys are fully-qualified, dotted names composed
/// with the current `namespace` prefix, matching the format accepted by
/// [`crate::Document::type_decl`] et al.
///
/// Names that are scoped (function parameters, let-bindings, items
/// nested inside `Block`s) are intentionally NOT indexed — those belong
/// to a scope model the rest of the language doesn't have yet.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SymbolIndex {
    /// Every indexed declaration, keyed by fully-qualified name.
    by_fqn: HashMap<String, SymbolRecord>,
    /// Block instances grouped by kind, so a kind lookup does not scan.
    blocks_by_kind: HashMap<String, Vec<SymbolPath>>,
}

#[derive(Debug, Clone, PartialEq)]
/// One indexed declaration: what it is called, what kind it is, and
/// where to find it.
pub struct SymbolRecord {
    /// Fully-qualified name, namespace included.
    pub fqn: String,
    /// What sort of declaration this is.
    pub kind: SymbolKind,
    /// Source span of the declaration.
    pub span: Span,
    /// How to reach the declaration in the item tree.
    pub path: SymbolPath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// What sort of declaration a [`SymbolRecord`] describes.
pub enum SymbolKind {
    /// A `fn name(…) -> T body` item (an indexed let binding).
    FnDecl,
    /// A `type` declaration.
    TypeDecl,
    /// An `interface` declaration.
    InterfaceDecl,
    /// A `union` declaration.
    UnionDecl,
    /// A `symbol_set` declaration.
    SymbolSetDecl,
    /// A `connection` declaration.
    ConnectionDecl,
    /// A top-level field.
    Field,
    /// A field declared on a `type`.
    TypeField {
        /// Fully-qualified name of the declaration this belongs to.
        parent_fqn: String,
    },
    /// A field declared on an `interface`.
    InterfaceField {
        /// Fully-qualified name of the declaration this belongs to.
        parent_fqn: String,
    },
    /// A variant declared on a `union`.
    UnionVariant {
        /// Fully-qualified name of the declaration this belongs to.
        parent_fqn: String,
    },
    /// A symbol declared in a `symbol_set`.
    SymbolEntry {
        /// Fully-qualified name of the declaration this belongs to.
        parent_fqn: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// A path to a declaration within a document's item tree — the
/// indices to follow from the top level down.
pub struct SymbolPath {
    /// Index into `Source.items`.
    pub item_index: usize,
    /// Index into `TypeDecl.fields` / `UnionDecl.variants` /
    /// `SymbolSetDecl.symbols` for member entries. `None` for top-level.
    pub member_index: Option<usize>,
}

#[derive(Debug, Clone)]
/// Two declarations claimed the same fully-qualified name. Carries
/// both spans so the diagnostic can point at each.
pub(crate) struct DuplicateSymbol {
    /// Where the name was first declared.
    pub first_span: Span,
    /// Where it was declared again.
    pub second_span: Span,
}

impl SymbolIndex {
    /// The declaration with this fully-qualified name.
    pub fn lookup(&self, fqn: &str) -> Option<&SymbolRecord> {
        self.by_fqn.get(fqn)
    }

    /// Whether any declaration claims this name.
    pub fn contains(&self, fqn: &str) -> bool {
        self.by_fqn.contains_key(fqn)
    }

    /// Every indexed declaration, in unspecified order.
    pub fn iter(&self) -> impl Iterator<Item = &SymbolRecord> {
        self.by_fqn.values()
    }

    /// How many declarations are indexed.
    pub fn len(&self) -> usize {
        self.by_fqn.len()
    }

    /// Whether nothing is indexed.
    pub fn is_empty(&self) -> bool {
        self.by_fqn.is_empty()
    }

    /// All indexed block paths with the given block kind, in source order.
    pub fn blocks_with_kind(&self, kind: &str) -> &[SymbolPath] {
        self.blocks_by_kind
            .get(kind)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Iterate (kind, paths) pairs.
    pub fn block_kinds(&self) -> impl Iterator<Item = (&String, &Vec<SymbolPath>)> {
        self.blocks_by_kind.iter()
    }

    /// Index one declaration, failing if its name is already taken.
    pub(crate) fn insert(&mut self, rec: SymbolRecord) -> Result<(), DuplicateSymbol> {
        if let Some(existing) = self.by_fqn.get(&rec.fqn) {
            return Err(DuplicateSymbol {
                first_span: existing.span,
                second_span: rec.span,
            });
        }
        self.by_fqn.insert(rec.fqn.clone(), rec);
        Ok(())
    }

    /// Record a block instance under its kind.
    pub(crate) fn push_block(&mut self, kind: String, path: SymbolPath) {
        self.blocks_by_kind.entry(kind).or_default().push(path);
    }
}
