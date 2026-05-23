//! `textDocument/codeAction` handler. Inspects the diagnostics the
//! client hands us back, recognises a few schema-violation patterns
//! by message text, and emits `WorkspaceEdit`-bearing code actions.
//!
//! Pattern matching against the message string is a known shortcut —
//! it keeps the v1 surface small. A future slice can lift structured
//! metadata (field name, parent block) onto `SchemaViolation` and
//! drop the parsing.

use std::collections::HashMap;

use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionResponse, Diagnostic, NumberOrString,
    Position, Range, TextEdit, Url, WorkspaceEdit,
};

/// Build code actions for every diagnostic the client knows about.
/// Returns `None` when nothing fits — clients treat that the same as
/// an empty list.
pub(crate) fn compute(
    uri: &Url,
    source: &str,
    diagnostics: &[Diagnostic],
) -> Option<CodeActionResponse> {
    let mut actions: Vec<CodeActionOrCommand> = Vec::new();
    for diag in diagnostics {
        if !is_wcl_schema(diag) {
            continue;
        }
        if let Some(action) = unknown_field_fix(uri, source, diag) {
            actions.push(CodeActionOrCommand::CodeAction(action));
        } else if let Some(action) = disallowed_child_fix(uri, source, diag) {
            actions.push(CodeActionOrCommand::CodeAction(action));
        }
    }
    if actions.is_empty() { None } else { Some(actions) }
}

fn is_wcl_schema(diag: &Diagnostic) -> bool {
    matches!(
        (&diag.source, &diag.code),
        (Some(s), Some(NumberOrString::String(c)))
            if s == "wcl" && c == "wcl::eval::schema_violation"
    )
}

/// "unknown field 'foo'" — delete the entire offending line(s).
fn unknown_field_fix(uri: &Url, source: &str, diag: &Diagnostic) -> Option<CodeAction> {
    let name = extract_quoted(&diag.message, "unknown field")?;
    let range = expand_to_full_lines(source, diag.range);
    Some(make_action(
        format!("Remove unknown field `{name}`"),
        uri,
        range,
        String::new(),
        diag.clone(),
    ))
}

/// "disallowed child" — delete the offending line(s) (the diagnostic
/// span covers the block's leading kind; we widen to line for a
/// pragmatic fix).
fn disallowed_child_fix(uri: &Url, source: &str, diag: &Diagnostic) -> Option<CodeAction> {
    if !diag.message.contains("disallowed child") {
        return None;
    }
    let label = extract_quoted(&diag.message, "disallowed child")
        .unwrap_or_else(|| "block".to_string());
    let range = expand_to_full_lines(source, diag.range);
    Some(make_action(
        format!("Remove disallowed `{label}` block"),
        uri,
        range,
        String::new(),
        diag.clone(),
    ))
}

/// Find the first single-quoted name in `msg` that appears after
/// `marker`. Returns `None` when the marker isn't present or the
/// message lacks a quoted token.
fn extract_quoted(msg: &str, marker: &str) -> Option<String> {
    let after = msg.split_once(marker).map(|(_, rest)| rest)?;
    let start = after.find('\'')?;
    let rest = &after[start + 1..];
    let end = rest.find('\'')?;
    Some(rest[..end].to_string())
}

/// Expand a diagnostic range to cover every full line it touches,
/// including the trailing newline of the last line. This lets a
/// "remove" edit drop the offending field/block cleanly.
fn expand_to_full_lines(source: &str, range: Range) -> Range {
    let bytes = source.as_bytes();
    // Walk to the start of the line containing `range.start`.
    let mut start_offset = 0;
    let mut line: u32 = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if line == range.start.line {
            start_offset = i;
            break;
        }
        if b == b'\n' {
            line += 1;
        }
    }
    // Walk past the newline that ends `range.end`'s line.
    let mut end_line: u32 = 0;
    let mut end_offset = bytes.len();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' {
            if end_line == range.end.line {
                end_offset = i + 1;
                break;
            }
            end_line += 1;
        }
    }
    Range {
        start: Position {
            line: line_for_offset(source, start_offset),
            character: 0,
        },
        end: Position {
            line: line_for_offset(source, end_offset),
            character: 0,
        },
    }
}

fn line_for_offset(source: &str, offset: usize) -> u32 {
    source.as_bytes()[..offset.min(source.len())]
        .iter()
        .filter(|&&b| b == b'\n')
        .count() as u32
}

fn make_action(
    title: String,
    uri: &Url,
    range: Range,
    new_text: String,
    diag: Diagnostic,
) -> CodeAction {
    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    changes.insert(uri.clone(), vec![TextEdit { range, new_text }]);
    CodeAction {
        title,
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diag]),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::DiagnosticSeverity;

    fn diag_at(message: &str, line: u32) -> Diagnostic {
        Diagnostic {
            range: Range {
                start: Position { line, character: 0 },
                end: Position {
                    line,
                    character: 10,
                },
            },
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(NumberOrString::String("wcl::eval::schema_violation".into())),
            source: Some("wcl".into()),
            message: message.into(),
            ..Default::default()
        }
    }

    #[test]
    fn extract_quoted_pulls_name_from_message() {
        assert_eq!(
            extract_quoted("unknown field 'foo' in block 'bar'", "unknown field"),
            Some("foo".to_string())
        );
        assert_eq!(
            extract_quoted("disallowed child 'baz' under 'svc'", "disallowed child"),
            Some("baz".to_string())
        );
        assert_eq!(extract_quoted("no quotes here", "unknown field"), None);
    }

    #[test]
    fn unknown_field_emits_quickfix_with_line_delete() {
        let src = "name = \"alpha\"\nunexpected = \"boom\"\nport = 8080\n";
        let uri = Url::parse("file:///t.wcl").unwrap();
        let diag = diag_at("unknown field 'unexpected'", 1);
        let resp = compute(&uri, src, &[diag]).expect("some actions");
        assert_eq!(resp.len(), 1);
        let CodeActionOrCommand::CodeAction(action) = &resp[0] else {
            panic!("expected action")
        };
        assert!(action.title.contains("Remove unknown field"));
        let edit = action.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = changes.get(&uri).unwrap();
        assert_eq!(edits.len(), 1);
        // The edit replaces the offending line with empty text.
        assert!(edits[0].new_text.is_empty());
        // Range covers the second line only.
        assert_eq!(edits[0].range.start.line, 1);
        assert_eq!(edits[0].range.end.line, 2);
    }

    #[test]
    fn unrelated_diagnostic_returns_none() {
        let uri = Url::parse("file:///t.wcl").unwrap();
        let mut diag = diag_at("something else", 0);
        diag.source = Some("other".into());
        assert!(compute(&uri, "", &[diag]).is_none());
    }
}
