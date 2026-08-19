//! Parse + schema-validate a source string and translate the
//! resulting errors into LSP [`Diagnostic`] values.

use std::path::Path;

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Range};
use wcl_lang::{Document, EvalError, FileLoader, ParseError, Span};

use crate::convert::span_to_range;

/// Open `source` the way the wdoc build would: system imports
/// (`import <wdoc.wcl>`) resolve through the caller's loader (the embedded
/// registry over an overlay of open buffers), relative imports resolve
/// against `base_dir`, and the wdoc [`Environment`](wcl_lang::Environment)
/// supplies builtins like `page_metadata`. A bare `Document::open` would
/// flag all three as errors in perfectly valid documents.
fn open_document(
    source: &str,
    uri: &str,
    base_dir: Option<&Path>,
    loader: FileLoader,
) -> Result<Document, ParseError> {
    Document::open_at_with_loader(
        source,
        uri,
        base_dir.map(Path::to_path_buf),
        &wcl_wdoc::wdoc_environment(),
        loader,
    )
}

/// Compute diagnostics for `source`. Returns an empty list when the
/// document parses and validates cleanly.
pub(crate) fn compute(
    source: &str,
    uri: &str,
    base_dir: Option<&Path>,
    loader: FileLoader,
) -> Vec<Diagnostic> {
    match open_document(source, uri, base_dir, loader) {
        Ok(doc) => doc
            .schema_errors()
            .into_iter()
            .map(|e| eval_error_to_diagnostic(source, &e, DiagnosticSeverity::ERROR))
            .chain(
                doc.schema_warnings()
                    .into_iter()
                    .map(|w| eval_error_to_diagnostic(source, &w, DiagnosticSeverity::WARNING)),
            )
            .collect(),
        Err(e) => parse_error_to_diagnostics(source, e),
    }
}

/// Syntax-only diagnostics for `source` — used for non-root files in a
/// rooted workspace, where schema-validating the fragment in isolation
/// reports false positives for everything the root document supplies.
pub(crate) fn compute_syntax_only(
    source: &str,
    uri: &str,
    base_dir: Option<&Path>,
    loader: FileLoader,
) -> Vec<Diagnostic> {
    match open_document(source, uri, base_dir, loader) {
        Ok(_) => Vec::new(),
        Err(e) => parse_error_to_diagnostics(source, e),
    }
}

/// Convert a parse failure into LSP diagnostics, including the
/// secondary label when the error carries one.
fn parse_error_to_diagnostics(source: &str, err: ParseError) -> Vec<Diagnostic> {
    match err {
        ParseError::Syntax(syntax) => {
            vec![Diagnostic {
                range: source_span_to_range(source, syntax.span),
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String("wcl::parse".into())),
                source: Some("wcl".into()),
                message: format!("{}: {}", syntax.message, syntax.label),
                ..Default::default()
            }]
        }
        ParseError::Io(err) => {
            vec![Diagnostic {
                range: Range::default(),
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String("wcl::io".into())),
                source: Some("wcl".into()),
                message: format!("io error: {err}"),
                ..Default::default()
            }]
        }
    }
}

/// Convert one evaluation or schema error into an LSP diagnostic.
fn eval_error_to_diagnostic(
    source: &str,
    err: &EvalError,
    severity: DiagnosticSeverity,
) -> Diagnostic {
    let span = eval_error_span(err);
    Diagnostic {
        range: source_span_to_range(source, span),
        severity: Some(severity),
        code: Some(NumberOrString::String(diagnostic_code(err).into())),
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

/// Convert a byte span into the line/character range LSP wants,
/// counting UTF-16 code units as the protocol requires.
fn source_span_to_range(source: &str, span: miette::SourceSpan) -> Range {
    let start = span.offset();
    let end = start + span.len();
    span_to_range(source, Span::new(start, end))
}

/// Pull the `SourceSpan` out of an `EvalError` variant. Every variant
/// carries a `span` field; we mirror them here rather than reflecting
/// via `miette::Diagnostic::labels` to avoid the dyn iterator detour
/// and keep this exhaustive at compile time.
fn eval_error_span(err: &EvalError) -> miette::SourceSpan {
    match err {
        EvalError::Cycle { span, .. }
        | EvalError::UnknownBuiltin { span, .. }
        | EvalError::BuiltinArity { span, .. }
        | EvalError::BuiltinTypeMismatch { span, .. }
        | EvalError::NonCallable { span }
        | EvalError::CallArity { span, .. }
        | EvalError::CallDepthExceeded { span, .. }
        | EvalError::MatchNoArm { span }
        | EvalError::GuardNotBool { span, .. }
        | EvalError::UnknownUnion { span, .. }
        | EvalError::UnknownVariant { span, .. }
        | EvalError::VariantShapeMismatch { span, .. }
        | EvalError::UserError { span, .. }
        | EvalError::UnionCycle { span, .. }
        | EvalError::TypeMismatch { span, .. }
        | EvalError::Arithmetic { span, .. }
        | EvalError::NotALeaf { span, .. }
        | EvalError::ImportFailed { span, .. }
        | EvalError::SchemaViolation { span, .. }
        | EvalError::UnresolvedReference { span, .. }
        | EvalError::NotAReference { span, .. }
        | EvalError::UnitNoMatch { span, .. }
        | EvalError::UnitWithoutType { span, .. }
        | EvalError::MissingExpander { span, .. } => *span,
    }
}

/// The stable `wcl::…` code for an error, so a client can filter or
/// map diagnostics without matching on message text.
fn diagnostic_code(err: &EvalError) -> &'static str {
    match err {
        EvalError::Cycle { .. } => "wcl::eval::cycle",
        EvalError::UnknownBuiltin { .. } => "wcl::eval::unknown_builtin",
        EvalError::BuiltinArity { .. } => "wcl::eval::builtin_arity",
        EvalError::BuiltinTypeMismatch { .. } => "wcl::eval::builtin_type",
        EvalError::NonCallable { .. } => "wcl::eval::non_callable",
        EvalError::CallArity { .. } => "wcl::eval::call_arity",
        EvalError::CallDepthExceeded { .. } => "wcl::eval::call_depth_exceeded",
        EvalError::MatchNoArm { .. } => "wcl::eval::match_no_arm",
        EvalError::GuardNotBool { .. } => "wcl::eval::guard_not_bool",
        EvalError::UnknownUnion { .. } => "wcl::eval::unknown_union",
        EvalError::UnknownVariant { .. } => "wcl::eval::unknown_variant",
        EvalError::VariantShapeMismatch { .. } => "wcl::eval::variant_shape_mismatch",
        EvalError::UserError { .. } => "wcl::eval::user_error",
        EvalError::UnionCycle { .. } => "wcl::eval::union_cycle",
        EvalError::TypeMismatch { .. } => "wcl::eval::type_mismatch",
        EvalError::Arithmetic { .. } => "wcl::eval::arithmetic",
        EvalError::NotALeaf { .. } => "wcl::eval::not_a_leaf",
        EvalError::ImportFailed { .. } => "wcl::eval::import_failed",
        EvalError::SchemaViolation { .. } => "wcl::eval::schema_violation",
        EvalError::UnresolvedReference { .. } => "wcl::eval::unresolved_reference",
        EvalError::NotAReference { .. } => "wcl::eval::not_a_reference",
        EvalError::UnitNoMatch { .. } => "wcl::eval::unit_no_match",
        EvalError::UnitWithoutType { .. } => "wcl::eval::unit_without_type",
        EvalError::MissingExpander { .. } => "wcl::eval::missing_expander",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Position;

    /// The loader the live server threads in: the embedded wdoc registry
    /// over disk (no open-buffer overlay in unit tests).
    fn loader() -> FileLoader {
        wcl_wdoc::schema_registry().loader(wcl_lang::disk_loader())
    }

    #[test]
    fn clean_document_has_no_diagnostics() {
        let src = "// no schema, no fields, nothing to validate\n";
        let diags = compute(src, "test.wcl", None, loader());
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:#?}");
    }

    #[test]
    fn syntax_error_emits_one_diagnostic() {
        // Unclosed brace fixture from examples/errors.
        let src = "@schemaless config {\n  region = \"us-east-1\"\n";
        let diags = compute(src, "test.wcl", None, loader());
        assert_eq!(diags.len(), 1, "expected one syntax diagnostic");
        let d = &diags[0];
        assert_eq!(d.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(d.code, Some(NumberOrString::String("wcl::parse".into())));
        assert!(d.range.start.line <= d.range.end.line);
    }

    #[test]
    fn system_import_resolves_in_both_paths() {
        // `import <wdoc.wcl>` must resolve through the registry loader —
        // a bare disk loader turns it into a bogus parse error (the bug
        // this loader threading fixed).
        let src = "import <wdoc.wcl>\n\npage index {\n  title = \"Hi\"\n\n  h1 \"Hi\"\n}\n";
        let diags = compute(src, "test.wcl", None, loader());
        assert!(diags.is_empty(), "root path flagged: {diags:#?}");
        let diags = compute_syntax_only(src, "test.wcl", None, loader());
        assert!(diags.is_empty(), "syntax-only path flagged: {diags:#?}");
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
        let diags = compute(main_src, "main.wcl", Some(td.path()), loader());
        assert!(diags.is_empty(), "rooted main flagged: {diags:#?}");
    }

    #[test]
    fn gather_shadow_surfaces_as_warning_severity() {
        // A root @document gather field shadowing the wdoc stdlib's
        // `pages` gather — advisory, so WARNING, not ERROR.
        let src = "import <wdoc.wcl>\n\n@block(\"part\")\ntype Part {\n  name: utf8\n}\n@document\ntype Mine {\n  @children(\"part\") pages: list<Part>\n}\n";
        let diags = compute(src, "test.wcl", None, loader());
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
        let diags = compute(src, "test.wcl", None, loader());
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
        let diags = compute(src, "test.wcl", None, loader());
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
