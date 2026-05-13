use async_lsp::lsp_types::{CompletionItem, CompletionItemKind};

use crate::state::AnalysisResult;

/// Built-in WCL keywords
const KEYWORDS: &[&str] = &[
    "let",
    "partial",
    "macro",
    "schema",
    "table",
    "import",
    "export",
    "query",
    "ref",
    "for",
    "in",
    "if",
    "else",
    "when",
    "inject",
    "set",
    "remove",
    "self",
    "validation",
    "decorator_schema",
    "declare",
    "true",
    "false",
    "null",
];

/// Built-in WCL type names
const TYPE_NAMES: &[&str] = &[
    "string",
    "i8",
    "u8",
    "i16",
    "u16",
    "i32",
    "u32",
    "i64",
    "u64",
    "i128",
    "u128",
    "f32",
    "f64",
    "date",
    "duration",
    "bool",
    "null",
    "identifier",
    "any",
    "list",
    "map",
    "set",
    "ref",
    "union",
];

/// Built-in decorator names
const BUILTIN_DECORATORS: &[&str] = &[
    "deprecated",
    "warning",
    "schema",
    "optional",
    "sensitive",
    "env",
    "readonly",
    "partial_requires",
];

/// Collect function names from analysis signatures (builtins + custom).
fn function_names(analysis: &AnalysisResult) -> Vec<&str> {
    analysis
        .function_signatures
        .iter()
        .map(|s| s.name.as_str())
        .collect()
}

fn namespace_function_members<'a>(analysis: &'a AnalysisResult, namespace: &str) -> Vec<&'a str> {
    let prefix = format!("{namespace}::");
    analysis
        .function_signatures
        .iter()
        .filter_map(|s| s.name.strip_prefix(&prefix))
        .filter(|name| !name.is_empty())
        .collect()
}

fn push_function_items(items: &mut Vec<CompletionItem>, names: impl IntoIterator<Item = String>) {
    for name in names {
        items.push(CompletionItem {
            label: name,
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some("function".to_string()),
            ..Default::default()
        });
    }
}

pub fn completions(analysis: &AnalysisResult, source: &str, offset: usize) -> Vec<CompletionItem> {
    let context = detect_context(source, offset);
    let mut items = Vec::new();

    match context {
        CompletionContext::NamespaceAccess(namespace) => {
            push_function_items(
                &mut items,
                namespace_function_members(analysis, &namespace)
                    .into_iter()
                    .map(str::to_string),
            );
        }
        CompletionContext::UseMembers(namespace) => {
            push_function_items(
                &mut items,
                namespace_function_members(analysis, &namespace)
                    .into_iter()
                    .map(str::to_string),
            );
        }
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
                if let wcl_lang::lang::ast::DocItem::Body(
                    wcl_lang::lang::ast::BodyItem::MacroDef(md),
                ) = item
                {
                    if md.kind == wcl_lang::lang::ast::MacroKind::Attribute {
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
                if entry.kind == wcl_lang::eval::ScopeEntryKind::Attribute {
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
                        wcl_lang::eval::ScopeEntryKind::LetBinding
                        | wcl_lang::eval::ScopeEntryKind::ExportLet
                        | wcl_lang::eval::ScopeEntryKind::IteratorVar => {
                            CompletionItemKind::VARIABLE
                        }
                        wcl_lang::eval::ScopeEntryKind::Attribute
                        | wcl_lang::eval::ScopeEntryKind::TableEntry => {
                            CompletionItemKind::PROPERTY
                        }
                        wcl_lang::eval::ScopeEntryKind::BlockChild => CompletionItemKind::CLASS,
                    }),
                    ..Default::default()
                });
            }
            push_function_items(
                &mut items,
                function_names(analysis).into_iter().map(str::to_string),
            );
        }
        CompletionContext::Expression => {
            // Variables from scope
            for (_, entry) in analysis.scopes.all_entries() {
                items.push(CompletionItem {
                    label: entry.name.clone(),
                    kind: Some(match entry.kind {
                        wcl_lang::eval::ScopeEntryKind::LetBinding
                        | wcl_lang::eval::ScopeEntryKind::ExportLet
                        | wcl_lang::eval::ScopeEntryKind::IteratorVar => {
                            CompletionItemKind::VARIABLE
                        }
                        wcl_lang::eval::ScopeEntryKind::Attribute
                        | wcl_lang::eval::ScopeEntryKind::TableEntry => {
                            CompletionItemKind::PROPERTY
                        }
                        wcl_lang::eval::ScopeEntryKind::BlockChild => CompletionItemKind::CLASS,
                    }),
                    ..Default::default()
                });
            }
            push_function_items(
                &mut items,
                function_names(analysis).into_iter().map(str::to_string),
            );
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
    NamespaceAccess(String),
    UseMembers(String),
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

    if let Some(namespace) = detect_use_members_context(trimmed) {
        return CompletionContext::UseMembers(namespace);
    }

    if let Some(namespace) = trimmed.strip_suffix("::").and_then(namespace_before_cursor) {
        return CompletionContext::NamespaceAccess(namespace.to_string());
    }

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

fn namespace_before_cursor(prefix: &str) -> Option<&str> {
    let start = prefix
        .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != ':')
        .map(|idx| idx + 1)
        .unwrap_or(0);
    let namespace = &prefix[start..];
    if namespace.is_empty() || namespace.ends_with(':') {
        None
    } else {
        Some(namespace)
    }
}

fn detect_use_members_context(trimmed: &str) -> Option<String> {
    let line = trimmed.rsplit('\n').next().unwrap_or(trimmed).trim_start();
    let after_use = line.strip_prefix("use ")?;
    let brace = after_use.rfind("::{")?;
    if after_use[brace + 3..].contains('}') {
        return None;
    }
    let namespace = after_use[..brace].trim();
    if namespace.is_empty() {
        None
    } else {
        Some(namespace.to_string())
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

fn collect_block_kinds(
    doc: &wcl_lang::lang::ast::Document,
    seen: &mut std::collections::HashSet<String>,
) {
    for item in &doc.items {
        if let wcl_lang::lang::ast::DocItem::Body(wcl_lang::lang::ast::BodyItem::Block(block)) =
            item
        {
            seen.insert(block.kind.name.clone());
            collect_block_kinds_in_body(&block.body, seen);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::analyze;
    use std::sync::Arc;

    fn get_completions(source: &str, offset: usize) -> Vec<CompletionItem> {
        let analysis = analyze(source, &wcl_lang::ParseOptions::default());
        completions(&analysis, source, offset)
    }

    fn wdoc_like_options() -> wcl_lang::ParseOptions {
        let mut functions = wcl_lang::FunctionRegistry::new();
        let dummy: wcl_lang::BuiltinFn =
            Arc::new(|_: &[wcl_lang::Value]| Ok(wcl_lang::Value::Null));
        functions.register(
            "icon",
            dummy.clone(),
            wcl_lang::FunctionSignature {
                name: "icon".into(),
                params: vec!["name: string".into()],
                return_type: "string".into(),
                doc: "Render a named SVG icon".into(),
            },
        );
        functions.register(
            "wdoc::icon",
            dummy.clone(),
            wcl_lang::FunctionSignature {
                name: "wdoc::icon".into(),
                params: vec!["name: string".into()],
                return_type: "string".into(),
                doc: "Render a named SVG icon from WDoc".into(),
            },
        );
        functions.register(
            "wdoc::measure_text",
            dummy,
            wcl_lang::FunctionSignature {
                name: "wdoc::measure_text".into(),
                params: vec!["text: any".into()],
                return_type: "map".into(),
                doc: "Measure text".into(),
            },
        );
        wcl_lang::ParseOptions {
            functions,
            ..Default::default()
        }
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
        assert!(labels.contains(&"i64"));
        assert!(labels.contains(&"f64"));
        assert!(labels.contains(&"date"));
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
        assert!(
            !labels.contains(&"let"),
            "should not contain keywords in member access context"
        );
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

    #[test]
    fn test_expression_context_includes_custom_wdoc_functions() {
        let source = "let x = ";
        let analysis = analyze(source, &wdoc_like_options());
        let items = completions(&analysis, source, source.len());
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"icon"));
        assert!(labels.contains(&"wdoc::icon"));
    }

    #[test]
    fn test_namespace_access_completion() {
        let source = "let x = wdoc::";
        let analysis = analyze(source, &wdoc_like_options());
        let items = completions(&analysis, source, source.len());
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"icon"));
        assert!(labels.contains(&"measure_text"));
        assert!(!labels.contains(&"wdoc::icon"));
    }

    #[test]
    fn test_use_members_completion() {
        let source = "use wdoc::{";
        let analysis = analyze(source, &wdoc_like_options());
        let items = completions(&analysis, source, source.len());
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"icon"));
        assert!(labels.contains(&"measure_text"));
    }
}

fn collect_block_kinds_in_body(
    body: &[wcl_lang::lang::ast::BodyItem],
    seen: &mut std::collections::HashSet<String>,
) {
    for item in body {
        if let wcl_lang::lang::ast::BodyItem::Block(block) = item {
            seen.insert(block.kind.name.clone());
            collect_block_kinds_in_body(&block.body, seen);
        }
    }
}
