//! `textDocument/definition` + `textDocument/references` request
//! handlers. Both run the same identifier resolver and then either
//! return the declaration span or scan the source for every textual
//! occurrence of the resolved identifier.

use tower_lsp::lsp_types::{GotoDefinitionResponse, Location, Range, Url};
use wcl_lang::Document;

use crate::convert::span_to_range;
use crate::resolve::{self, LocatedSymbol};

/// Go-to-definition for `(uri, offset)`. Returns `None` when the
/// cursor isn't on an identifier we can resolve, or when the symbol
/// has no AST declaration site (e.g. a builtin decorator).
pub(crate) fn goto_definition(
    uri: Url,
    source: &str,
    offset: usize,
) -> Option<GotoDefinitionResponse> {
    let doc = Document::open(source, uri.as_str()).ok()?;
    let (sym, _) = resolve::locate(&doc, source, offset)?;
    let span = resolve::declaration_span(&doc, &sym)?;
    Some(GotoDefinitionResponse::Scalar(Location {
        uri,
        range: span_to_range(source, span),
    }))
}

/// Find every occurrence of the identifier under the cursor. The
/// match is whole-word on raw source bytes — good enough for the
/// kinds of identifiers we resolve (type names, decorator names,
/// block kinds, union variants). Cross-file references are not
/// resolved in this slice.
pub(crate) fn references(
    uri: Url,
    source: &str,
    offset: usize,
    include_declaration: bool,
) -> Option<Vec<Location>> {
    let doc = Document::open(source, uri.as_str()).ok()?;
    let (sym, _) = resolve::locate(&doc, source, offset)?;
    let needle = display_name(&sym);
    let decl_span = resolve::declaration_span(&doc, &sym);
    let mut out = Vec::new();
    let mut decl_seen = false;
    for (start, end) in whole_word_matches(source, &needle) {
        // SymbolRecord.span covers the entire declaration form (e.g.
        // `type Foo { ... }`), not just the name. Treat the first
        // whole-word match that falls inside that span as the decl.
        let inside_decl = decl_span.is_some_and(|s| start >= s.start && end <= s.end);
        let is_decl = inside_decl && !decl_seen;
        if is_decl {
            decl_seen = true;
            if !include_declaration {
                continue;
            }
        }
        out.push(Location {
            uri: uri.clone(),
            range: Range {
                start: crate::convert::offset_to_position(source, start),
                end: crate::convert::offset_to_position(source, end),
            },
        });
    }
    Some(out)
}

/// The identifier as it appears in source for a given symbol. For
/// dotted FQNs we take the last segment — that's what's actually
/// typed at use sites.
fn display_name(sym: &LocatedSymbol) -> String {
    let fqn = match sym {
        LocatedSymbol::Type(f) | LocatedSymbol::Decorator(f) | LocatedSymbol::BlockKind(f) => {
            f.as_str()
        }
        LocatedSymbol::UnionVariant { variant, .. } => variant.as_str(),
        LocatedSymbol::SymbolEntry { entry, .. } => entry.as_str(),
    };
    fqn.rsplit('.').next().unwrap_or(fqn).to_string()
}

/// Return `(start, end)` byte spans of every whole-word match of
/// `needle` in `source`. A "whole word" is bordered by either a
/// non-identifier byte or the document boundary.
fn whole_word_matches(source: &str, needle: &str) -> Vec<(usize, usize)> {
    if needle.is_empty() {
        return Vec::new();
    }
    let bytes = source.as_bytes();
    let nbytes = needle.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + nbytes.len() <= bytes.len() {
        if &bytes[i..i + nbytes.len()] == nbytes {
            let before_ok = i == 0 || !is_ident(bytes[i - 1]);
            let after_ok = i + nbytes.len() == bytes.len() || !is_ident(bytes[i + nbytes.len()]);
            if before_ok && after_ok {
                out.push((i, i + nbytes.len()));
                i += nbytes.len();
                continue;
            }
        }
        i += 1;
    }
    out
}

fn is_ident(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url() -> Url {
        Url::parse("file:///test.wcl").unwrap()
    }

    #[test]
    fn goto_jumps_to_block_kind_decl() {
        let src = "@document\ntype Root {\n  c: Config\n}\n@block(\"config\")\ntype Config {\n  region: utf8\n}\nconfig {\n  region = \"x\"\n}\n";
        let cursor = src.find("config {").unwrap() + 2;
        let resp = goto_definition(url(), src, cursor).expect("def found");
        let GotoDefinitionResponse::Scalar(loc) = resp else {
            panic!("expected scalar")
        };
        // SymbolRecord.span covers the full `@block(...)\ntype Config {...}`
        // form. We assert the range starts somewhere before `type Config`
        // and includes that line.
        let type_kw = src.find("type Config").unwrap();
        let decl_start = crate::convert::offset_to_position(src, type_kw);
        assert!(loc.range.start <= decl_start);
        assert!(loc.range.end > decl_start);
    }

    #[test]
    fn references_returns_decl_and_uses() {
        let src = "@document\ntype Root {\n  v: Foo\n}\n@block(\"foo\")\ntype Foo {\n  x: utf8\n}\nfoo {\n  x = \"a\"\n}\nfoo {\n  x = \"b\"\n}\n";
        // Cursor on the type-ref "Foo" in `v: Foo`.
        let cursor = src.find("v: Foo").unwrap() + 3;
        let locs = references(url(), src, cursor, true).expect("some refs");
        // Should include the declaration "type Foo" and the "v: Foo" use,
        // but not the lowercase block kind "foo".
        assert_eq!(locs.len(), 2, "found: {locs:#?}");
    }

    #[test]
    fn references_excludes_decl_when_requested() {
        let src =
            "@document\ntype Root {\n  v: Foo\n}\n@block(\"foo\")\ntype Foo {\n  x: utf8\n}\n";
        let cursor = src.find("v: Foo").unwrap() + 3;
        let with_decl = references(url(), src, cursor, true).unwrap();
        let no_decl = references(url(), src, cursor, false).unwrap();
        assert_eq!(with_decl.len(), no_decl.len() + 1);
    }
}
