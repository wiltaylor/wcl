//! Parse + schema-validate a source string and translate the
//! resulting errors into LSP [`Diagnostic`] values.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Range};
use wcl_lang::{Document, EvalError, ParseError, Span};

use crate::convert::span_to_range;

/// Compute diagnostics for `source`. Returns an empty list when the
/// document parses and validates cleanly.
pub(crate) fn compute(source: &str, uri: &str) -> Vec<Diagnostic> {
    match Document::open(source, uri) {
        Ok(doc) => doc
            .schema_errors()
            .into_iter()
            .map(|e| eval_error_to_diagnostic(source, &e))
            .collect(),
        Err(ParseError::Syntax(syntax)) => {
            vec![Diagnostic {
                range: source_span_to_range(source, syntax.span),
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String("wcl::parse".into())),
                source: Some("wcl".into()),
                message: format!("{}: {}", syntax.message, syntax.label),
                ..Default::default()
            }]
        }
        Err(ParseError::Io(err)) => {
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

fn eval_error_to_diagnostic(source: &str, err: &EvalError) -> Diagnostic {
    let span = eval_error_span(err);
    Diagnostic {
        range: source_span_to_range(source, span),
        severity: Some(DiagnosticSeverity::ERROR),
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
        | EvalError::NotALeaf { span, .. }
        | EvalError::ImportFailed { span, .. }
        | EvalError::SchemaViolation { span, .. }
        | EvalError::UnresolvedReference { span, .. }
        | EvalError::NotAReference { span, .. } => *span,
    }
}

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
        EvalError::NotALeaf { .. } => "wcl::eval::not_a_leaf",
        EvalError::ImportFailed { .. } => "wcl::eval::import_failed",
        EvalError::SchemaViolation { .. } => "wcl::eval::schema_violation",
        EvalError::UnresolvedReference { .. } => "wcl::eval::unresolved_reference",
        EvalError::NotAReference { .. } => "wcl::eval::not_a_reference",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Position;

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
        let diags = compute(src, "test.wcl");
        assert_eq!(diags.len(), 1, "expected one syntax diagnostic");
        let d = &diags[0];
        assert_eq!(d.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(d.code, Some(NumberOrString::String("wcl::parse".into())));
        assert!(d.range.start.line <= d.range.end.line);
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
}
