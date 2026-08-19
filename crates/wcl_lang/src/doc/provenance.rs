//! Provenance: which file declared a given item, and the `NamedSource`
//! a diagnostic against it should render with.
//!
//! A document is one root source plus its imports, so a span alone does
//! not say which text it indexes into. Everything here answers that
//! question, mostly by comparing node addresses against each source's
//! items — identity, not equality, since two files can declare
//! structurally identical nodes.

use std::path::Path;

use miette::NamedSource;

use crate::ast;
use crate::symbols::SymbolIndex;
use crate::symbols::SymbolRecord;

use super::cells::{self, ItemCellKind, ItemCells, LoadedImport};
use super::{Document, SymbolHit};

impl Document {
    /// Borrow the importer's items + cells + symbols followed by every
    /// eagerly-imported file's items + cells + symbols (recursively).
    /// Used by all of `field` / `block` / `type_decl` etc. so imports
    /// are searched after the importer.
    /// Locate the file that owns `target_field` by pointer-identity.
    /// Returns `None` when the field lives in the document's main
    /// source (the file the host opened directly); returns the
    /// originating import's `path` when the field came in through an
    /// eager top-level import or an already-forced in-block lazy
    /// import.
    ///
    /// Always returning `None` for the main source keeps `Document`
    /// out of the business of tracking its own filesystem path — the
    /// CLI (or other host) already has that string from
    /// `Document::from_file(path)` and doesn't need it round-tripped.
    pub(crate) fn find_field_source_path(&self, target: *const ast::Field) -> Option<&Path> {
        // Main file first — if we find it there, the answer is None.
        if field_in_items(&self.ast.items, target, &self.cells.items) {
            return None;
        }
        // Eager imports (and their transitive eagers) carry their
        // own path; descend until we find a match.
        for imp in &self.eager_imports {
            if let Some(p) = find_in_import(imp, target) {
                return Some(p);
            }
        }
        // Lazy in-block imports inside the main file. Their
        // `LoadedImport` is populated on first access — if a CLI
        // caller drove `Document::get` over a path that crossed
        // them, the cell is filled and we can recover the path.
        find_lazy_in_blocks(&self.ast.items, &self.cells.items, target)
    }

    /// Like [`find_field_source_path`] but returns the source's
    /// `file_ns` (the namespace declared at the top of that file).
    /// Falls back to the document's own `file_ns` when the field
    /// isn't located in any known source — callers treat the main
    /// document as the default.
    pub(crate) fn find_field_source_ns(&self, target: *const ast::Field) -> &[String] {
        if field_in_items(&self.ast.items, target, &self.cells.items) {
            return &self.file_ns;
        }
        for imp in &self.eager_imports {
            if let Some(ns) = find_field_ns_in_import(imp, target) {
                return ns;
            }
        }
        find_lazy_field_ns_in_blocks(&self.ast.items, &self.cells.items, target)
            .unwrap_or(&self.file_ns)
    }

    /// miette source (name + text) for the root document.
    pub(super) fn root_named_source(&self) -> NamedSource<String> {
        NamedSource::new(self.src.name(), self.src.inner().clone())
    }

    /// The `NamedSource` a diagnostic against this source should
    /// render with.
    pub(super) fn named_source_for_view(&self, source: SourceView<'_>) -> NamedSource<String> {
        match source.path {
            Some(path) => NamedSource::new(path.display().to_string(), source.source.to_string()),
            None => self.root_named_source(),
        }
    }

    /// The miette source (name + text) of the file that declares the
    /// block `target` points into — the root document, or the imported
    /// file that carries it. Hosts (e.g. the wdoc renderer) use this to
    /// render an eval diagnostic against the correct file's snippet rather
    /// than always against the root source (whose offsets won't match a
    /// cross-file span — the cause of the `OutOfBounds` misrender). Falls
    /// back to the root source when the block can't be located (e.g. a
    /// synthesised block that isn't backed by on-disk AST).
    pub fn named_source_for_block(&self, target: *const ast::Block) -> NamedSource<String> {
        if block_in_items(&self.ast.items, target) {
            return self.root_named_source();
        }
        if let Some(source) =
            named_source_for_block_in_lazy(&self.ast.items, &self.cells.items, target)
        {
            return source;
        }
        for imp in &self.eager_imports {
            if let Some(src) = named_source_in_import(imp, target) {
                return src;
            }
        }
        self.root_named_source()
    }

    /// Locate the source declaring `target` and build its
    /// `NamedSource`, so a union diagnostic points at the right file.
    pub(super) fn named_source_for_union(
        &self,
        target: *const ast::UnionDecl,
    ) -> NamedSource<String> {
        if union_in_items(&self.ast.items, target) {
            return self.root_named_source();
        }
        for imp in &self.eager_imports {
            if let Some(source) = named_source_for_union_in_import(imp, target) {
                return source;
            }
        }
        self.root_named_source()
    }

    /// Locate the source declaring `target` and build its
    /// `NamedSource`, so a type diagnostic points at the right file.
    pub(super) fn named_source_for_type(
        &self,
        target: *const ast::TypeDecl,
    ) -> NamedSource<String> {
        if type_in_items(&self.ast.items, target) {
            return self.root_named_source();
        }
        for import in &self.eager_imports {
            if let Some(source) = named_source_for_type_in_import(import, target) {
                return source;
            }
        }
        self.root_named_source()
    }

    /// The root source followed by every eagerly-loaded import, as a
    /// uniform list so lookups need not special-case the root.
    pub(super) fn all_sources(&self) -> Vec<SourceView<'_>> {
        let mut out = vec![SourceView {
            symbols: &self.symbols,
            items: &self.ast.items,
            cells: &self.cells.items,
            source: self.src.inner(),
            file_ns: &self.file_ns,
            path: None,
        }];
        fn push_imports<'a>(imports: &'a [LoadedImport], out: &mut Vec<SourceView<'a>>) {
            for imp in imports {
                out.push(SourceView {
                    symbols: &imp.symbols,
                    items: &imp.items,
                    cells: &imp.cells,
                    source: &imp.source,
                    file_ns: &imp.file_ns,
                    path: Some(imp.path.as_path()),
                });
                push_imports(&imp.eager_imports, out);
            }
        }
        push_imports(&self.eager_imports, &mut out);
        out
    }

    /// Paths of every eagerly-loaded import reachable from this
    /// document, deduplicated. Used by tooling (e.g. the LSP) that
    /// needs to scan imported source files — for instance, to find
    /// cross-file references to a symbol.
    pub fn imported_paths(&self) -> Vec<&Path> {
        let mut out = Vec::new();
        fn walk<'a>(imports: &'a [LoadedImport], out: &mut Vec<&'a Path>) {
            for imp in imports {
                let p = imp.path.as_path();
                if !out.contains(&p) {
                    out.push(p);
                }
                walk(&imp.eager_imports, out);
            }
        }
        walk(&self.eager_imports, &mut out);
        out
    }

    /// Every symbol across this document and its eagerly-loaded import
    /// graph, paired with the file path of the source that declares it
    /// (`None` for the root document). The projection reuses the
    /// already-built per-source symbol indexes — nothing is re-parsed.
    /// Hosts use this for workspace-wide symbol search.
    pub fn all_symbols(&self) -> impl Iterator<Item = (Option<&Path>, &SymbolRecord)> {
        self.all_sources()
            .into_iter()
            .flat_map(|src| src.symbols.iter().map(move |rec| (src.path, rec)))
    }

    /// Lookup a fully-qualified symbol across this document and every
    /// eagerly-loaded import. Returns the matching `SymbolRecord`
    /// together with the file path of the source it lives in (`None`
    /// for the root document). Hosts use this for cross-file
    /// go-to-definition.
    pub fn find_symbol(&self, fqn: &str) -> Option<SymbolHit<'_>> {
        for src in self.all_sources() {
            if let Some(record) = src.symbols.lookup(fqn) {
                return Some(SymbolHit {
                    record,
                    source_path: src.path,
                });
            }
        }
        None
    }
}

/// Pointer-identity walk used by `Field::source_path`. Returns
/// `true` if `target_field` lives directly in any `Item::Field` of
/// `items`, or inside an `Item::Block`'s nested items. Lazy in-block
/// imports are searched separately by [`find_lazy_in_blocks`] so the
/// path-bearing variant can return the import's `PathBuf`.
/// `true` when `target` points at a [`ast::Block`] reachable from `items`
/// (recursing through nested blocks). Block identity is by pointer, so
/// this only matches blocks backed by on-disk AST — synthesised blocks
/// (table rows, computed children, component expansions) aren't found.
fn block_in_items(items: &[ast::Item], target: *const ast::Block) -> bool {
    for item in items {
        if let ast::Item::Block(b) = item {
            if std::ptr::eq(b, target) {
                return true;
            }
            if block_in_items(&b.items, target) {
                return true;
            }
        }
    }
    false
}

/// Whether `target` is one of these items, compared by address —
/// the identity test behind provenance lookup.
fn union_in_items(items: &[ast::Item], target: *const ast::UnionDecl) -> bool {
    items
        .iter()
        .any(|item| matches!(item, ast::Item::UnionDecl(union) if std::ptr::eq(union, target)))
}

/// Whether `target` is one of these items, compared by address.
fn type_in_items(items: &[ast::Item], target: *const ast::TypeDecl) -> bool {
    items
        .iter()
        .any(|item| matches!(item, ast::Item::TypeDecl(declaration) if std::ptr::eq(declaration, target)))
}

/// The miette source (name + text) of `imp` (or a transitive eager
/// import) when it declares the block `target` points into.
fn named_source_in_import(
    imp: &cells::LoadedImport,
    target: *const ast::Block,
) -> Option<NamedSource<String>> {
    if block_in_items(&imp.items, target) {
        return Some(NamedSource::new(
            imp.path.display().to_string(),
            imp.source.clone(),
        ));
    }
    if let Some(source) = named_source_for_block_in_lazy(&imp.items, &imp.cells, target) {
        return Some(source);
    }
    for child in &imp.eager_imports {
        if let Some(src) = named_source_in_import(child, target) {
            return Some(src);
        }
    }
    None
}

/// Find the lazily-loaded import declaring this block and build its
/// `NamedSource`.
fn named_source_for_block_in_lazy(
    items: &[ast::Item],
    cells: &[ItemCells],
    target: *const ast::Block,
) -> Option<NamedSource<String>> {
    for (item, cell) in items.iter().zip(cells) {
        match (item, &cell.kind) {
            (ast::Item::Block(block), ItemCellKind::Block { items, .. }) => {
                if let Some(source) = named_source_for_block_in_lazy(&block.items, items, target) {
                    return Some(source);
                }
            }
            (ast::Item::Import(_), ItemCellKind::Import { loaded, .. }) => {
                let Some(Ok(import)) = loaded.get() else {
                    continue;
                };
                if let Some(source) = named_source_in_import(import, target) {
                    return Some(source);
                }
            }
            _ => {}
        }
    }
    None
}

/// Find the import declaring this union and build its `NamedSource`.
fn named_source_for_union_in_import(
    imp: &cells::LoadedImport,
    target: *const ast::UnionDecl,
) -> Option<NamedSource<String>> {
    if union_in_items(&imp.items, target) {
        return Some(NamedSource::new(
            imp.path.display().to_string(),
            imp.source.clone(),
        ));
    }
    for child in &imp.eager_imports {
        if let Some(source) = named_source_for_union_in_import(child, target) {
            return Some(source);
        }
    }
    None
}

/// Find the import declaring this type and build its `NamedSource`.
fn named_source_for_type_in_import(
    import: &cells::LoadedImport,
    target: *const ast::TypeDecl,
) -> Option<NamedSource<String>> {
    if type_in_items(&import.items, target) {
        return Some(NamedSource::new(
            import.path.display().to_string(),
            import.source.clone(),
        ));
    }
    for child in &import.eager_imports {
        if let Some(source) = named_source_for_type_in_import(child, target) {
            return Some(source);
        }
    }
    None
}

/// Whether `target` is one of these items or nested inside one,
/// compared by address.
fn field_in_items(items: &[ast::Item], target: *const ast::Field, cells: &[ItemCells]) -> bool {
    for (i, item) in items.iter().enumerate() {
        match item {
            ast::Item::Field(f) => {
                if std::ptr::eq(f, target) {
                    return true;
                }
            }
            ast::Item::Block(b) => {
                let block_cells = match cells.get(i).map(|c| &c.kind) {
                    Some(ItemCellKind::Block { items: inner, .. }) => inner.as_slice(),
                    _ => &[],
                };
                if field_in_items(&b.items, target, block_cells) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Recursively search a [`LoadedImport`] (and any in-block lazy
/// imports it owns) for `target`. The match's enclosing import's
/// `path` is returned via the first enclosing scope that owns the
/// item — never the deepest, so a field in shared.wcl reports
/// shared.wcl even if it's inside a block.
fn find_in_import(imp: &cells::LoadedImport, target: *const ast::Field) -> Option<&Path> {
    if field_in_items(&imp.items, target, &imp.cells) {
        return Some(&imp.path);
    }
    if let Some(p) = find_lazy_in_blocks(&imp.items, &imp.cells, target) {
        return Some(p);
    }
    for child in &imp.eager_imports {
        if let Some(p) = find_in_import(child, target) {
            return Some(p);
        }
    }
    None
}

/// Like [`find_in_import`] but returns the import's `file_ns`.
fn find_field_ns_in_import(
    imp: &cells::LoadedImport,
    target: *const ast::Field,
) -> Option<&[String]> {
    if field_in_items(&imp.items, target, &imp.cells) {
        return Some(&imp.file_ns);
    }
    if let Some(ns) = find_lazy_field_ns_in_blocks(&imp.items, &imp.cells, target) {
        return Some(ns);
    }
    for child in &imp.eager_imports {
        if let Some(ns) = find_field_ns_in_import(child, target) {
            return Some(ns);
        }
    }
    None
}

/// Like [`find_lazy_in_blocks`] but returns the originating import's
/// `file_ns` rather than its path.
fn find_lazy_field_ns_in_blocks<'a>(
    items: &'a [ast::Item],
    cells: &'a [ItemCells],
    target: *const ast::Field,
) -> Option<&'a [String]> {
    for (i, item) in items.iter().enumerate() {
        let Some(cell) = cells.get(i) else { continue };
        if let ast::Item::Block(b) = item {
            let block_cells = match &cell.kind {
                ItemCellKind::Block { items: inner, .. } => inner.as_slice(),
                _ => continue,
            };
            for (j, inner_item) in b.items.iter().enumerate() {
                let Some(inner_cell) = block_cells.get(j) else {
                    continue;
                };
                if let ast::Item::Import(_) = inner_item
                    && let ItemCellKind::Import { loaded, .. } = &inner_cell.kind
                    && let Some(Ok(li)) = loaded.get()
                    && let Some(ns) = find_field_ns_in_import(li, target)
                {
                    return Some(ns);
                }
            }
            if let Some(ns) = find_lazy_field_ns_in_blocks(&b.items, block_cells, target) {
                return Some(ns);
            }
        }
    }
    None
}

/// Walk `items`+`cells` looking for `ItemCellKind::Import` cells
/// whose lazy `loaded` slot has been forced. Each forced
/// `LoadedImport` is searched via [`find_in_import`].
fn find_lazy_in_blocks<'a>(
    items: &'a [ast::Item],
    cells: &'a [ItemCells],
    target: *const ast::Field,
) -> Option<&'a Path> {
    for (i, item) in items.iter().enumerate() {
        let Some(cell) = cells.get(i) else { continue };
        if let ast::Item::Block(b) = item {
            let block_cells = match &cell.kind {
                ItemCellKind::Block { items: inner, .. } => inner.as_slice(),
                _ => continue,
            };
            // Lazy `import` statements live in this block's cells:
            // check those first, then recurse into nested blocks.
            for (j, inner_item) in b.items.iter().enumerate() {
                let Some(inner_cell) = block_cells.get(j) else {
                    continue;
                };
                if let ast::Item::Import(_) = inner_item
                    && let ItemCellKind::Import { loaded, .. } = &inner_cell.kind
                    && let Some(Ok(li)) = loaded.get()
                    && let Some(p) = find_in_import(li, target)
                {
                    return Some(p);
                }
            }
            if let Some(p) = find_lazy_in_blocks(&b.items, block_cells, target) {
                return Some(p);
            }
        }
    }
    None
}

/// A homogeneous view over one source of top-level items — either the
/// importer's own source or an eagerly-loaded import.
#[derive(Clone, Copy)]
pub(super) struct SourceView<'a> {
    /// Name index for this source.
    pub(super) symbols: &'a SymbolIndex,
    /// The source's top-level items.
    pub(super) items: &'a [ast::Item],
    /// Evaluation caches, index-aligned with `items`.
    pub(super) cells: &'a [ItemCells],
    /// The raw text, for rendering diagnostics against this source.
    pub(super) source: &'a str,
    /// Namespace this source declares.
    pub(super) file_ns: &'a [String],
    /// Resolved path on disk. `None` for the root document (the host
    /// typically supplies that path itself, e.g. via the LSP request
    /// URI); `Some` for every eagerly-loaded import.
    pub(super) path: Option<&'a Path>,
}
