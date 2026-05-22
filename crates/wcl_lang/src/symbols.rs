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
    by_fqn: HashMap<String, SymbolRecord>,
    blocks_by_kind: HashMap<String, Vec<SymbolPath>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SymbolRecord {
    pub fqn: String,
    pub kind: SymbolKind,
    pub span: Span,
    pub path: SymbolPath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    TypeDecl,
    InterfaceDecl,
    UnionDecl,
    SymbolSetDecl,
    ConnectionDecl,
    Field,
    TypeField { parent_fqn: String },
    InterfaceField { parent_fqn: String },
    UnionVariant { parent_fqn: String },
    SymbolEntry { parent_fqn: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SymbolPath {
    /// Index into `Source.items`.
    pub item_index: usize,
    /// Index into `TypeDecl.fields` / `UnionDecl.variants` /
    /// `SymbolSetDecl.symbols` for member entries. `None` for top-level.
    pub member_index: Option<usize>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // fqn / first_span are kept for future richer diagnostics
pub(crate) struct DuplicateSymbol {
    pub fqn: String,
    pub first_span: Span,
    pub second_span: Span,
}

impl SymbolIndex {
    pub fn lookup(&self, fqn: &str) -> Option<&SymbolRecord> {
        self.by_fqn.get(fqn)
    }

    pub fn contains(&self, fqn: &str) -> bool {
        self.by_fqn.contains_key(fqn)
    }

    pub fn iter(&self) -> impl Iterator<Item = &SymbolRecord> {
        self.by_fqn.values()
    }

    pub fn len(&self) -> usize {
        self.by_fqn.len()
    }

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

    pub(crate) fn insert(&mut self, rec: SymbolRecord) -> Result<(), DuplicateSymbol> {
        if let Some(existing) = self.by_fqn.get(&rec.fqn) {
            return Err(DuplicateSymbol {
                fqn: rec.fqn.clone(),
                first_span: existing.span,
                second_span: rec.span,
            });
        }
        self.by_fqn.insert(rec.fqn.clone(), rec);
        Ok(())
    }

    pub(crate) fn push_block(&mut self, kind: String, path: SymbolPath) {
        self.blocks_by_kind.entry(kind).or_default().push(path);
    }
}
