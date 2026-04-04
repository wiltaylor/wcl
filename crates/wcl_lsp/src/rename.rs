use std::collections::HashMap;

use async_lsp::lsp_types::{Range, TextEdit, Url, WorkspaceEdit};
use ropey::Rope;

use crate::ast_utils::{find_node_at_offset, NodeAtOffset};
use crate::convert::span_to_lsp_range;
use crate::references::find_references;
use crate::state::AnalysisResult;

/// Check whether the identifier at `offset` can be renamed, returning its range.
pub fn prepare_rename(analysis: &AnalysisResult, offset: usize, rope: &Rope) -> Option<Range> {
    let node = find_node_at_offset(&analysis.ast, offset);
    let span = match node {
        NodeAtOffset::IdentRef(ident) => ident.span,
        NodeAtOffset::AttributeName(attr) => attr.name.span,
        NodeAtOffset::LetBindingName(lb) => lb.name.span,
        NodeAtOffset::MacroDefName(md) => md.name.span,
        NodeAtOffset::MacroCallName(mc) => mc.name.span,
        NodeAtOffset::BlockKind(block) => block.kind.span,
        _ => return None,
    };
    Some(span_to_lsp_range(span, rope))
}

/// Rename the identifier at `offset` to `new_name`, returning edits for all references.
pub fn rename(
    analysis: &AnalysisResult,
    offset: usize,
    new_name: &str,
    rope: &Rope,
    uri: &Url,
) -> Option<WorkspaceEdit> {
    let locations = find_references(analysis, offset, rope, uri, true);
    if locations.is_empty() {
        return None;
    }

    let edits: Vec<TextEdit> = locations
        .into_iter()
        .map(|loc| TextEdit {
            range: loc.range,
            new_text: new_name.to_string(),
        })
        .collect();

    let mut changes = HashMap::new();
    changes.insert(uri.clone(), edits);
    Some(WorkspaceEdit {
        changes: Some(changes),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::analyze;

    #[test]
    fn test_prepare_rename_let_binding() {
        let source = "let x = 42\nconfig { port = x }";
        let analysis = analyze(source, &wcl_lang::ParseOptions::default());
        let rope = Rope::from_str(source);
        let offset = source.find("x").unwrap();
        let result = prepare_rename(&analysis, offset, &rope);
        assert!(result.is_some());
    }

    #[test]
    fn test_prepare_rename_literal_fails() {
        let source = "config { port = 8080 }";
        let analysis = analyze(source, &wcl_lang::ParseOptions::default());
        let rope = Rope::from_str(source);
        let offset = source.find("8080").unwrap();
        let result = prepare_rename(&analysis, offset, &rope);
        assert!(result.is_none());
    }

    #[test]
    fn test_rename_let_binding() {
        let source = "let x = 42\nconfig { port = x }";
        let analysis = analyze(source, &wcl_lang::ParseOptions::default());
        let rope = Rope::from_str(source);
        let uri = Url::parse("file:///test.wcl").unwrap();
        let offset = source.find("x").unwrap();
        let result = rename(&analysis, offset, "y", &rope, &uri);
        assert!(result.is_some());
        let ws = result.unwrap();
        let edits = ws.changes.unwrap();
        let file_edits = edits.get(&uri).unwrap();
        // Should rename both the definition and the usage
        assert!(
            file_edits.len() >= 2,
            "expected >= 2 edits, got {}",
            file_edits.len()
        );
        assert!(file_edits.iter().all(|e| e.new_text == "y"));
    }

    #[test]
    fn test_rename_block_kind() {
        let source = "server web { port = 8080 }\nserver api { port = 9090 }";
        let analysis = analyze(source, &wcl_lang::ParseOptions::default());
        let rope = Rope::from_str(source);
        let uri = Url::parse("file:///test.wcl").unwrap();
        let offset = source.find("server").unwrap();
        let result = rename(&analysis, offset, "service", &rope, &uri);
        assert!(result.is_some());
        let ws = result.unwrap();
        let edits = ws.changes.unwrap();
        let file_edits = edits.get(&uri).unwrap();
        assert_eq!(file_edits.len(), 2);
        assert!(file_edits.iter().all(|e| e.new_text == "service"));
    }
}
