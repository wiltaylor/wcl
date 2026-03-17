use crate::span::Span;

/// Severity level of a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

/// An annotated source span with an explanatory message.
#[derive(Debug, Clone)]
pub struct Label {
    pub span: Span,
    pub message: String,
}

/// A single diagnostic (error, warning, info, or hint) emitted during compilation.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: Span,
    /// Additional annotated spans (e.g. "first defined here").
    pub labels: Vec<Label>,
    /// Free-form notes appended after the main message.
    pub notes: Vec<String>,
    /// Optional machine-readable error code, e.g. `"E0042"`.
    pub code: Option<String>,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>, span: Span) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
            span,
            labels: Vec::new(),
            notes: Vec::new(),
            code: None,
        }
    }

    pub fn warning(message: impl Into<String>, span: Span) -> Self {
        Self {
            severity: Severity::Warning,
            message: message.into(),
            span,
            labels: Vec::new(),
            notes: Vec::new(),
            code: None,
        }
    }

    /// Builder: attach a machine-readable error code.
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    /// Builder: attach an additional labelled span.
    pub fn with_label(mut self, span: Span, message: impl Into<String>) -> Self {
        self.labels.push(Label {
            span,
            message: message.into(),
        });
        self
    }

    /// Builder: append a free-form note.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

/// Accumulates diagnostics across compilation phases.
///
/// Each phase appends to the bag rather than failing immediately, allowing all
/// errors to be reported together.
#[derive(Debug, Default)]
pub struct DiagnosticBag {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticBag {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an already-constructed [`Diagnostic`].
    pub fn add(&mut self, diag: Diagnostic) {
        self.diagnostics.push(diag);
    }

    /// Convenience: create and append an error diagnostic.
    pub fn error(&mut self, message: impl Into<String>, span: Span) {
        self.add(Diagnostic::error(message, span));
    }

    /// Convenience: create and append a warning diagnostic.
    pub fn warning(&mut self, message: impl Into<String>, span: Span) {
        self.add(Diagnostic::warning(message, span));
    }

    /// Convenience: create and append an error with a machine-readable code.
    pub fn error_with_code(&mut self, message: impl Into<String>, span: Span, code: &str) {
        self.add(Diagnostic::error(message, span).with_code(code));
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.is_error())
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    /// Move all diagnostics from `other` into `self`.
    pub fn merge(&mut self, other: DiagnosticBag) {
        self.diagnostics.extend(other.diagnostics);
    }

    pub fn error_count(&self) -> usize {
        self.diagnostics.iter().filter(|d| d.is_error()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::{FileId, Span};

    fn dummy_span() -> Span {
        Span::new(FileId(0), 0, 1)
    }

    #[test]
    fn new_bag_has_no_errors() {
        let bag = DiagnosticBag::new();
        assert!(!bag.has_errors());
        assert_eq!(bag.error_count(), 0);
        assert!(bag.diagnostics().is_empty());
    }

    #[test]
    fn add_error_detected() {
        let mut bag = DiagnosticBag::new();
        bag.error("something went wrong", dummy_span());
        assert!(bag.has_errors());
        assert_eq!(bag.error_count(), 1);
    }

    #[test]
    fn warning_does_not_count_as_error() {
        let mut bag = DiagnosticBag::new();
        bag.warning("mild concern", dummy_span());
        assert!(!bag.has_errors());
        assert_eq!(bag.error_count(), 0);
        assert_eq!(bag.diagnostics().len(), 1);
    }

    #[test]
    fn error_with_code_sets_code() {
        let mut bag = DiagnosticBag::new();
        bag.error_with_code("duplicate id", dummy_span(), "E030");
        let diag = &bag.diagnostics()[0];
        assert_eq!(diag.code.as_deref(), Some("E030"));
        assert!(diag.is_error());
    }

    #[test]
    fn merge_combines_diagnostics() {
        let mut a = DiagnosticBag::new();
        a.error("err a", dummy_span());

        let mut b = DiagnosticBag::new();
        b.warning("warn b", dummy_span());
        b.error("err b", dummy_span());

        a.merge(b);
        assert_eq!(a.error_count(), 2);
        assert_eq!(a.diagnostics().len(), 3);
    }

    #[test]
    fn into_diagnostics_consumes_bag() {
        let mut bag = DiagnosticBag::new();
        bag.error("e1", dummy_span());
        bag.error("e2", dummy_span());
        let diags = bag.into_diagnostics();
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn builder_methods_chain() {
        let span = dummy_span();
        let d = Diagnostic::error("type mismatch", span)
            .with_code("E050")
            .with_label(span, "expected int here")
            .with_note("consider casting the value");

        assert_eq!(d.code.as_deref(), Some("E050"));
        assert_eq!(d.labels.len(), 1);
        assert_eq!(d.notes.len(), 1);
        assert!(d.is_error());
    }

    #[test]
    fn severity_variants() {
        assert_ne!(Severity::Error, Severity::Warning);
        assert_ne!(Severity::Info, Severity::Hint);
    }
}
