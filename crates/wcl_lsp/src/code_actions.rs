//! `textDocument/codeAction` handler. Inspects the diagnostics the
//! client hands us back and emits `WorkspaceEdit`-bearing quick-fixes
//! for the schema violations it understands.
//!
//! Each schema-violation diagnostic carries a structured `data` payload
//! (`{ "kind", "name" }`, attached in [`crate::diagnostics`]); we
//! dispatch on `kind` and use `name` for the action title, so there's
//! no message-string parsing.

use std::collections::HashMap;

use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionResponse, Diagnostic, Position,
    Range, TextEdit, Url, WorkspaceEdit,
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
        let Some((kind, name)) = schema_violation_meta(diag) else {
            continue;
        };
        // Each recognised kind deletes the offending line(s); only the
        // title (and which name it cites) differs.
        let title = match kind.as_str() {
            "UnknownField" => format!(
                "Remove unknown field `{}`",
                name.as_deref().unwrap_or("field")
            ),
            "DisallowedChild" => {
                format!(
                    "Remove disallowed `{}` block",
                    name.as_deref().unwrap_or("block")
                )
            }
            _ => continue,
        };
        let range = expand_to_full_lines(source, diag.range);
        actions.push(CodeActionOrCommand::CodeAction(make_action(
            title,
            uri,
            range,
            String::new(),
            diag.clone(),
        )));
    }
    if actions.is_empty() {
        None
    } else {
        Some(actions)
    }
}

/// Read the `{ "kind", "name" }` payload off a `wcl` schema-violation
/// diagnostic. Returns `None` for diagnostics from another source or
/// without the structured `data` we attach.
fn schema_violation_meta(diag: &Diagnostic) -> Option<(String, Option<String>)> {
    if diag.source.as_deref() != Some("wcl") {
        return None;
    }
    let data = diag.data.as_ref()?;
    let kind = data.get("kind")?.as_str()?.to_string();
    let name = data
        .get("name")
        .and_then(|n| n.as_str())
        .map(str::to_string);
    Some((kind, name))
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
            line: crate::scan::line_for_offset(source, start_offset),
            character: 0,
        },
        end: Position {
            line: crate::scan::line_for_offset(source, end_offset),
            character: 0,
        },
    }
}

/// Build one quick-fix action that replaces `range` with `new_text`.
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

    /// A diagnostic carrying the structured `{kind, name}` payload that
    /// real schema violations emit (see `crate::diagnostics`).
    fn diag_with_data(kind: &str, name: &str, line: u32) -> Diagnostic {
        Diagnostic {
            range: Range {
                start: Position { line, character: 0 },
                end: Position {
                    line,
                    character: 10,
                },
            },
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("wcl".into()),
            message: "schema violation".into(),
            data: Some(serde_json::json!({ "kind": kind, "name": name })),
            ..Default::default()
        }
    }

    #[test]
    fn unknown_field_emits_quickfix_with_line_delete() {
        let src = "name = \"alpha\"\nunexpected = \"boom\"\nport = 8080\n";
        let uri = Url::parse("file:///t.wcl").unwrap();
        let diag = diag_with_data("UnknownField", "unexpected", 1);
        let resp = compute(&uri, src, &[diag]).expect("some actions");
        assert_eq!(resp.len(), 1);
        let CodeActionOrCommand::CodeAction(action) = &resp[0] else {
            panic!("expected action")
        };
        assert!(action.title.contains("Remove unknown field"));
        // The name comes from structured data, not message parsing.
        assert!(action.title.contains("unexpected"));
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
    fn disallowed_child_emits_quickfix_with_name() {
        let uri = Url::parse("file:///t.wcl").unwrap();
        let diag = diag_with_data("DisallowedChild", "badchild", 1);
        let resp = compute(&uri, "a {\n  badchild {\n  }\n}\n", &[diag]).expect("some actions");
        let CodeActionOrCommand::CodeAction(action) = &resp[0] else {
            panic!("expected action")
        };
        assert!(action.title.contains("Remove disallowed"));
        assert!(action.title.contains("badchild"));
    }

    #[test]
    fn unrelated_diagnostic_returns_none() {
        let uri = Url::parse("file:///t.wcl").unwrap();
        let mut diag = diag_with_data("UnknownField", "x", 0);
        diag.source = Some("other".into());
        assert!(compute(&uri, "", &[diag]).is_none());
    }

    #[test]
    fn wcl_diagnostic_without_data_returns_none() {
        let uri = Url::parse("file:///t.wcl").unwrap();
        let diag = Diagnostic {
            range: Range::default(),
            source: Some("wcl".into()),
            message: "parse error".into(),
            ..Default::default()
        };
        assert!(compute(&uri, "x = 1\n", &[diag]).is_none());
    }

    /// End-to-end: a real unknown-field document → diagnostics (with
    /// structured data) → a quick-fix that cites the field name.
    #[test]
    fn end_to_end_unknown_field_roundtrip() {
        let src = "@document\ntype Root {\n  region: utf8\n}\n@block(\"service\")\ntype Service {\n  region: utf8\n}\nservice web {\n  region = \"x\"\n  unexpected = \"boom\"\n}\n";
        let uri = Url::parse("file:///t.wcl").unwrap();
        let diags = crate::diagnostics::compute(
            src,
            uri.as_str(),
            None,
            wcl_wdoc::schema_registry().loader(wcl_lang::disk_loader()),
        );
        let resp = compute(&uri, src, &diags).expect("some actions");
        // The doc also flags the top-level `service` block, so search all
        // actions for the unknown-field fix rather than assuming order.
        let titles: Vec<&str> = resp
            .iter()
            .filter_map(|a| match a {
                CodeActionOrCommand::CodeAction(c) => Some(c.title.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            titles
                .iter()
                .any(|t| t.contains("Remove unknown field") && t.contains("unexpected")),
            "{titles:?}"
        );
    }
}
