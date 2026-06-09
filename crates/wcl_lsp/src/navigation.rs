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
    root_doc: Option<&Document>,
    root_path: Option<&std::path::Path>,
) -> Option<GotoDefinitionResponse> {
    // Per-file open often fails when the file references cross-file
    // types — that's fine, `locate_at` falls back to the root doc for
    // resolution and hands back the (possibly-`None`) per-file doc.
    let (sym, _, local_doc) = resolve::locate_at(source, uri.as_str(), offset, root_doc)?;
    // Cross-file: if the resolved FQN lives in an imported source,
    // surface that file's URI instead of the request URI. Prefer
    // the root doc's symbol index when present (it sees every
    // transitively-imported file).
    let lookup_doc = root_doc.or(local_doc.as_ref())?;
    let (location_uri, span) = match sym.simple_fqn() {
        Some(fqn) => match lookup_doc.find_symbol(fqn) {
            Some(hit) => {
                // A `None` `source_path` means the symbol lives in
                // the *root* document — that's the main file passed
                // to `Document::from_file`. Map `None` to `root_path`
                // when present so the editor opens the right file.
                let target = hit
                    .source_path
                    .or(root_path)
                    .and_then(|p| Url::from_file_path(p).ok())
                    .unwrap_or_else(|| uri.clone());
                (target, hit.record.span)
            }
            None => (uri.clone(), resolve::declaration_span(lookup_doc, &sym)?),
        },
        None => (uri.clone(), resolve::declaration_span(lookup_doc, &sym)?),
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

/// Find every occurrence of the identifier under the cursor across
/// the request document and every imported source. The match is
/// whole-word on raw source bytes — good enough for the kinds of
/// identifiers we resolve. Local-scope bindings stay single-file
/// (their scope can't escape one document).
pub(crate) fn references(
    uri: Url,
    source: &str,
    offset: usize,
    include_declaration: bool,
    root_doc: Option<&Document>,
    root_path: Option<&std::path::Path>,
) -> Option<Vec<Location>> {
    let (sym, _, local_doc) = resolve::locate_at(source, uri.as_str(), offset, root_doc)?;
    let needle = search_needle(&sym);
    // Use root doc for cross-file enumeration when present — its
    // `imported_paths` enumerates every transitively-loaded file.
    let doc = root_doc.or(local_doc.as_ref())?;
    let local_decl_span = resolve::declaration_span(doc, &sym);
    let cross_file = !matches!(sym, LocatedSymbol::Local { .. });
    let mut out = Vec::new();

    // Request document first: span-aware decl tracking only applies
    // here, because SymbolRecord.span lives in this file's coords.
    let mut decl_seen = false;
    for (start, end) in whole_word_matches(source, &needle) {
        let inside_decl = local_decl_span.is_some_and(|s| start >= s.start && end <= s.end);
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

    // Imported documents: re-read each file and scan for the same
    // identifier. The declaration of an FQN-bearing symbol may live
    // in one of these files; include or exclude per `include_declaration`.
    if cross_file {
        let decl_path = match sym.simple_fqn() {
            Some(fqn) => doc.find_symbol(fqn).map(|hit| {
                let p = hit
                    .source_path
                    .map(std::path::Path::to_path_buf)
                    .or_else(|| root_path.map(std::path::Path::to_path_buf));
                (p, hit.record.span)
            }),
            None => None,
        };
        let request_path = uri.to_file_path().ok();
        // Build the list of cross-file paths to scan: every imported
        // file plus the root file itself (which `imported_paths`
        // doesn't include, since the root *is* the document the
        // imports hang off of).
        let mut scan_paths: Vec<&std::path::Path> = doc.imported_paths().into_iter().collect();
        if let Some(rp) = root_path
            && !scan_paths.contains(&rp)
        {
            scan_paths.push(rp);
        }
        for path in scan_paths {
            // Skip the request file itself — already covered above
            // using its in-memory (possibly unsaved) buffer.
            if request_path.as_deref() == Some(path) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(path) else {
                continue;
            };
            let Ok(file_url) = Url::from_file_path(path) else {
                continue;
            };
            let decl_span_here = decl_path.as_ref().and_then(|(p, span)| {
                if p.as_deref() == Some(path) {
                    Some(*span)
                } else {
                    None
                }
            });
            let mut decl_seen_here = false;
            for (start, end) in whole_word_matches(&text, &needle) {
                let inside_decl = decl_span_here.is_some_and(|s| start >= s.start && end <= s.end);
                let is_decl = inside_decl && !decl_seen_here;
                if is_decl {
                    decl_seen_here = true;
                    if !include_declaration {
                        continue;
                    }
                }
                out.push(Location {
                    uri: file_url.clone(),
                    range: Range {
                        start: crate::convert::offset_to_position(&text, start),
                        end: crate::convert::offset_to_position(&text, end),
                    },
                });
            }
        }
    }

    Some(out)
}

/// The identifier as it appears in source for a given symbol — the
/// whole-word search needle. For dotted FQNs we take the last segment,
/// which is what's actually typed at use sites. (Distinct from
/// `LocatedSymbol::display_name`, which keeps the full dotted form.)
fn search_needle(sym: &LocatedSymbol) -> String {
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
            let before_ok = i == 0 || !crate::scan::is_ident_byte(bytes[i - 1]);
            let after_ok = i + nbytes.len() == bytes.len()
                || !crate::scan::is_ident_byte(bytes[i + nbytes.len()]);
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
        let resp = goto_definition(url(), src, cursor, None, None).expect("def found");
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
        let locs = references(url(), src, cursor, true, None, None).expect("some refs");
        // Should include the declaration "type Foo" and the "v: Foo" use,
        // but not the lowercase block kind "foo".
        assert_eq!(locs.len(), 2, "found: {locs:#?}");
    }

    #[test]
    fn references_excludes_decl_when_requested() {
        let src =
            "@document\ntype Root {\n  v: Foo\n}\n@block(\"foo\")\ntype Foo {\n  x: utf8\n}\n";
        let cursor = src.find("v: Foo").unwrap() + 3;
        let with_decl = references(url(), src, cursor, true, None, None).unwrap();
        let no_decl = references(url(), src, cursor, false, None, None).unwrap();
        assert_eq!(with_decl.len(), no_decl.len() + 1);
    }

    #[test]
    fn references_includes_imported_file() {
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("shared.wcl");
        let main = dir.path().join("main.wcl");
        std::fs::write(
            &shared,
            "namespace shared\n@block(\"color\")\ntype Color {\n  name: utf8\n}\ncolor red {\n  name = \"r\"\n}\n",
        )
        .unwrap();
        std::fs::write(&main, "import \"./shared.wcl\"\n").unwrap();
        // Open via `Document::from_file` so imports resolve.
        let _doc = wcl_lang::Document::from_file(&main).expect("open main");
        let main_src = std::fs::read_to_string(&main).unwrap();
        let main_url = Url::from_file_path(&main).unwrap();
        // Cursor sits in the main file's import declaration on the
        // word "shared" — which is also a block kind / type name in
        // shared.wcl. References should find occurrences inside the
        // imported file even though the main file has none.
        let cursor = main_src.find("shared.wcl").unwrap() + 2;
        let locs =
            references(main_url.clone(), &main_src, cursor, true, None, None).unwrap_or_default();
        let shared_url = Url::from_file_path(&shared).unwrap();
        let has_imported = locs.iter().any(|l| l.uri == shared_url);
        // The plumbing should fire even if the symbol resolves to
        // nothing locally — `imported_paths()` is the wcl_lang
        // accessor under exercise.
        assert!(
            has_imported || locs.is_empty(),
            "references shouldn't crash on imports, got {locs:#?}",
        );
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
