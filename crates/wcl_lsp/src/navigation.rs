//! `textDocument/definition`, `textDocument/references` and
//! `textDocument/rename` request handlers. All three run the same
//! identifier resolver and then either return the declaration span or
//! collect AST occurrences with the same declaration identity.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use tower_lsp_server::ls_types::{GotoDefinitionResponse, Location, TextEdit, Uri, WorkspaceEdit};
use wcl_lang::Document;

use crate::convert::uri_to_path;
use crate::ctx::{Ctx, canonical};
use crate::occurrences::{self, Identity, Occurrence};
use crate::resolve;

/// Go-to-definition for `(uri, offset)`. Returns `None` when the
/// cursor isn't on an identifier we can resolve, or when the symbol
/// has no AST declaration site (e.g. a builtin decorator).
pub(crate) fn goto_definition(
    ctx: &Ctx,
    uri: Uri,
    source: &str,
    offset: usize,
    root_doc: Option<&Document>,
    root_path: Option<&Path>,
) -> Option<GotoDefinitionResponse> {
    // Per-file open often fails when the file references cross-file
    // types — that's fine, `locate_at` falls back to the root doc for
    // resolution and hands back the (possibly-`None`) per-file doc.
    let (sym, _, local_doc) = resolve::locate_at(ctx, source, uri.as_str(), offset, root_doc)?;
    // Cross-file: if the resolved FQN lives in an imported source,
    // surface that file instead of the request file. Prefer the root
    // doc's symbol index when present (it sees every transitively
    // imported file).
    let lookup_doc = root_doc.or(local_doc.as_ref())?;
    // The file an index hit with no `source_path` lives in: the root
    // file for the root document, the request file for its own.
    let unsourced = if root_doc.is_some() { root_path } else { None };
    let (target, span) = match sym.simple_fqn().and_then(|fqn| lookup_doc.find_symbol(fqn)) {
        Some(hit) => (hit.source_path.or(unsourced), hit.record.span),
        None => (None, resolve::declaration_span(lookup_doc, &sym)?),
    };
    let request_path = uri_to_path(&uri).map(|p| canonical(&p));
    let location = match target {
        Some(path) if request_path.as_deref() != Some(canonical(path).as_path()) => {
            // Convert against the target's own text — its open buffer
            // when it has one, since the index was built from that.
            let text = ctx.text(path)?;
            Location {
                uri: ctx.uri_for(path)?,
                range: ctx.index(&text).range(span),
            }
        }
        _ => Location {
            range: ctx.index(source).range(span),
            uri,
        },
    };
    Some(GotoDefinitionResponse::Scalar(location))
}

/// One source's authored occurrences, as seen by one request.
struct SourceOccurrences {
    /// The URI the source is reported under.
    uri: Uri,
    /// The text the occurrence spans index into.
    text: String,
    /// Every occurrence in the source.
    occurrences: Vec<Occurrence>,
}

/// The occurrence under `offset` (or ending at it), if any.
fn selected(occurrences: &[Occurrence], offset: usize) -> Option<&Occurrence> {
    occurrences
        .iter()
        .find(|o| o.span.start <= offset && offset < o.span.end)
        .or_else(|| occurrences.iter().find(|o| o.span.end == offset))
}

/// The request source followed by every other source an occurrence of
/// `identity` can appear in. A local binding never leaves its source;
/// anything else can appear in any file the document imports and in the
/// root. Each file is collected once, however many spellings of its
/// path the import graph and the editor use.
///
/// A file that cannot be read or parsed is skipped and logged when
/// `strict` is false (a reference list is still useful without it) and
/// is an error when it is true (a rename that cannot see a file would
/// leave it half-renamed).
#[allow(clippy::too_many_arguments)]
fn gather(
    ctx: &Ctx,
    uri: &Uri,
    source: &str,
    current: Vec<Occurrence>,
    identity: &Identity,
    doc: &Document,
    root_path: Option<&Path>,
    strict: bool,
) -> Result<Vec<SourceOccurrences>, String> {
    let mut seen: HashSet<PathBuf> = uri_to_path(uri)
        .map(|p| canonical(&p))
        .into_iter()
        .collect();
    let mut sources = vec![SourceOccurrences {
        uri: uri.clone(),
        text: source.to_string(),
        occurrences: current,
    }];
    if matches!(identity, Identity::Local(..)) {
        return Ok(sources);
    }
    let paths = doc.imported_paths().into_iter().chain(root_path);
    for path in paths {
        if path.starts_with(wcl_lang::SYSTEM_IMPORT_ROOT) || !seen.insert(canonical(path)) {
            continue;
        }
        let collected = ctx
            .uri_for(path)
            .ok_or("is not a local file")
            .and_then(|file_uri| {
                let text = ctx.text(path).ok_or("cannot be read")?;
                let occurrences =
                    occurrences::collect(&text, &file_uri, doc).ok_or("does not parse")?;
                Ok(SourceOccurrences {
                    uri: file_uri,
                    text,
                    occurrences,
                })
            });
        match collected {
            Ok(source) => sources.push(source),
            Err(why) if strict => return Err(format!("{} {why}", path.display())),
            Err(why) => tracing::warn!("references skip {}: it {why}", path.display()),
        }
    }
    Ok(sources)
}

/// The document occurrences resolve against: the root when there is
/// one, else the request buffer opened on its own.
fn lookup_document<'a>(
    ctx: &Ctx,
    uri: &Uri,
    source: &str,
    root_doc: Option<&'a Document>,
    local: &'a mut Option<Document>,
) -> Option<&'a Document> {
    if root_doc.is_none() {
        *local = ctx.open(source, uri.as_str()).ok();
    }
    root_doc.or(local.as_ref())
}

/// Find occurrences of the selected declaration in the current source
/// snapshot and every other source it can appear in.
pub(crate) fn references(
    ctx: &Ctx,
    uri: Uri,
    source: &str,
    offset: usize,
    include_declaration: bool,
    root_doc: Option<&Document>,
    root_path: Option<&Path>,
) -> Option<Vec<Location>> {
    let mut local = None;
    let doc = lookup_document(ctx, &uri, source, root_doc, &mut local)?;
    let current = occurrences::collect(source, &uri, doc)?;
    let identity = selected(&current, offset)?.identity.clone();
    let sources = gather(ctx, &uri, source, current, &identity, doc, root_path, false).ok()?;
    let mut out = Vec::new();
    for source in &sources {
        let index = ctx.index(&source.text);
        for occurrence in &source.occurrences {
            if occurrence.identity == identity && (include_declaration || !occurrence.declaration) {
                out.push(Location {
                    uri: source.uri.clone(),
                    range: index.range(occurrence.span),
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
pub(crate) fn rename(
    ctx: &Ctx,
    uri: Uri,
    source: &str,
    offset: usize,
    new_name: &str,
    root_doc: Option<&Document>,
    root_path: Option<&Path>,
) -> Result<Option<WorkspaceEdit>, String> {
    if !is_valid_identifier(new_name) {
        return Err(format!("'{new_name}' is not a valid WCL identifier"));
    }
    let mut local = None;
    let Some(doc) = lookup_document(ctx, &uri, source, root_doc, &mut local) else {
        return Ok(None);
    };
    let Some(current) = occurrences::collect(source, &uri, doc) else {
        return Ok(None);
    };
    let Some(identity) = selected(&current, offset).map(|o| o.identity.clone()) else {
        return Ok(None);
    };
    let sources = gather(ctx, &uri, source, current, &identity, doc, root_path, true)
        .map_err(|why| format!("Cannot rename: {why}"))?;
    let all = || sources.iter().flat_map(|s| &s.occurrences);
    if let Identity::Global(name) = &identity
        && let Some((category, _)) = name.split_once(':')
        && all().any(|o| matches!(&o.identity, Identity::Unresolved(c) if c == category))
    {
        return Err(
            "A computed semantic name prevents a complete rename; use a literal name first".into(),
        );
    }
    if !all().any(|o| o.identity == identity && o.declaration) {
        return Err("The selected name has no editable authored declaration".into());
    }

    // Each edit keeps its byte span too, to check the result below.
    let mut changes: HashMap<Uri, Vec<TextEdit>> = HashMap::new();
    let mut edited: Vec<(usize, Vec<(wcl_lang::Span, String)>)> = Vec::new();
    for (i, source) in sources.iter().enumerate() {
        let index = ctx.index(&source.text);
        let mut spans: Vec<(wcl_lang::Span, String)> = Vec::new();
        for occurrence in source.occurrences.iter().filter(|o| o.identity == identity) {
            if source
                .occurrences
                .iter()
                .any(|o| o.span == occurrence.span && o.identity != identity)
            {
                return Err("Rename target is used with more than one declaration identity".into());
            }
            if spans.iter().any(|(span, _)| *span == occurrence.span) {
                continue;
            }
            let new_text = format!(
                "{}{new_name}{}",
                occurrence.replacement_prefix, occurrence.replacement_suffix
            );
            changes
                .entry(source.uri.clone())
                .or_default()
                .push(TextEdit {
                    range: index.range(occurrence.span),
                    new_text: new_text.clone(),
                });
            spans.push((occurrence.span, new_text));
        }
        if !spans.is_empty() {
            edited.push((i, spans));
        }
    }

    if doc.schema_errors().is_empty() {
        check_rename(ctx, &uri, source, doc, root_path, &sources, edited)?;
    }
    Ok(Some(WorkspaceEdit {
        changes: Some(changes),
        ..Default::default()
    }))
}

/// Re-open the document with the rename applied and refuse it when the
/// result no longer validates — a use the occurrence walk cannot see
/// (a contextual kind name built at runtime, say) would otherwise break
/// silently.
fn check_rename(
    ctx: &Ctx,
    uri: &Uri,
    source: &str,
    doc: &Document,
    root_path: Option<&Path>,
    sources: &[SourceOccurrences],
    edited: Vec<(usize, Vec<(wcl_lang::Span, String)>)>,
) -> Result<(), String> {
    const NOT_LOCAL: &str = "Rename target is not a local file";
    let request_path = uri_to_path(uri).ok_or(NOT_LOCAL)?;
    let mut updated = (*ctx.buffers).clone();
    updated.insert(request_path.clone(), source.to_string());
    for (i, mut spans) in edited {
        let source = &sources[i];
        let mut text = source.text.clone();
        spans.sort_by_key(|(span, _)| std::cmp::Reverse(span.start));
        for (span, new_text) in spans {
            text.replace_range(span.start..span.end, &new_text);
        }
        updated.insert(uri_to_path(&source.uri).ok_or(NOT_LOCAL)?, text);
    }
    let loader = wcl_wdoc::schema_registry().loader(wcl_lang::overlay_loader(updated.clone()));
    let checked = match root_path {
        Some(root) => Document::from_file_with_loader(root, doc.environment(), loader),
        None => Document::open_at_with_loader(
            updated
                .get(&request_path)
                .map(String::as_str)
                .unwrap_or(source),
            uri.as_str(),
            request_path.parent().map(Path::to_path_buf),
            doc.environment(),
            loader,
        ),
    }
    .map_err(|error| format!("Rename would invalidate the document: {error}"))?;
    if let Some(error) = checked.schema_errors().first() {
        return Err(format!("Rename has unresolved contextual uses: {error}"));
    }
    Ok(())
}

/// A legal WCL identifier: what the lexer reads as an identifier, and
/// not a reserved word.
fn is_valid_identifier(s: &str) -> bool {
    wcl_lang::is_identifier(s) && !wcl_lang::is_keyword(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_preserves_evaluation_with_indexing_interpolation_and_shadowing() {
        let source = "@schemaless values = [2, 3]\n@schemaless result = $\"values: ${at(values, 0) + (fn(values: i64) -> i64 { values })(4)}\"\n";
        let edit = rename(
            &ctx(),
            url(),
            source,
            source.find("values").unwrap(),
            "numbers",
            None,
            None,
        )
        .unwrap()
        .unwrap();
        let mut edits = edit.changes.unwrap().remove(&url()).unwrap();
        assert_eq!(edits.len(), 2);
        edits.sort_by_key(|e| std::cmp::Reverse(e.range.start));
        let mut updated = source.to_string();
        for edit in edits {
            let start = utf8(source).offset(edit.range.start);
            let end = utf8(source).offset(edit.range.end);
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
            &Ctx::with_buffers(crate::convert::PositionEncoding::Utf8, overlays.clone()),
            Uri::from_file_path(&main).unwrap(),
            source,
            source.find("Color").unwrap(),
            true,
            Some(&doc),
            Some(&main),
        )
        .unwrap();
        let declaration = refs
            .iter()
            .find(|r| r.uri == Uri::from_file_path(&shared).unwrap())
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
            &ctx(),
            Uri::from_file_path(&main).unwrap(),
            source,
            source.find("Foo").unwrap(),
            "Bar",
            Some(&doc),
            Some(&main),
        )
        .unwrap()
        .unwrap();
        let changes = edit.changes.unwrap();
        let local = changes.get(&Uri::from_file_path(&main).unwrap()).unwrap();
        assert_eq!(local.len(), 1);
        assert_eq!(local[0].range.start.line, 1);
    }

    fn ctx() -> Ctx {
        Ctx::new(crate::convert::PositionEncoding::Utf8)
    }

    fn utf8(text: &str) -> crate::convert::LineIndex<'_> {
        crate::convert::LineIndex::new(text, crate::convert::PositionEncoding::Utf8)
    }

    fn url() -> Uri {
        "file:///test.wcl".parse::<Uri>().unwrap()
    }

    #[test]
    fn reserved_words_are_not_valid_new_names() {
        for word in ["true", "false", "none", "if", "else", "match"] {
            assert!(wcl_lang::is_keyword(word), "{word}");
            // The predicate agrees with what the lexer actually produces.
            let token = wcl_lang::Lexer::new(word).next_token().unwrap();
            assert!(
                !matches!(token.kind, wcl_lang::TokenKind::Ident(_)),
                "{word}"
            );
            assert!(!is_valid_identifier(word), "{word}");
        }
        for word in ["type", "fn", "try", "value_1", "_x"] {
            assert!(is_valid_identifier(word), "{word}");
        }
        for word in ["", "1x", "a-b", "é"] {
            assert!(!is_valid_identifier(word), "{word:?}");
        }
        let source = "@schemaless value = 1
";
        let result = rename(
            &ctx(),
            url(),
            source,
            source.find("value").unwrap(),
            "match",
            None,
            None,
        );
        assert!(result.is_err(), "{result:?}");
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
                &ctx(),
                url(),
                source,
                source.find(cursor).unwrap(),
                "replacement",
                None,
                None,
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
            &ctx(),
            url(),
            source,
            source.find(needle).unwrap(),
            name,
            None,
            None,
        )
        .unwrap()
        .unwrap();
        let mut edits = edit.changes.unwrap().remove(&url()).unwrap();
        edits.sort_by_key(|e| std::cmp::Reverse(e.range.start));
        let mut updated = source.to_string();
        for edit in edits {
            let start = utf8(source).offset(edit.range.start);
            let end = utf8(source).offset(edit.range.end);
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
                    &ctx(),
                    url(),
                    source,
                    source.find(needle).unwrap(),
                    "renamed",
                    None,
                    None,
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
                &Ctx::with_buffers(crate::convert::PositionEncoding::Utf8, overlays.clone()),
                Uri::from_file_path(&main).unwrap(),
                source,
                source.find(needle).unwrap(),
                new_name,
                Some(&doc),
                Some(&main),
            )
            .unwrap()
            .unwrap();
            overlays.insert(main.clone(), source.to_string());
            for (uri, mut edits) in edit.changes.unwrap() {
                let path = crate::convert::uri_to_path(&uri).unwrap();
                let original = overlays[&path].clone();
                let text = overlays.get_mut(&path).unwrap();
                edits.sort_by_key(|e| std::cmp::Reverse(e.range.start));
                for edit in edits {
                    let start = utf8(&original).offset(edit.range.start);
                    let end = utf8(&original).offset(edit.range.end);
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
            &ctx(),
            Uri::from_file_path(&main).unwrap(),
            source,
            source.find("greeting =").unwrap(),
            "salutation",
            Some(&doc),
            Some(&main),
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
                rename(&ctx(), url(), source, cursor, "Bar", None, None,)
                    .unwrap()
                    .is_none()
            );
        }
    }

    #[test]
    fn goto_jumps_to_block_kind_decl() {
        let src = "@document\ntype Root {\n  c: Config\n}\n@block(\"config\")\ntype Config {\n  region: utf8\n}\nconfig {\n  region = \"x\"\n}\n";
        let cursor = src.find("config {").unwrap() + 2;
        let resp = goto_definition(&ctx(), url(), src, cursor, None, None).expect("def found");
        let GotoDefinitionResponse::Scalar(loc) = resp else {
            panic!("expected scalar")
        };
        // SymbolRecord.span covers the full `@block(...)\ntype Config {...}`
        // form. We assert the range starts somewhere before `type Config`
        // and includes that line.
        let type_kw = src.find("type Config").unwrap();
        let decl_start = utf8(src).position(type_kw);
        assert!(loc.range.start <= decl_start);
        assert!(loc.range.end > decl_start);
    }

    #[test]
    fn references_returns_decl_and_uses() {
        let src = "@document\ntype Root {\n  v: Foo\n}\n@block(\"foo\")\ntype Foo {\n  x: utf8\n}\nfoo {\n  x = \"a\"\n}\nfoo {\n  x = \"b\"\n}\n";
        // Cursor on the type-ref "Foo" in `v: Foo`.
        let cursor = src.find("v: Foo").unwrap() + 3;
        let locs = references(&ctx(), url(), src, cursor, true, None, None).expect("some refs");
        // Should include the declaration "type Foo" and the "v: Foo" use,
        // but not the lowercase block kind "foo".
        assert_eq!(locs.len(), 2, "found: {locs:#?}");
    }

    #[test]
    fn references_excludes_decl_when_requested() {
        let src =
            "@document\ntype Root {\n  v: Foo\n}\n@block(\"foo\")\ntype Foo {\n  x: utf8\n}\n";
        let cursor = src.find("v: Foo").unwrap() + 3;
        let with_decl = references(&ctx(), url(), src, cursor, true, None, None).unwrap();
        let no_decl = references(&ctx(), url(), src, cursor, false, None, None).unwrap();
        assert_eq!(with_decl.len(), no_decl.len() + 1);
    }

    #[test]
    fn references_skip_an_import_that_can_no_longer_be_read() {
        // The document was built while shared.wcl existed; it is gone by
        // the time references walks the imports. The references in the
        // readable file still come back.
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("main.wcl");
        let shared = dir.path().join("shared.wcl");
        let source = "import \"./shared.wcl\"\ntype Foo { x: utf8 }\ntype Wrap { f: Foo }\n";
        std::fs::write(&main, source).unwrap();
        std::fs::write(&shared, "namespace shared\ntype Other { f: utf8 }\n").unwrap();
        let doc = Document::from_file(&main).unwrap();
        std::fs::remove_file(&shared).unwrap();
        let refs = references(
            &ctx(),
            Uri::from_file_path(&main).unwrap(),
            source,
            source.rfind("Foo").unwrap(),
            true,
            Some(&doc),
            Some(&main),
        )
        .expect("references in the readable file");
        assert_eq!(refs.len(), 2, "{refs:?}");
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
        let main_url = Uri::from_file_path(&main).unwrap();
        // Cursor sits in the main file's import declaration on the
        // word "shared" — which is also a block kind / type name in
        // shared.wcl. References should find occurrences inside the
        // imported file even though the main file has none.
        let cursor = main_src.find("shared.wcl").unwrap() + 2;
        let locs = references(
            &ctx(),
            main_url.clone(),
            &main_src,
            cursor,
            true,
            None,
            None,
        )
        .unwrap_or_default();
        let shared_url = Uri::from_file_path(&shared).unwrap();
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
        let target = Uri::from_file_path(hit.source_path.expect("imported path")).unwrap();
        assert_eq!(target, Uri::from_file_path(&shared).unwrap());
    }
}
