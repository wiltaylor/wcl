use tower_lsp::lsp_types::{
    self as lsp, DiagnosticRelatedInformation, DiagnosticSeverity, Location, NumberOrString,
};
use ropey::Rope;
use wcl_core::diagnostic::{Diagnostic, Severity};
use wcl_core::span::Span;

use crate::convert::span_to_lsp_range;

/// Convert a WCL Diagnostic into an LSP Diagnostic.
/// Returns None for diagnostics with dummy spans (synthetic/test spans).
pub fn to_lsp_diagnostic(
    diag: &Diagnostic,
    rope: &Rope,
    uri: &tower_lsp::lsp_types::Url,
) -> Option<lsp::Diagnostic> {
    // Skip diagnostics with dummy spans
    if is_dummy_span(diag.span) {
        return None;
    }

    let range = span_to_lsp_range(diag.span, rope);
    let severity = Some(match diag.severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Info => DiagnosticSeverity::INFORMATION,
        Severity::Hint => DiagnosticSeverity::HINT,
    });

    let code = diag
        .code
        .as_ref()
        .map(|c| NumberOrString::String(c.clone()));

    let related_information = if diag.labels.is_empty() {
        None
    } else {
        let infos: Vec<DiagnosticRelatedInformation> = diag
            .labels
            .iter()
            .filter(|label| !is_dummy_span(label.span))
            .map(|label| DiagnosticRelatedInformation {
                location: Location {
                    uri: uri.clone(),
                    range: span_to_lsp_range(label.span, rope),
                },
                message: label.message.clone(),
            })
            .collect();
        if infos.is_empty() {
            None
        } else {
            Some(infos)
        }
    };

    Some(lsp::Diagnostic {
        range,
        severity,
        code,
        code_description: None,
        source: Some("wcl".to_string()),
        message: diag.message.clone(),
        related_information,
        tags: None,
        data: None,
    })
}

/// Detect the synthetic dummy span produced by `Span::dummy()`.
/// A real span at the start of a file will have `end > 0`, so it will not
/// match this check.
fn is_dummy_span(span: Span) -> bool {
    span == Span::dummy()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Url;
    use wcl_core::span::{FileId, Span};

    #[test]
    fn test_error_severity() {
        let rope = Rope::from_str("hello world");
        let uri = Url::parse("file:///test.wcl").unwrap();
        let diag = Diagnostic::error("bad", Span::new(FileId(0), 0, 5));
        let lsp = to_lsp_diagnostic(&diag, &rope, &uri).unwrap();
        assert_eq!(lsp.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(lsp.message, "bad");
        assert_eq!(lsp.source, Some("wcl".to_string()));
    }

    #[test]
    fn test_warning_severity() {
        let rope = Rope::from_str("hello");
        let uri = Url::parse("file:///test.wcl").unwrap();
        let diag = Diagnostic::warning("warn", Span::new(FileId(0), 0, 3));
        let lsp = to_lsp_diagnostic(&diag, &rope, &uri).unwrap();
        assert_eq!(lsp.severity, Some(DiagnosticSeverity::WARNING));
    }

    #[test]
    fn test_with_code() {
        let rope = Rope::from_str("hello");
        let uri = Url::parse("file:///test.wcl").unwrap();
        let diag = Diagnostic::error("dup", Span::new(FileId(0), 0, 5)).with_code("E030");
        let lsp = to_lsp_diagnostic(&diag, &rope, &uri).unwrap();
        assert_eq!(lsp.code, Some(NumberOrString::String("E030".to_string())));
    }

    #[test]
    fn test_dummy_span_skipped() {
        let rope = Rope::from_str("hello");
        let uri = Url::parse("file:///test.wcl").unwrap();
        let diag = Diagnostic::error("skip", Span::dummy());
        assert!(to_lsp_diagnostic(&diag, &rope, &uri).is_none());
    }

    #[test]
    fn test_span_at_file_start_not_skipped() {
        let rope = Rope::from_str("hello world");
        let uri = Url::parse("file:///test.wcl").unwrap();
        // A real diagnostic at the very start of the file (0..5) must not be skipped.
        let diag = Diagnostic::error("start", Span::new(FileId(0), 0, 5));
        let lsp = to_lsp_diagnostic(&diag, &rope, &uri);
        assert!(lsp.is_some(), "diagnostic at file start should not be skipped");
        assert_eq!(lsp.unwrap().message, "start");
    }

    #[test]
    fn test_with_labels() {
        let rope = Rope::from_str("hello world foo");
        let uri = Url::parse("file:///test.wcl").unwrap();
        let diag = Diagnostic::error("main", Span::new(FileId(0), 0, 5))
            .with_label(Span::new(FileId(0), 6, 11), "related");
        let lsp = to_lsp_diagnostic(&diag, &rope, &uri).unwrap();
        let related = lsp.related_information.unwrap();
        assert_eq!(related.len(), 1);
        assert_eq!(related[0].message, "related");
    }
}
