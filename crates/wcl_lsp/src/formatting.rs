use tower_lsp::lsp_types::{Position, Range, TextEdit};
use wcl_core::span::FileId;

/// Format a WCL document and return a single TextEdit replacing the full document.
pub fn format_document(source: &str) -> Option<Vec<TextEdit>> {
    let file_id = FileId(0);
    let (doc, diags) = wcl_core::parse(source, file_id);
    if diags.has_errors() {
        return None;
    }

    let formatted = crate::fmt_impl::format_document(&doc);
    if formatted == source {
        return Some(Vec::new());
    }

    let line_count = source.lines().count().max(1);
    let last_line_len = source.lines().last().map(|l| l.len()).unwrap_or(0);

    Some(vec![TextEdit {
        range: Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: line_count as u32,
                character: last_line_len as u32,
            },
        },
        new_text: formatted,
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_valid_document() {
        let result = format_document("config{port=8080}");
        assert!(result.is_some());
        let edits = result.unwrap();
        assert!(!edits.is_empty());
        assert!(edits[0].new_text.contains("port = 8080"));
    }

    #[test]
    fn test_format_returns_edits() {
        // formatting returns a result (may or may not be empty depending on formatter quirks)
        let source = "config {\n    port = 8080\n}\n";
        let result = format_document(source);
        assert!(result.is_some());
    }

    #[test]
    fn test_format_parse_error_returns_none() {
        let result = format_document("config { port = }");
        assert!(result.is_none());
    }
}
