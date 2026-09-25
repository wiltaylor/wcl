//! Parse + schema-validate a document and translate the resulting
//! errors into LSP [`Diagnostic`] values, each placed in the file its
//! span indexes into.
//!
//! A document's errors are not all its own: a violation inside an
//! imported file carries that file's source, and its span only makes
//! sense against that text. Every conversion here therefore pairs a
//! diagnostic with its [`Origin`] and converts the span against the
//! text the error was raised on.

use std::path::{Path, PathBuf};

use miette::{Diagnostic as _, NamedSource, SourceSpan};
use tower_lsp_server::ls_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Range};
use wcl_lang::{Document, EvalError, ParseError, SYSTEM_IMPORT_ROOT, Span};

use crate::ctx::Ctx;

/// The file a diagnostic belongs to.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Origin {
    /// The document that was analysed.
    Analysed,
    /// Another file in its import graph, by path.
    File(PathBuf),
}

/// Open `source` (named `uri`) through [`Ctx::open`] and report every
/// diagnostic it produces, each with the file it belongs to. Empty when
/// the document parses and validates cleanly.
pub(crate) fn analyse(ctx: &Ctx, source: &str, uri: &str) -> Vec<(Origin, Diagnostic)> {
    match ctx.open(source, uri) {
        Ok(doc) => document(ctx, &doc),
        Err(e) => parse_failure(ctx, &e, uri),
    }
}

/// Schema errors and warnings of an opened document. An error whose
/// provenance is known goes to that source; one without (the library
/// does not yet tag every cross-file error) and every warning stay on
/// the analysed document, as the CLI renders them.
pub(crate) fn document(ctx: &Ctx, doc: &Document) -> Vec<(Origin, Diagnostic)> {
    let root = doc.source();
    let mut out = Vec::new();
    for (error, source) in doc.schema_diagnostics() {
        let source = source.as_ref().unwrap_or(root);
        if let Some(origin) = origin_of(source.name(), root.name()) {
            out.push((
                origin,
                eval_error_to_diagnostic(ctx, source, &error, DiagnosticSeverity::ERROR),
            ));
        }
    }
    for warning in doc.schema_warnings() {
        out.push((
            Origin::Analysed,
            eval_error_to_diagnostic(ctx, root, &warning, DiagnosticSeverity::WARNING),
        ));
    }
    out
}

/// A document that failed to open. A syntax error carries the source it
/// was raised on — the analysed document or an imported file — so it is
/// placed there; an I/O failure lands at the top of the analysed file.
pub(crate) fn parse_failure(ctx: &Ctx, err: &ParseError, name: &str) -> Vec<(Origin, Diagnostic)> {
    match err {
        ParseError::Syntax(syntax) => origin_of(syntax.src.name(), name)
            .map(|origin| {
                let range = source_span_to_range(ctx, syntax.src.inner(), syntax.span);
                (
                    origin,
                    syntax_diagnostic(range, &syntax.message, &syntax.label),
                )
            })
            .into_iter()
            .collect(),
        ParseError::Io(io) => vec![(
            Origin::Analysed,
            Diagnostic {
                range: Range::default(),
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String("wcl::io".into())),
                source: Some("wcl".into()),
                message: format!("io error: {io}"),
                ..Default::default()
            },
        )],
    }
}

/// Syntax-only diagnostics for `source`: a parse with no import
/// resolution or schema validation. Used for files outside a configured
/// root's import graph, where validating the fragment in isolation
/// reports false positives for everything the root supplies.
pub(crate) fn syntax_only(ctx: &Ctx, source: &str, name: &str) -> Vec<Diagnostic> {
    match wcl_lang::parse_for_edit(source, name) {
        Ok(_) => Vec::new(),
        Err(ParseError::Syntax(syntax)) => {
            let range = source_span_to_range(ctx, source, syntax.span);
            vec![syntax_diagnostic(range, &syntax.message, &syntax.label)]
        }
        // `parse_for_edit` reads nothing, so it cannot fail on I/O.
        Err(ParseError::Io(_)) => Vec::new(),
    }
}

/// Where a diagnostic raised on the source named `name` belongs, given
/// the analysed document is named `analysed`. Embedded system imports
/// have no file an editor can open, so their diagnostics are dropped.
fn origin_of(name: &str, analysed: &str) -> Option<Origin> {
    if name == analysed {
        return Some(Origin::Analysed);
    }
    let path = Path::new(name);
    if path.starts_with(SYSTEM_IMPORT_ROOT) {
        tracing::warn!("dropping a diagnostic raised inside system import {name}");
        return None;
    }
    Some(if path.is_absolute() {
        Origin::File(path.to_path_buf())
    } else {
        Origin::Analysed
    })
}

/// The LSP form of one parse failure.
fn syntax_diagnostic(range: Range, message: &str, label: &str) -> Diagnostic {
    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(NumberOrString::String("wcl::parse".into())),
        source: Some("wcl".into()),
        message: format!("{message}: {label}"),
        ..Default::default()
    }
}

/// Convert one evaluation or schema error raised on `source` into an
/// LSP diagnostic. The code and span come from the error's
/// `miette::Diagnostic` implementation — its `code()` and first label —
/// the same way the CLI's JSON report reads them.
fn eval_error_to_diagnostic(
    ctx: &Ctx,
    source: &NamedSource<String>,
    err: &EvalError,
    severity: DiagnosticSeverity,
) -> Diagnostic {
    let range = err
        .labels()
        .and_then(|mut labels| labels.next())
        .map(|label| source_span_to_range(ctx, source.inner(), *label.inner()))
        .unwrap_or_default();
    Diagnostic {
        range,
        severity: Some(severity),
        code: err
            .code()
            .map(|code| NumberOrString::String(code.to_string())),
        source: Some("wcl".into()),
        message: err.to_string(),
        data: diagnostic_data(err),
        ..Default::default()
    }
}

/// Structured payload round-tripped to the client (and back, on a code
/// action request) so consumers act on a violation without re-parsing
/// `message`. For schema violations we carry the kind (its variant name)
/// and the offending identifier when one is recorded.
fn diagnostic_data(err: &EvalError) -> Option<serde_json::Value> {
    match err {
        EvalError::SchemaViolation { kind, detail, .. } => Some(serde_json::json!({
            // `{kind:?}` is the variant identifier (e.g. "UnknownField")
            // — both are field-less variants, so Debug is the stable name.
            "kind": format!("{kind:?}"),
            "name": detail,
        })),
        _ => None,
    }
}

/// Convert a byte span in `text` into the line/character range LSP
/// wants, with `character` counted in the negotiated position encoding.
fn source_span_to_range(ctx: &Ctx, text: &str, span: SourceSpan) -> Range {
    let start = span.offset();
    ctx.index(text).range(Span::new(start, start + span.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp_server::ls_types::Position;

    fn ctx() -> Ctx {
        Ctx::new(Default::default())
    }

    /// Diagnostics for a single-file document, asserting none of them
    /// was placed in another file.
    fn compute(src: &str, name: &str) -> Vec<Diagnostic> {
        analyse(&ctx(), src, name)
            .into_iter()
            .map(|(origin, diagnostic)| {
                assert_eq!(origin, Origin::Analysed, "{diagnostic:?}");
                diagnostic
            })
            .collect()
    }

    #[test]
    fn clean_document_has_no_diagnostics() {
        let src = "// no schema, no fields, nothing to validate\n";
        let diags = compute(src, "test.wcl");
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:#?}");
    }

    #[test]
    fn syntax_error_emits_one_diagnostic() {
        // Unclosed brace fixture from examples/errors.
        let src = "@schemaless config {\n  region = \"us-east-1\"\n";
        for diags in [
            compute(src, "test.wcl"),
            syntax_only(&ctx(), src, "test.wcl"),
        ] {
            assert_eq!(diags.len(), 1, "expected one syntax diagnostic");
            let d = &diags[0];
            assert_eq!(d.severity, Some(DiagnosticSeverity::ERROR));
            assert_eq!(d.code, Some(NumberOrString::String("wcl::parse".into())));
            assert!(d.range.start.line <= d.range.end.line);
        }
    }

    #[test]
    fn system_import_resolves_and_syntax_only_skips_it() {
        // `import <wdoc.wcl>` must resolve through the registry loader —
        // a bare disk loader turns it into a bogus parse error (the bug
        // this loader threading fixed). The syntax-only check never
        // resolves imports at all.
        let src = "import <wdoc.wcl>\n\npage index {\n  title = \"Hi\"\n\n  h1 \"Hi\"\n}\n";
        let diags = compute(src, "test.wcl");
        assert!(diags.is_empty(), "root path flagged: {diags:#?}");
        let diags = syntax_only(&ctx(), src, "test.wcl");
        assert!(diags.is_empty(), "syntax-only path flagged: {diags:#?}");
    }

    #[test]
    fn syntax_only_does_not_resolve_imports() {
        let src = "import \"./does-not-exist.wcl\"\n";
        assert!(!compute(src, "test.wcl").is_empty());
        assert!(syntax_only(&ctx(), src, "test.wcl").is_empty());
    }

    #[test]
    fn relative_import_resolves_against_base_dir() {
        // A cross-file workspace: main.wcl imports pages.wcl by relative
        // path; both use the system import. Diagnostics for either file
        // must resolve the quoted import against the file's directory.
        let td = tempfile::tempdir().unwrap();
        std::fs::write(
            td.path().join("pages.wcl"),
            "import <wdoc.wcl>\n\npage about {\n  title = \"About\"\n\n  h1 \"About\"\n}\n",
        )
        .unwrap();
        let main_src = "import <wdoc.wcl>\nimport \"pages.wcl\"\n\npage index {\n  title = \"Hi\"\n\n  h1 \"Hi\"\n}\n";
        let main = crate::convert::path_to_uri(&td.path().join("main.wcl")).unwrap();
        let diags = compute(main_src, main.as_str());
        assert!(diags.is_empty(), "rooted main flagged: {diags:#?}");
    }

    #[test]
    fn imported_file_errors_are_placed_in_that_file() {
        // An undeclared decorator inside an imported file carries the
        // imported source: it is reported there, at its own line.
        let td = tempfile::tempdir().unwrap();
        let shared = td.path().join("shared.wcl");
        std::fs::write(&shared, "\n\n@missing\ntitle = \"Hi\"\n").unwrap();
        let main_src = "import \"./shared.wcl\"\n@document type Root { title: utf8 }\n";
        let main = crate::convert::path_to_uri(&td.path().join("main.wcl")).unwrap();
        let diags = analyse(&ctx(), main_src, main.as_str());
        let (origin, diagnostic) = diags
            .iter()
            .find(|(_, d)| d.message.contains("decorator 'missing'"))
            .expect("undeclared decorator diagnostic");
        let Origin::File(path) = origin else {
            panic!("placed in the analysed file: {diags:#?}")
        };
        assert_eq!(
            std::fs::canonicalize(path).unwrap(),
            std::fs::canonicalize(&shared).unwrap()
        );
        assert_eq!(diagnostic.range.start, Position::new(2, 1));
    }

    #[test]
    fn imported_file_syntax_errors_are_placed_in_that_file() {
        let td = tempfile::tempdir().unwrap();
        let shared = td.path().join("shared.wcl");
        std::fs::write(&shared, "\n\n\n@schemaless x = {\n").unwrap();
        let main_src = "import \"./shared.wcl\"\n";
        let main = crate::convert::path_to_uri(&td.path().join("main.wcl")).unwrap();
        let diags = analyse(&ctx(), main_src, main.as_str());
        assert_eq!(diags.len(), 1, "{diags:#?}");
        let (origin, diagnostic) = &diags[0];
        assert!(matches!(origin, Origin::File(_)), "{diags:#?}");
        assert!(diagnostic.range.start.line >= 3, "{diagnostic:#?}");
    }

    #[test]
    fn gather_shadow_surfaces_as_warning_severity() {
        // A root @document gather field shadowing the wdoc stdlib's
        // `pages` gather — advisory, so WARNING, not ERROR.
        let src = "import <wdoc.wcl>\n\n@block(\"part\")\ntype Part {\n  name: utf8\n}\n@document\ntype Mine {\n  @children(\"part\") pages: list<Part>\n}\n";
        let diags = compute(src, "test.wcl");
        let warn = diags
            .iter()
            .find(|d| d.severity == Some(DiagnosticSeverity::WARNING))
            .expect("shadow warning present");
        assert!(
            warn.message.contains("pages"),
            "warning names the field: {}",
            warn.message
        );
        assert!(
            !diags
                .iter()
                .any(|d| d.severity == Some(DiagnosticSeverity::ERROR)),
            "no errors expected: {diags:#?}"
        );
    }

    #[test]
    fn schema_violation_reports_at_field_span() {
        // Mirror examples/errors/unknown_field.wcl.
        let src = "@document\ntype Root {\n  region: utf8\n}\n@block(\"service\")\ntype Service {\n  region: utf8\n}\nservice web {\n  region = \"us-east-1\"\n  unexpected = \"boom\"\n}\n";
        let diags = compute(src, "test.wcl");
        assert!(!diags.is_empty(), "expected at least one schema diagnostic");
        let has_unknown = diags.iter().any(|d| {
            matches!(&d.code, Some(NumberOrString::String(c)) if c == "wcl::eval::schema_violation")
        });
        assert!(
            has_unknown,
            "expected schema_violation code; got {diags:#?}"
        );
        for d in &diags {
            assert!(d.range.start <= d.range.end);
            assert!(d.range.start != Position::default() || d.range.end != Position::default());
        }
    }

    #[test]
    fn undeclared_decorator_reports_at_its_name() {
        let src = "@document type Root { title: utf8 }\n@missing\ntitle = \"Hello\"\n";
        let diags = compute(src, "test.wcl");
        let diagnostic = diags
            .iter()
            .find(|diagnostic| diagnostic.message.contains("decorator 'missing'"))
            .expect("undeclared decorator diagnostic");

        assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(diagnostic.range.start, Position::new(1, 1));
        assert_eq!(diagnostic.range.end, Position::new(1, 8));
        assert_eq!(
            diagnostic.data,
            Some(serde_json::json!({
                "kind": "UndeclaredDecorator",
                "name": "missing",
            }))
        );
    }
}
