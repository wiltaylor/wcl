//! Document symbol (outline) extraction.
//!
//! Walks `Document::symbols()` and emits an LSP `DocumentSymbol` per
//! [`SymbolRecord`]. `SymbolIndex` indexes top-level declarations plus
//! their immediate members (type fields, union variants, symbol-set
//! entries) — that maps cleanly onto a single-level outline.

#[allow(deprecated)] // DocumentSymbol::deprecated is required by lsp-types
use tower_lsp_server::ls_types::{DocumentSymbol, SymbolKind as LspSymbolKind};
use wcl_lang::{SymbolIndex, SymbolKind, SymbolRecord};

use crate::convert::LineIndex;
use crate::ctx::Ctx;

/// Build a flat list of document symbols for `source`. A file with
/// syntax errors still gets an outline: the declarations of every item
/// that parsed (the diagnostics path surfaces the errors themselves).
pub(crate) fn compute(ctx: &Ctx, source: &str, uri: &str) -> Vec<DocumentSymbol> {
    let index = ctx.index(source);
    // `SymbolIndex` is hash-keyed, so its iteration order is arbitrary.
    // An outline reads top to bottom, so order by source position.
    let to_symbols = |symbols: &SymbolIndex| {
        let mut records: Vec<&SymbolRecord> = symbols.iter().collect();
        records.sort_by_key(|rec| (rec.span.start, rec.span.end));
        records
            .into_iter()
            .map(|rec| record_to_symbol(&index, rec))
            .collect()
    };
    match ctx.open(source, uri) {
        Ok(doc) => to_symbols(doc.symbols()),
        Err(_) => to_symbols(&wcl_lang::parse_for_edit_recovering(source, uri).symbols),
    }
}

/// Convert an indexed declaration into the outline entry a client
/// shows.
fn record_to_symbol(index: &LineIndex<'_>, rec: &SymbolRecord) -> DocumentSymbol {
    let range = index.range(rec.span);
    let (kind, detail) = classify(&rec.kind);
    #[allow(deprecated)]
    DocumentSymbol {
        name: short_name(&rec.fqn),
        detail,
        kind,
        tags: None,
        deprecated: None,
        range,
        selection_range: range,
        children: None,
    }
}

/// Map a `wcl_lang` symbol kind onto the LSP kind plus the parent FQN
/// (as `detail`/`containerName` context) for member symbols. Shared by
/// the document outline and workspace symbol search.
pub(crate) fn classify(kind: &SymbolKind) -> (LspSymbolKind, Option<String>) {
    match kind {
        SymbolKind::FnDecl => (LspSymbolKind::FUNCTION, None),
        SymbolKind::TypeDecl => (LspSymbolKind::CLASS, None),
        SymbolKind::InterfaceDecl => (LspSymbolKind::INTERFACE, None),
        SymbolKind::UnionDecl => (LspSymbolKind::ENUM, None),
        SymbolKind::SymbolSetDecl => (LspSymbolKind::NAMESPACE, None),
        SymbolKind::ConnectionDecl => (LspSymbolKind::EVENT, None),
        SymbolKind::Field => (LspSymbolKind::FIELD, None),
        SymbolKind::TypeField { parent_fqn } => (LspSymbolKind::FIELD, Some(parent_fqn.clone())),
        SymbolKind::InterfaceField { parent_fqn } => {
            (LspSymbolKind::FIELD, Some(parent_fqn.clone()))
        }
        SymbolKind::UnionVariant { parent_fqn } => {
            (LspSymbolKind::ENUM_MEMBER, Some(parent_fqn.clone()))
        }
        SymbolKind::SymbolEntry { parent_fqn } => {
            (LspSymbolKind::CONSTANT, Some(parent_fqn.clone()))
        }
    }
}

/// Trim the leading namespace prefix off an FQN for display. The
/// `detail` field already carries parent context for members.
fn short_name(fqn: &str) -> String {
    fqn.rsplit('.').next().unwrap_or(fqn).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_decl_and_field_appear_in_outline() {
        let src = "@document\ntype Root {\n  name: utf8\n}\nname = \"alpha\"\n";
        let syms = compute(&Ctx::new(Default::default()), src, "test.wcl");
        let names: Vec<_> = syms.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Root"), "missing Root: {names:?}");
        assert!(names.contains(&"name"), "missing name field: {names:?}");
        let root = syms.iter().find(|s| s.name == "Root").unwrap();
        assert_eq!(root.kind, LspSymbolKind::CLASS);
    }

    #[test]
    fn parse_failure_keeps_the_symbols_that_parsed() {
        let src = "type Kept {\n  name: utf8\n}\nbroken = = 1\ntype Broken {";
        let syms = compute(&Ctx::new(Default::default()), src, "test.wcl");
        let names: Vec<_> = syms.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["Kept", "name"]);
    }
}
