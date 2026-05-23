//! `textDocument/definition` + `textDocument/references` request
//! handlers. Both run the same identifier resolver and then either
//! return the declaration span or scan the source for every textual
//! occurrence of the resolved identifier.

use tower_lsp::lsp_types::{GotoDefinitionResponse, Location, Range, Url};
use wcl_lang::{Document, parse_for_edit};

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
    let ast = parse_for_edit(source, uri.as_str()).ok()?;
    let (sym, _) = resolve::locate(&doc, &ast, source, offset)?;
    // Cross-file: if the resolved FQN lives in an imported source,
    // surface that file's URI instead of the request URI.
    let (location_uri, span) = match symbol_fqn(&sym) {
        Some(fqn) => match doc.find_symbol(fqn) {
            Some(hit) => {
                let target = hit
                    .source_path
                    .and_then(|p| Url::from_file_path(p).ok())
                    .unwrap_or_else(|| uri.clone());
                (target, hit.record.span)
            }
            None => (uri.clone(), resolve::declaration_span(&doc, &sym)?),
        },
        None => (uri.clone(), resolve::declaration_span(&doc, &sym)?),
    };
    // For cross-file hits we want the range computed against the
    // *target* file's source, not the request's. We only have the
    // request source here — fall back to span-on-request when the
    // file matches; otherwise emit a zero-based range and let the
    // editor open the file to the offset.
    let range = if location_uri == uri {
        span_to_range(source, span)
    } else {
        // Read the target file lazily to compute line/col. If the
        // read fails (transient I/O), report the same as a request-
        // file range — the byte offsets still help the editor.
        match location_uri
            .to_file_path()
            .ok()
            .and_then(|p| std::fs::read_to_string(p).ok())
        {
            Some(text) => span_to_range(&text, span),
            None => span_to_range(source, span),
        }
    };
    Some(GotoDefinitionResponse::Scalar(Location {
        uri: location_uri,
        range,
    }))
}

/// FQN-bearing variants for cross-file lookup. `Local` has no FQN
/// (it's a function param / let binding); `UnionVariant`/`SymbolEntry`
/// nest under a parent FQN but their target source is the same file
/// as the parent — falling back to `declaration_span` is fine.
fn symbol_fqn(sym: &LocatedSymbol) -> Option<&str> {
    match sym {
        LocatedSymbol::Type(f)
        | LocatedSymbol::Decorator(f)
        | LocatedSymbol::BlockKind(f)
        | LocatedSymbol::Field(f) => Some(f.as_str()),
        LocatedSymbol::Local { .. }
        | LocatedSymbol::UnionVariant { .. }
        | LocatedSymbol::SymbolEntry { .. } => None,
    }
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
    let ast = parse_for_edit(source, uri.as_str()).ok()?;
    let (sym, _) = resolve::locate(&doc, &ast, source, offset)?;
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
        LocatedSymbol::Type(f)
        | LocatedSymbol::Decorator(f)
        | LocatedSymbol::BlockKind(f)
        | LocatedSymbol::Field(f) => f.as_str(),
        LocatedSymbol::UnionVariant { variant, .. } => variant.as_str(),
        LocatedSymbol::SymbolEntry { entry, .. } => entry.as_str(),
        LocatedSymbol::Local { name, .. } => name.as_str(),
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

    #[test]
    fn find_symbol_returns_imported_file_path() {
        // Verifies the cross-file plumbing: a Document opened from a
        // file with an `import`d sibling exposes the import's path
        // via `find_symbol`. The LSP handler uses this to build a
        // `Location` pointing at the imported file when go-to-def
        // resolves a cross-file FQN.
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("shared.wcl");
        let main = dir.path().join("main.wcl");
        std::fs::write(&shared, "namespace shared\ntype Color {\n  name: utf8\n}\n").unwrap();
        std::fs::write(&main, "import \"./shared.wcl\"\n").unwrap();
        let doc = wcl_lang::Document::from_file(&main).expect("open main");
        let hit = doc.find_symbol("shared.Color").expect("hit");
        let target = Url::from_file_path(hit.source_path.expect("imported path")).unwrap();
        assert_eq!(target, Url::from_file_path(&shared).unwrap());
    }
}
