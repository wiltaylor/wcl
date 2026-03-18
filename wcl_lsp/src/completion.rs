use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind};

use crate::state::AnalysisResult;

/// Built-in WCL keywords
const KEYWORDS: &[&str] = &[
    "let", "partial", "macro", "schema", "table", "import", "export", "query", "ref", "for",
    "in", "if", "else", "when", "inject", "set", "remove", "self", "validation",
    "decorator_schema", "declare", "true", "false", "null",
];

/// Built-in WCL type names
const TYPE_NAMES: &[&str] = &[
    "string", "int", "float", "bool", "null", "identifier", "any", "list", "map", "set", "ref",
    "union",
];

/// Built-in decorator names
const BUILTIN_DECORATORS: &[&str] = &[
    "deprecated", "warning", "schema", "optional", "sensitive", "env", "readonly",
    "partial_requires",
];

/// Collect function names from analysis signatures (builtins + custom).
fn function_names(analysis: &AnalysisResult) -> Vec<&str> {
    analysis.function_signatures.iter().map(|s| s.name.as_str()).collect()
}

pub fn completions(
    analysis: &AnalysisResult,
    source: &str,
    offset: usize,
) -> Vec<CompletionItem> {
    let context = detect_context(source, offset);
    let mut items = Vec::new();

    match context {
        CompletionContext::Decorator => {
            for name in BUILTIN_DECORATORS {
                items.push(CompletionItem {
                    label: name.to_string(),
                    kind: Some(CompletionItemKind::PROPERTY),
                    detail: Some("decorator".to_string()),
                    ..Default::default()
                });
            }
            // Add attribute macros from the registry
            for item in &analysis.ast.items {
                if let wcl_core::ast::DocItem::Body(wcl_core::ast::BodyItem::MacroDef(md)) = item {
                    if md.kind == wcl_core::ast::MacroKind::Attribute {
                        items.push(CompletionItem {
                            label: md.name.name.clone(),
                            kind: Some(CompletionItemKind::PROPERTY),
                            detail: Some("attribute macro".to_string()),
                            ..Default::default()
                        });
                    }
                }
            }
        }
        CompletionContext::MemberAccess => {
            // After a `.`, suggest known attribute names from the analysis values
            for name in analysis.values.keys() {
                items.push(CompletionItem {
                    label: name.clone(),
                    kind: Some(CompletionItemKind::PROPERTY),
                    detail: Some("attribute".to_string()),
                    ..Default::default()
                });
            }
            // Also add attribute names from scope entries
            for (_, entry) in analysis.scopes.all_entries() {
                if entry.kind == wcl_eval::ScopeEntryKind::Attribute {
                    items.push(CompletionItem {
                        label: entry.name.clone(),
                        kind: Some(CompletionItemKind::PROPERTY),
                        detail: Some("attribute".to_string()),
                        ..Default::default()
                    });
                }
            }
        }
        CompletionContext::StringInterpolation => {
            // Inside "${...}", offer expression completions (variables and functions)
            for (_, entry) in analysis.scopes.all_entries() {
                items.push(CompletionItem {
                    label: entry.name.clone(),
                    kind: Some(match entry.kind {
                        wcl_eval::ScopeEntryKind::LetBinding
                        | wcl_eval::ScopeEntryKind::ExportLet
                        | wcl_eval::ScopeEntryKind::IteratorVar => CompletionItemKind::VARIABLE,
                        wcl_eval::ScopeEntryKind::Attribute => CompletionItemKind::PROPERTY,
                        wcl_eval::ScopeEntryKind::BlockChild => CompletionItemKind::CLASS,
                    }),
                    ..Default::default()
                });
            }
            for name in function_names(analysis) {
                items.push(CompletionItem {
                    label: name.to_string(),
                    kind: Some(CompletionItemKind::FUNCTION),
                    detail: Some("builtin".to_string()),
                    ..Default::default()
                });
            }
        }
        CompletionContext::Expression => {
            // Variables from scope
            for (_, entry) in analysis.scopes.all_entries() {
                items.push(CompletionItem {
                    label: entry.name.clone(),
                    kind: Some(match entry.kind {
                        wcl_eval::ScopeEntryKind::LetBinding
                        | wcl_eval::ScopeEntryKind::ExportLet
                        | wcl_eval::ScopeEntryKind::IteratorVar => CompletionItemKind::VARIABLE,
                        wcl_eval::ScopeEntryKind::Attribute => CompletionItemKind::PROPERTY,
                        wcl_eval::ScopeEntryKind::BlockChild => CompletionItemKind::CLASS,
                    }),
                    ..Default::default()
                });
            }
            // Built-in functions
            for name in function_names(analysis) {
                items.push(CompletionItem {
                    label: name.to_string(),
                    kind: Some(CompletionItemKind::FUNCTION),
                    detail: Some("builtin".to_string()),
                    ..Default::default()
                });
            }
        }
        CompletionContext::TopLevel | CompletionContext::BlockBody => {
            for kw in KEYWORDS {
                items.push(CompletionItem {
                    label: kw.to_string(),
                    kind: Some(CompletionItemKind::KEYWORD),
                    ..Default::default()
                });
            }
            // Block kinds from document
            let mut seen = std::collections::HashSet::new();
            collect_block_kinds(&analysis.ast, &mut seen);
            for kind in seen {
                items.push(CompletionItem {
                    label: kind,
                    kind: Some(CompletionItemKind::CLASS),
                    detail: Some("block type".to_string()),
                    ..Default::default()
                });
            }
            // In block body, also offer known attribute names
            if matches!(context, CompletionContext::BlockBody) {
                for name in analysis.values.keys() {
                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: Some(CompletionItemKind::PROPERTY),
                        detail: Some("attribute".to_string()),
                        ..Default::default()
                    });
                }
            }
        }
        CompletionContext::Type => {
            for name in TYPE_NAMES {
                items.push(CompletionItem {
                    label: name.to_string(),
                    kind: Some(CompletionItemKind::TYPE_PARAMETER),
                    ..Default::default()
                });
            }
        }
    }

    // Deduplicate by label
    items.sort_by(|a, b| a.label.cmp(&b.label));
    items.dedup_by(|a, b| a.label == b.label);
    items
}

#[derive(Debug)]
enum CompletionContext {
    TopLevel,
    BlockBody,
    Expression,
    Decorator,
    Type,
    MemberAccess,
    StringInterpolation,
}

fn detect_context(source: &str, offset: usize) -> CompletionContext {
    let before = &source[..offset.min(source.len())];

    // Check if cursor is inside a string or comment
    if is_in_string_or_comment(before) {
        return CompletionContext::Expression; // minimal completions inside strings
    }

    // Check if cursor is inside a string interpolation `"...${"`
    if is_in_string_interpolation(before) {
        return CompletionContext::StringInterpolation;
    }

    // Check for @ prefix (skipping whitespace)
    if before.ends_with('@') || before.trim_end().ends_with('@') {
        return CompletionContext::Decorator;
    }

    // Check for = (expression context)
    let trimmed = before.trim_end();

    // Check for `.` member access
    if trimmed.ends_with('.') {
        return CompletionContext::MemberAccess;
    }

    if trimmed.ends_with('=') && !trimmed.ends_with("==") && !trimmed.ends_with("!=") {
        return CompletionContext::Expression;
    }

    // Check for : (type context, e.g. inside schema or table column)
    if trimmed.ends_with(':') {
        return CompletionContext::Type;
    }

    // Count braces to determine if inside a block (string/comment-aware)
    let mut depth = 0i32;
    let mut in_string = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut prev_char = '\0';

    for ch in before.chars() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
            }
        } else if in_block_comment {
            if prev_char == '*' && ch == '/' {
                in_block_comment = false;
            }
        } else if in_string {
            if ch == '"' && prev_char != '\\' {
                in_string = false;
            }
        } else {
            match ch {
                '"' => in_string = true,
                '/' if prev_char == '/' => in_line_comment = true,
                '*' if prev_char == '/' => in_block_comment = true,
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
        }
        prev_char = ch;
    }

    if depth > 0 {
        CompletionContext::BlockBody
    } else {
        CompletionContext::TopLevel
    }
}

/// Check if cursor position is inside a `"${...}"` string interpolation.
fn is_in_string_interpolation(before: &str) -> bool {
    let mut in_string = false;
    let mut in_interp_depth: i32 = 0;
    let mut prev_char = '\0';

    for ch in before.chars() {
        if in_interp_depth > 0 {
            match ch {
                '{' => in_interp_depth += 1,
                '}' => {
                    in_interp_depth -= 1;
                    if in_interp_depth == 0 {
                        // Back inside the string
                    }
                }
                _ => {}
            }
        } else if in_string {
            if ch == '"' && prev_char != '\\' {
                in_string = false;
            } else if ch == '{' && prev_char == '$' {
                in_interp_depth = 1;
            }
        } else if ch == '"' {
            in_string = true;
        }
        prev_char = ch;
    }

    in_interp_depth > 0
}

/// Check if cursor position is inside a string literal or comment.
fn is_in_string_or_comment(before: &str) -> bool {
    let mut in_string = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut prev_char = '\0';

    for ch in before.chars() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
            }
        } else if in_block_comment {
            if prev_char == '*' && ch == '/' {
                in_block_comment = false;
            }
        } else if in_string {
            if ch == '"' && prev_char != '\\' {
                in_string = false;
            }
        } else {
            match ch {
                '"' => in_string = true,
                '/' if prev_char == '/' => in_line_comment = true,
                '*' if prev_char == '/' => in_block_comment = true,
                _ => {}
            }
        }
        prev_char = ch;
    }

    in_string || in_line_comment || in_block_comment
}

fn collect_block_kinds(doc: &wcl_core::ast::Document, seen: &mut std::collections::HashSet<String>) {
    for item in &doc.items {
        if let wcl_core::ast::DocItem::Body(wcl_core::ast::BodyItem::Block(block)) = item {
            seen.insert(block.kind.name.clone());
            collect_block_kinds_in_body(&block.body, seen);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::analyze;

    fn get_completions(source: &str, offset: usize) -> Vec<CompletionItem> {
        let analysis = analyze(source, &wcl::ParseOptions::default());
        completions(&analysis, source, offset)
    }

    #[test]
    fn test_top_level_has_keywords() {
        let items = get_completions("", 0);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"let"));
        assert!(labels.contains(&"schema"));
        assert!(labels.contains(&"import"));
    }

    #[test]
    fn test_decorator_context() {
        let source = "@ ";
        let items = get_completions(source, 1);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"deprecated"));
        assert!(labels.contains(&"schema"));
    }

    #[test]
    fn test_expression_context() {
        let source = "let x = 42\nconfig { port = ";
        let items = get_completions(source, source.len());
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        // Should have builtin functions
        assert!(labels.contains(&"upper"));
        assert!(labels.contains(&"len"));
    }

    #[test]
    fn test_type_context() {
        let source = "schema \"s\" { name: ";
        let items = get_completions(source, source.len());
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"string"));
        assert!(labels.contains(&"int"));
        assert!(labels.contains(&"list"));
    }

    #[test]
    fn test_block_body_has_keywords() {
        let source = "config { port = 8080\n";
        let items = get_completions(source, source.len());
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"let"));
    }

    #[test]
    fn test_in_string_returns_expression_context() {
        // Inside a string literal, should not give decorator completions
        let source = "name = \"hello @";
        let items = get_completions(source, source.len());
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        // Should NOT have decorator completions
        assert!(!labels.contains(&"deprecated"));
    }

    #[test]
    fn test_in_comment_returns_expression_context() {
        let source = "// comment @";
        let items = get_completions(source, source.len());
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(!labels.contains(&"deprecated"));
    }

    #[test]
    fn test_member_access_context() {
        let source = "config { port = 8080 }\nlet x = config.";
        let items = get_completions(source, source.len());
        // Should offer attribute names (from values), not keywords
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(!labels.contains(&"let"), "should not contain keywords in member access context");
        // All items should be property kind
        for item in &items {
            assert_eq!(item.kind, Some(CompletionItemKind::PROPERTY));
        }
    }

    #[test]
    fn test_completions_deduplicated() {
        // Build a source that would produce duplicate attribute names from both
        // values and scope entries
        let source = "config { port = 8080\nhost = \"localhost\"\n";
        let items = get_completions(source, source.len());
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        // Check no duplicates
        let mut seen = std::collections::HashSet::new();
        for label in &labels {
            assert!(seen.insert(label), "duplicate completion label: {}", label);
        }
    }
}

fn collect_block_kinds_in_body(body: &[wcl_core::ast::BodyItem], seen: &mut std::collections::HashSet<String>) {
    for item in body {
        if let wcl_core::ast::BodyItem::Block(block) = item {
            seen.insert(block.kind.name.clone());
            collect_block_kinds_in_body(&block.body, seen);
        }
    }
}
