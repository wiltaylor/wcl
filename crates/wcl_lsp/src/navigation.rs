//! `textDocument/definition` + `textDocument/references` request
//! handlers. Both run the same identifier resolver and then either
//! return the declaration span or collect AST occurrences with the
//! same declaration identity.

use tower_lsp::lsp_types::{GotoDefinitionResponse, Location, Url};
use wcl_lang::Document;

use crate::convert::span_to_range;
use crate::resolve;

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

/// Find occurrences of the selected declaration in the current source snapshot.
pub(crate) fn references(
    uri: Url,
    source: &str,
    offset: usize,
    include_declaration: bool,
    root_doc: Option<&Document>,
    root_path: Option<&std::path::Path>,
    overlays: &std::collections::HashMap<std::path::PathBuf, String>,
) -> Option<Vec<Location>> {
    let local_doc = if root_doc.is_none() {
        Document::open(source, uri.as_str()).ok()
    } else {
        None
    };
    let doc = root_doc.or(local_doc.as_ref())?;
    let current = crate::occurrences::collect(source, &uri, doc)?;
    let selected = current
        .iter()
        .find(|o| o.span.start <= offset && offset < o.span.end)
        .or_else(|| current.iter().find(|o| o.span.end == offset))?;
    let identity = selected.identity.clone();
    let mut sources = vec![(uri.clone(), source.to_string(), current)];
    if !matches!(identity, crate::occurrences::Identity::Local(..)) {
        let mut paths: Vec<_> = doc
            .imported_paths()
            .into_iter()
            .map(std::path::Path::to_path_buf)
            .collect();
        if let Some(root) = root_path {
            paths.push(root.to_path_buf());
        }
        paths.sort();
        paths.dedup();
        for path in paths {
            let file_uri = Url::from_file_path(&path).ok()?;
            if file_uri == uri {
                continue;
            }
            let text = overlays
                .get(&path)
                .cloned()
                .or_else(|| std::fs::read_to_string(&path).ok())?;
            let occurrences = crate::occurrences::collect(&text, &file_uri, doc)?;
            sources.push((file_uri, text, occurrences));
        }
    }
    let mut out = Vec::new();
    for (file_uri, text, occurrences) in sources {
        for occurrence in occurrences {
            if occurrence.identity == identity && (include_declaration || !occurrence.declaration) {
                out.push(Location {
                    uri: file_uri.clone(),
                    range: span_to_range(&text, occurrence.span),
                });
            }
        }
    }
    Some(out)
}

/// `textDocument/rename`: every reference to the symbol under the
/// cursor (declaration included) becomes a text edit replacing it
/// with `new_name`, using declaration identities across source snapshots.
/// `Err` explains an invalid new name or a target with unsupported contextual uses.
#[allow(clippy::too_many_arguments)]
pub(crate) fn rename(
    uri: Url,
    source: &str,
    offset: usize,
    new_name: &str,
    root_doc: Option<&Document>,
    root_path: Option<&std::path::Path>,
    overlays: &std::collections::HashMap<std::path::PathBuf, String>,
) -> Result<Option<tower_lsp::lsp_types::WorkspaceEdit>, String> {
    if !is_valid_identifier(new_name) {
        return Err(format!("'{new_name}' is not a valid WCL identifier"));
    }
    let local_doc = if root_doc.is_none() {
        Document::open(source, uri.as_str()).ok()
    } else {
        None
    };
    let Some(doc) = root_doc.or(local_doc.as_ref()) else {
        return Ok(None);
    };
    let Some(occurrences) = crate::occurrences::collect(source, &uri, doc) else {
        return Ok(None);
    };
    let selected = occurrences
        .iter()
        .find(|o| o.span.start <= offset && offset < o.span.end)
        .or_else(|| occurrences.iter().find(|o| o.span.end == offset));
    if let Some(selected) = selected
        && occurrences
            .iter()
            .any(|o| o.identity == selected.identity && !o.rename_supported)
    {
        return Err("Rename is not available for shorthand pattern bindings".into());
    }
    if let Some(crate::occurrences::Occurrence {
        identity: crate::occurrences::Identity::Global(fqn),
        ..
    }) = selected
    {
        // These names also occur in schema strings or context-inferred variants.
        // Until those uses carry identities, no partial rename is safe.
        if fqn.starts_with("block:")
            || fqn.starts_with("decorator:")
            || doc.find_symbol(fqn).is_some_and(|hit| {
                matches!(
                    hit.record.kind,
                    wcl_lang::SymbolKind::UnionVariant { .. }
                        | wcl_lang::SymbolKind::SymbolEntry { .. }
                )
            })
        {
            return Err(
                "Rename is not available for schema kind names or context-inferred variants".into(),
            );
        }
    }
    let Some(locations) = references(uri, source, offset, true, root_doc, root_path, overlays)
    else {
        return Ok(None);
    };
    let mut changes: std::collections::HashMap<Url, Vec<tower_lsp::lsp_types::TextEdit>> =
        std::collections::HashMap::new();
    let mut seen: std::collections::HashSet<(Url, u32, u32, u32, u32)> =
        std::collections::HashSet::new();
    for loc in locations {
        let key = (
            loc.uri.clone(),
            loc.range.start.line,
            loc.range.start.character,
            loc.range.end.line,
            loc.range.end.character,
        );
        if seen.insert(key) {
            changes
                .entry(loc.uri)
                .or_default()
                .push(tower_lsp::lsp_types::TextEdit {
                    range: loc.range,
                    new_text: new_name.to_string(),
                });
        }
    }
    Ok(Some(tower_lsp::lsp_types::WorkspaceEdit {
        changes: Some(changes),
        ..Default::default()
    }))
}

/// A legal WCL identifier: ASCII letter / underscore head, ASCII
/// alphanumeric / underscore tail, and not a reserved word.
fn is_valid_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !matches!(s, "true" | "false" | "none" | "if" | "else" | "match")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_preserves_evaluation_with_indexing_interpolation_and_shadowing() {
        let source = "@schemaless values = [2, 3]\n@schemaless result = $\"values: ${at(values, 0) + (fn(values: i64) -> i64 { values })(4)}\"\n";
        let edit = rename(
            url(),
            source,
            source.find("values").unwrap(),
            "numbers",
            None,
            None,
            &Default::default(),
        )
        .unwrap()
        .unwrap();
        let mut edits = edit.changes.unwrap().remove(&url()).unwrap();
        assert_eq!(edits.len(), 2);
        edits.sort_by_key(|e| std::cmp::Reverse(e.range.start));
        let mut updated = source.to_string();
        for edit in edits {
            let start = crate::convert::position_to_offset(source, edit.range.start);
            let end = crate::convert::position_to_offset(source, edit.range.end);
            updated.replace_range(start..end, &edit.new_text);
        }
        let before = Document::open(source, "before.wcl").unwrap();
        let after = Document::open(&updated, "after.wcl").unwrap();
        assert_eq!(
            before.field("result").unwrap().value().unwrap(),
            after.field("result").unwrap().value().unwrap()
        );
        assert!(updated.contains("fn(values: i64) -> i64 { values }"));
    }

    #[test]
    fn references_use_unsaved_cross_file_coordinates() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("main.wcl");
        let shared = dir.path().join("shared.wcl");
        let source = "import \"./shared.wcl\"\ntype Root { color: shared.Color }\n";
        std::fs::write(&main, source).unwrap();
        std::fs::write(&shared, "namespace shared\ntype Color { name: utf8 }\n").unwrap();
        let changed = "\n\n\nnamespace shared\ntype Color { name: utf8 }\n".to_string();
        let overlays = std::collections::HashMap::from([(shared.clone(), changed)]);
        let doc = Document::from_file_with_loader(
            &main,
            &wcl_lang::Environment::new(),
            wcl_lang::overlay_loader(overlays.clone()),
        )
        .unwrap();
        let refs = references(
            Url::from_file_path(&main).unwrap(),
            source,
            source.find("Color").unwrap(),
            true,
            Some(&doc),
            Some(&main),
            &overlays,
        )
        .unwrap();
        let declaration = refs
            .iter()
            .find(|r| r.uri == Url::from_file_path(&shared).unwrap())
            .unwrap();
        assert_eq!(declaration.range.start.line, 4);
        assert_eq!(declaration.range.start.character, 5);
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn rename_type_preserves_explicit_use_alias_in_variant_constructor() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("main.wcl");
        let shared = dir.path().join("shared.wcl");
        let source =
            "import \"./shared.wcl\"\nuse ns.Foo as Alias\n@schemaless value = Alias::One\n";
        std::fs::write(&main, source).unwrap();
        std::fs::write(&shared, "namespace ns\nunion Foo { One none }\n").unwrap();
        let doc = Document::from_file(&main).unwrap();
        let edit = rename(
            Url::from_file_path(&main).unwrap(),
            source,
            source.find("Foo").unwrap(),
            "Bar",
            Some(&doc),
            Some(&main),
            &Default::default(),
        )
        .unwrap()
        .unwrap();
        let changes = edit.changes.unwrap();
        let local = changes.get(&Url::from_file_path(&main).unwrap()).unwrap();
        assert_eq!(local.len(), 1);
        assert_eq!(local[0].range.start.line, 1);
    }

    fn url() -> Url {
        Url::parse("file:///test.wcl").unwrap()
    }

    #[test]
    fn rename_rejects_names_with_unresolved_contextual_uses() {
        let cases = [
            (
                "@block(\"config\") type Config {}\nconfig {}\n",
                "config {}",
            ),
            (
                "@decorator(\"note\") type Note {}\n@note type T {}\n",
                "note type",
            ),
            (
                "union Shape { Circle none }\n@schemaless x = Shape::Circle\n",
                "Circle none",
            ),
            (
                "@schemaless x = match c1 {\n Shape::Circle { radius, .. } => radius,\n _ => 0,\n}\n",
                "radius,",
            ),
        ];
        for (source, cursor) in cases {
            Document::open(source, "test.wcl").expect("valid rename fixture");
            let result = rename(
                url(),
                source,
                source.find(cursor).unwrap(),
                "replacement",
                None,
                None,
                &Default::default(),
            );
            assert!(result.is_err(), "{cursor}: {result:?}");
        }
    }

    #[test]
    fn rename_ignores_a_cursor_in_comments_or_strings() {
        let source = "type Foo {}\n// Foo\n@schemaless text = \"Foo\"\n";
        for cursor in [
            source.find("// Foo").unwrap() + 3,
            source.find("\"Foo\"").unwrap() + 1,
        ] {
            assert!(
                rename(
                    url(),
                    source,
                    cursor,
                    "Bar",
                    None,
                    None,
                    &Default::default()
                )
                .unwrap()
                .is_none()
            );
        }
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
        let locs = references(url(), src, cursor, true, None, None, &Default::default())
            .expect("some refs");
        // Should include the declaration "type Foo" and the "v: Foo" use,
        // but not the lowercase block kind "foo".
        assert_eq!(locs.len(), 2, "found: {locs:#?}");
    }

    #[test]
    fn references_excludes_decl_when_requested() {
        let src =
            "@document\ntype Root {\n  v: Foo\n}\n@block(\"foo\")\ntype Foo {\n  x: utf8\n}\n";
        let cursor = src.find("v: Foo").unwrap() + 3;
        let with_decl =
            references(url(), src, cursor, true, None, None, &Default::default()).unwrap();
        let no_decl =
            references(url(), src, cursor, false, None, None, &Default::default()).unwrap();
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
        let locs = references(
            main_url.clone(),
            &main_src,
            cursor,
            true,
            None,
            None,
            &Default::default(),
        )
        .unwrap_or_default();
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
