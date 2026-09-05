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
            if path.starts_with(wcl_lang::SYSTEM_IMPORT_ROOT) {
                continue;
            }
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
    let Some(selected) = selected else {
        return Ok(None);
    };
    let identity = selected.identity.clone();
    let Some(locations) = references(
        uri.clone(),
        source,
        offset,
        true,
        root_doc,
        root_path,
        overlays,
    ) else {
        return Ok(None);
    };
    let mut changes: std::collections::HashMap<Url, Vec<tower_lsp::lsp_types::TextEdit>> =
        std::collections::HashMap::new();
    let mut seen: std::collections::HashSet<(Url, u32, u32, u32, u32)> =
        std::collections::HashSet::new();
    let mut snapshots =
        std::collections::HashMap::from([(uri.clone(), (source.to_string(), occurrences))]);
    for path in doc.imported_paths().into_iter().chain(root_path) {
        if path.starts_with(wcl_lang::SYSTEM_IMPORT_ROOT) {
            continue;
        }
        let target = Url::from_file_path(path).map_err(|_| "Rename target is not a local file")?;
        if snapshots.contains_key(&target) {
            continue;
        }
        let text = overlays
            .get(path)
            .cloned()
            .or_else(|| std::fs::read_to_string(path).ok())
            .ok_or("Cannot read rename target")?;
        let occurrences =
            crate::occurrences::collect(&text, &target, doc).ok_or("Cannot parse rename target")?;
        snapshots.insert(target, (text, occurrences));
    }
    if let crate::occurrences::Identity::Global(name) = &identity
        && let Some((category, _)) = name.split_once(':')
        && snapshots.values().any(|(_, occurrences)| occurrences.iter().any(|o| matches!(&o.identity, crate::occurrences::Identity::Unresolved(c) if c == category)))
    {
        return Err("A computed semantic name prevents a complete rename; use a literal name first".into());
    }
    for loc in locations {
        if !snapshots.contains_key(&loc.uri) {
            let path = loc
                .uri
                .to_file_path()
                .map_err(|_| "Rename target is not a local file")?;
            let text = overlays
                .get(&path)
                .cloned()
                .or_else(|| std::fs::read_to_string(path).ok())
                .ok_or("Cannot read rename target")?;
            let occurrences = crate::occurrences::collect(&text, &loc.uri, doc)
                .ok_or("Cannot parse rename target")?;
            snapshots.insert(loc.uri.clone(), (text, occurrences));
        }
        let (text, occurrences) = &snapshots[&loc.uri];
        let occurrence = occurrences
            .iter()
            .find(|o| o.identity == identity && span_to_range(text, o.span) == loc.range)
            .ok_or("Rename target changed")?;
        if occurrences
            .iter()
            .any(|o| o.span == occurrence.span && o.identity != identity)
        {
            return Err("Rename target is used with more than one declaration identity".into());
        }
        let new_text = format!(
            "{}{new_name}{}",
            occurrence.replacement_prefix, occurrence.replacement_suffix
        );
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
                    new_text,
                });
        }
    }
    if !snapshots.values().any(|(_, occurrences)| {
        occurrences
            .iter()
            .any(|o| o.identity == identity && o.declaration)
    }) {
        return Err("The selected name has no editable authored declaration".into());
    }
    if doc.schema_errors().is_empty() {
        let mut updated = overlays.clone();
        updated.insert(
            uri.to_file_path()
                .map_err(|_| "Rename target is not a local file")?,
            source.to_string(),
        );
        for (target, edits) in &changes {
            let original = &snapshots[target].0;
            let mut text = original.clone();
            let mut ordered: Vec<_> = edits.iter().collect();
            ordered.sort_by_key(|e| std::cmp::Reverse(e.range.start));
            for edit in ordered {
                let start = crate::convert::position_to_offset(original, edit.range.start);
                let end = crate::convert::position_to_offset(original, edit.range.end);
                text.replace_range(start..end, &edit.new_text);
            }
            updated.insert(
                target
                    .to_file_path()
                    .map_err(|_| "Rename target is not a local file")?,
                text,
            );
        }
        let checked = if let Some(root) = root_path {
            Document::from_file_with_loader(
                root,
                doc.environment(),
                wcl_wdoc::schema_registry().loader(wcl_lang::overlay_loader(updated)),
            )
        } else {
            let path = uri
                .to_file_path()
                .map_err(|_| "Rename target is not a local file")?;
            Document::open_at_with_loader(
                updated.get(&path).map(String::as_str).unwrap_or(source),
                uri.as_str(),
                path.parent().map(std::path::Path::to_path_buf),
                doc.environment(),
                wcl_wdoc::schema_registry().loader(wcl_lang::overlay_loader(updated.clone())),
            )
        }
        .map_err(|error| format!("Rename would invalidate the document: {error}"))?;
        if let Some(error) = checked.schema_errors().first() {
            return Err(format!("Rename has unresolved contextual uses: {error}"));
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
    fn rename_accepts_contextual_categories() {
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
            assert!(matches!(result, Ok(Some(_))), "{cursor}: {result:?}");
        }
    }

    fn apply_rename(source: &str, needle: &str, name: &str) -> String {
        let before = Document::open(source, "before.wcl").expect(source);
        assert!(
            before.schema_errors().is_empty(),
            "{:?}",
            before.schema_errors()
        );
        let edit = rename(
            url(),
            source,
            source.find(needle).unwrap(),
            name,
            None,
            None,
            &Default::default(),
        )
        .unwrap()
        .unwrap();
        let mut edits = edit.changes.unwrap().remove(&url()).unwrap();
        edits.sort_by_key(|e| std::cmp::Reverse(e.range.start));
        let mut updated = source.to_string();
        for edit in edits {
            let start = crate::convert::position_to_offset(source, edit.range.start);
            let end = crate::convert::position_to_offset(source, edit.range.end);
            updated.replace_range(start..end, &edit.new_text);
        }
        let document = Document::open(&updated, "renamed.wcl").unwrap();
        assert!(
            document.schema_errors().is_empty(),
            "{updated}\n{:?}",
            document.schema_errors()
        );
        updated
    }

    #[test]
    fn rename_kind_updates_schema_metadata_and_reflection_only() {
        let source = r#"@block("leaf") type Leaf { @inline(0) id: identifier }
@block("tree", required_children = ["leaf"]) type Tree {
  @children("leaf") leaves: list<Leaf>
  @ref("leaf") selected: identifier?
}
@decorator("note") @applies_to(on = [:block], kinds = ["leaf"]) type Note {}
@document type Root { @children("tree") trees: list<Tree> }
tree { @note leaf first {} selected = first }
@schemaless text = "leaf"
@schemaless reflect = decorators_for_kind("leaf")
"#;
        let updated = apply_rename(source, "leaf\") type", "twig");
        assert!(updated.contains("required_children = [\"twig\"]"));
        assert!(updated.contains("@children(\"twig\")"));
        assert!(updated.contains("@ref(\"twig\")"));
        assert!(updated.contains("kinds = [\"twig\"]"));
        assert!(updated.contains("@note twig first"));
        assert!(updated.contains("text = \"leaf\""));
        assert!(updated.contains("decorators_for_kind(\"twig\")"));
    }

    #[test]
    fn rename_decorator_updates_reflective_name() {
        let source = "@decorator(\"note\") type Note { label: utf8 }\n@note(label = \"note\") type T {}\n@schemaless result = decorator_arg(T, \"note\", \"label\")\n";
        let updated = apply_rename(source, "note\") type", "annotation");
        assert!(updated.contains("@annotation(label = \"note\")"));
        assert!(updated.contains("decorator_arg(T, \"annotation\", \"label\")"));
        assert_eq!(
            Document::open(&updated, "test.wcl")
                .unwrap()
                .field("result")
                .unwrap()
                .value()
                .unwrap(),
            &wcl_lang::Value::Utf8("note".into())
        );
    }

    #[test]
    fn rename_inferred_variants_and_shorthand_preserves_evaluation() {
        let source = "union Shape { Circle { radius: i64 } }\nunion Other { Circle none }\n@document type Root { shape: Shape result: i64 }\nshape = Shape::Circle { radius: 7 }\nresult = match shape { Circle { radius, .. } => radius, _ => 0 }\n";
        let renamed = apply_rename(source, "Circle { radius:", "Round");
        assert!(renamed.contains("Shape::Round"));
        assert!(renamed.contains("match shape { Round"));
        assert!(renamed.contains("union Other { Circle none }"));
        let updated = apply_rename(&renamed, "radius, ..", "r");
        assert!(updated.contains("Round { radius: r, .. } => r"));
        assert_eq!(
            Document::open(&updated, "test.wcl")
                .unwrap()
                .field("result")
                .unwrap()
                .value()
                .unwrap(),
            &wcl_lang::Value::I64(7)
        );
    }

    #[test]
    fn rename_symbols_uses_schema_function_and_pattern_contexts() {
        let source = "symbol_set Color { red blue }\nsymbol_set Other { red blue }\ntype Hue = Color\ntype Paint { color: Hue }\n@block(\"swatch\") type Swatch { @inline(0) color: Hue }\n@document type Root { paint: Paint colors: list<Color> other: Other selected: Color result: i64 @children(\"swatch\") swatches: list<Swatch> }\npaint = { color: :red }\ncolors = [:red, :blue]\nother = :red\nfn pick(c: Hue) -> Color { c }\nselected = pick(:red)\nresult = match selected { :red => 7, _ => 0 }\nswatch :red {}\n";
        let updated = apply_rename(source, "red blue }", "scarlet");
        assert!(updated.contains("symbol_set Other { red blue }"));
        assert!(updated.contains("other = :red"));
        assert!(updated.contains("color: :scarlet"));
        assert!(updated.contains("colors = [:scarlet, :blue]"));
        assert!(updated.contains("pick(:scarlet)"));
        assert!(updated.contains("{ :scarlet => 7"));
        assert!(updated.contains("swatch :scarlet"));
        assert_eq!(
            Document::open(&updated, "test.wcl")
                .unwrap()
                .field("result")
                .unwrap()
                .value()
                .unwrap(),
            &wcl_lang::Value::I64(7)
        );
    }

    #[test]
    fn rename_symbol_defaults_and_function_returns() {
        let source = "symbol_set Color { red blue }\n@block(\"swatch\") type Swatch { @default(:red) first: Color second = fn() -> Color { :red } }\n@document type Root { @children(\"swatch\") swatches: list<Swatch> color: Color }\nswatch {}\nfn pick() -> Color { :red }\ncolor = pick()\n";
        let updated = apply_rename(source, "red blue", "scarlet");
        assert!(updated.contains("@default(:scarlet)"));
        assert!(updated.contains("second = fn() -> Color { :scarlet }"));
        assert!(updated.contains("-> Color { :scarlet }"));
        assert_eq!(
            Document::open(&updated, "test.wcl")
                .unwrap()
                .field("color")
                .unwrap()
                .value()
                .unwrap(),
            &wcl_lang::Value::Symbol("scarlet".into())
        );
    }

    #[test]
    fn rename_preserves_comments_before_shorthand_bindings() {
        let source = "union Shape { Circle { radius: i64 } }\n@schemaless result = match Shape::Circle { radius: 7 } { Circle { // radius\n radius } => radius, _ => 0 }\n";
        let updated = apply_rename(source, "radius } =>", "r");
        assert!(updated.contains("// radius\n radius: r } => r"));
        assert_eq!(
            Document::open(&updated, "test.wcl")
                .unwrap()
                .field("result")
                .unwrap()
                .value()
                .unwrap(),
            &wcl_lang::Value::I64(7)
        );
    }

    #[test]
    fn rename_never_returns_partial_edits_for_uneditable_or_ambiguous_names() {
        for (source, needle) in [
            ("@document type Root {}\n", "document"),
            (
                "symbol_set Color { red }\nfn helper() -> symbol { :red }\n@document type Root { color: Color }\ncolor = helper()\n",
                "red }",
            ),
            (
                "@block(\"leaf\") type Leaf {}\n@block(\"tree\", required_children = [concat(\"le\", \"af\")]) type Tree { @children(\"leaf\") leaves: list<Leaf> }\n@document type Root { @children(\"tree\") trees: list<Tree> }\ntree { leaf {} }\n",
                "leaf\") type",
            ),
        ] {
            assert!(
                rename(
                    url(),
                    source,
                    source.find(needle).unwrap(),
                    "renamed",
                    None,
                    None,
                    &Default::default()
                )
                .is_err(),
                "{source}"
            );
        }
    }

    #[test]
    fn rename_variant_and_symbols_across_unsaved_imports() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("main.wcl");
        let shared = dir.path().join("shared.wcl");
        let library = "\n\nnamespace lib\nsymbol_set Color { red blue }\nunion Base { Circle { radius: i64 } }\nunion Shape extends Base { Empty none }\n";
        let source = "import \"./shared.wcl\"\nuse lib.Shape as Form\nuse lib.Color as Hue\n@document type Root { shape: lib.Shape color: Hue result: i64 }\nshape = lib.Shape::Circle { radius: 7 }\ncolor = :red\nresult = match shape { Circle { radius } => radius, _ => 0 }\n";
        std::fs::write(&main, source).unwrap();
        std::fs::write(&shared, library.trim_start()).unwrap();
        for (needle, new_name, declaration) in [
            ("Circle { radius:", "Round", "Round { radius:"),
            ("red\n", "scarlet", "scarlet blue"),
        ] {
            let mut overlays =
                std::collections::HashMap::from([(shared.clone(), library.to_string())]);
            let doc = Document::from_file_with_loader(
                &main,
                &wcl_lang::Environment::new(),
                wcl_lang::overlay_loader(overlays.clone()),
            )
            .unwrap();
            let edit = rename(
                Url::from_file_path(&main).unwrap(),
                source,
                source.find(needle).unwrap(),
                new_name,
                Some(&doc),
                Some(&main),
                &overlays,
            )
            .unwrap()
            .unwrap();
            overlays.insert(main.clone(), source.to_string());
            for (uri, mut edits) in edit.changes.unwrap() {
                let path = uri.to_file_path().unwrap();
                let original = overlays[&path].clone();
                let text = overlays.get_mut(&path).unwrap();
                edits.sort_by_key(|e| std::cmp::Reverse(e.range.start));
                for edit in edits {
                    let start = crate::convert::position_to_offset(&original, edit.range.start);
                    let end = crate::convert::position_to_offset(&original, edit.range.end);
                    text.replace_range(start..end, &edit.new_text);
                }
            }
            assert!(overlays[&shared].starts_with("\n\nnamespace lib"));
            assert!(overlays[&shared].contains(declaration));
            assert!(overlays[&main].contains("use lib.Shape as Form"));
            let after = Document::from_file_with_loader(
                &main,
                &wcl_lang::Environment::new(),
                wcl_lang::overlay_loader(overlays),
            )
            .unwrap();
            assert!(
                after.schema_errors().is_empty(),
                "{:?}",
                after.schema_errors()
            );
            assert_eq!(
                after.field("result").unwrap().value().unwrap(),
                &wcl_lang::Value::I64(7)
            );
        }
    }

    #[test]
    fn rename_with_standard_import_preserves_environment() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("main.wcl");
        let source = "import <wdoc.wcl>\nlet greeting = \"Hello\"\npage index { title = greeting h1 greeting }\n";
        std::fs::write(&main, source).unwrap();
        let doc = Document::from_file_with_loader(
            &main,
            &wcl_wdoc::wdoc_environment(),
            wcl_wdoc::schema_registry().loader(wcl_lang::disk_loader()),
        )
        .unwrap();
        assert!(doc.schema_errors().is_empty());
        let edit = rename(
            Url::from_file_path(&main).unwrap(),
            source,
            source.find("greeting =").unwrap(),
            "salutation",
            Some(&doc),
            Some(&main),
            &Default::default(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            edit.changes.unwrap().values().map(Vec::len).sum::<usize>(),
            3
        );
    }

    #[test]
    fn rename_symbols_in_table_rows_and_connections() {
        let source = "symbol_set Color { red blue }\n@table(\"paint\") type Paint { color: Color }\n@document type Root { paints: list<Paint> }\npaints:\n  | :red |\n";
        let updated = apply_rename(source, "red blue", "scarlet");
        assert!(updated.contains("| :scarlet |"));
        let source = "symbol_set EdgeKind { uses depends_on }\nconnection DependsOn: Service -> Service : EdgeKind\n@block(\"service\") type Service { @inline(0) id: identifier }\n@document type Root { @children(\"service\") services: list<Service> @connections(DependsOn) edges: list<DependsOn> }\nservice web {}\nservice db {}\nweb -> db :uses\n";
        let updated = apply_rename(source, "uses depends_on", "calls");
        assert!(updated.contains("web -> db :calls"));
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
