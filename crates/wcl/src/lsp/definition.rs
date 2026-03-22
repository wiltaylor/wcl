use std::path::Path;

use crate::lang::ast;
use ropey::Rope;
use tower_lsp::lsp_types::{GotoDefinitionResponse, Location, Url};

use crate::lsp::ast_utils::{find_node_at_offset, NodeAtOffset};
use crate::lsp::convert::span_to_lsp_range;
use crate::lsp::state::AnalysisResult;

pub fn goto_definition(
    analysis: &AnalysisResult,
    offset: usize,
    rope: &Rope,
    uri: &Url,
) -> Option<GotoDefinitionResponse> {
    let node = find_node_at_offset(&analysis.ast, offset);

    match node {
        NodeAtOffset::IdentRef(ident) => {
            // Search all scopes for the definition, returning the first match.
            // Limitation: scopes are iterated in creation order (document order),
            // so the first match may not always be the innermost/closest definition
            // if shadowing is involved. A proper fix would require tracking which
            // scope is active at the cursor offset.
            for scope in analysis.scopes.all_scopes() {
                if let Some(entry) = scope.entries.get(&ident.name) {
                    if entry.span.start == 0 && entry.span.end == 0 {
                        continue;
                    }
                    let range = span_to_lsp_range(entry.span, rope);
                    return Some(GotoDefinitionResponse::Scalar(Location {
                        uri: uri.clone(),
                        range,
                    }));
                }
            }
            None
        }
        NodeAtOffset::MacroCallName(mc) => {
            // Walk entire AST (including nested blocks) to find MacroDef with matching name
            find_macro_def(&analysis.ast.items, &mc.name.name).map(|span| {
                GotoDefinitionResponse::Scalar(Location {
                    uri: uri.clone(),
                    range: span_to_lsp_range(span, rope),
                })
            })
        }
        NodeAtOffset::AttributeName(attr) => {
            // Jump to the attribute's own span (the whole attribute)
            Some(GotoDefinitionResponse::Scalar(Location {
                uri: uri.clone(),
                range: span_to_lsp_range(attr.span, rope),
            }))
        }
        NodeAtOffset::LetBindingName(lb) => Some(GotoDefinitionResponse::Scalar(Location {
            uri: uri.clone(),
            range: span_to_lsp_range(lb.span, rope),
        })),
        NodeAtOffset::ImportPath(import) => resolve_import_path(import, uri),
        _ => None,
    }
}

pub fn goto_type_definition(
    analysis: &AnalysisResult,
    offset: usize,
    rope: &Rope,
    uri: &Url,
) -> Option<GotoDefinitionResponse> {
    let node = find_node_at_offset(&analysis.ast, offset);
    match node {
        NodeAtOffset::BlockKind(block) => {
            let schema_name = &block.kind.name;
            find_schema_in_ast(&analysis.ast, schema_name).map(|span| {
                GotoDefinitionResponse::Scalar(Location {
                    uri: uri.clone(),
                    range: span_to_lsp_range(span, rope),
                })
            })
        }
        _ => None,
    }
}

/// Walk AST to find a Schema whose name matches `target_name`, returning its name span.
fn find_schema_in_ast(
    doc: &ast::Document,
    target_name: &str,
) -> Option<crate::lang::span::Span> {
    for item in &doc.items {
        if let ast::DocItem::Body(ast::BodyItem::Schema(schema)) = item {
            let name = crate::schema::schema::string_lit_to_string(&schema.name);
            if name == target_name {
                return Some(schema.span);
            }
        }
    }
    None
}

/// Resolve an import statement to a file Location.
///
/// Handles relative paths, absolute paths, and library imports (`import <name.wcl>`).
fn resolve_import_path(import: &ast::Import, current_uri: &Url) -> Option<GotoDefinitionResponse> {
    let path_str: String = import
        .path
        .parts
        .iter()
        .filter_map(|p| match p {
            ast::StringPart::Literal(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();

    if path_str.is_empty() {
        return None;
    }

    let resolved = if import.kind == ast::ImportKind::Library {
        // Search library paths
        crate::eval::resolve_library_import(
            &path_str,
            &crate::eval::RealFileSystem,
            &crate::eval::LibraryConfig::default(),
        )?
    } else {
        let import_path = Path::new(&path_str);
        if import_path.is_absolute() {
            import_path.to_path_buf()
        } else {
            let current_file = current_uri.to_file_path().ok()?;
            let current_dir = current_file.parent()?;
            current_dir.join(import_path)
        }
    };

    let target_uri = Url::from_file_path(&resolved).ok()?;
    Some(GotoDefinitionResponse::Scalar(Location {
        uri: target_uri,
        range: tower_lsp::lsp_types::Range::default(),
    }))
}

/// Recursively walk AST items to find a MacroDef with the given name.
fn find_macro_def(items: &[ast::DocItem], name: &str) -> Option<crate::lang::span::Span> {
    for item in items {
        if let ast::DocItem::Body(body_item) = item {
            if let Some(span) = find_macro_def_in_body(body_item, name) {
                return Some(span);
            }
        }
    }
    None
}

fn find_macro_def_in_body(item: &ast::BodyItem, name: &str) -> Option<crate::lang::span::Span> {
    match item {
        ast::BodyItem::MacroDef(md) if md.name.name == name => Some(md.span),
        ast::BodyItem::Block(block) => {
            for child in &block.body {
                if let Some(span) = find_macro_def_in_body(child, name) {
                    return Some(span);
                }
            }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::analysis::analyze;
    use tower_lsp::lsp_types::Url;

    #[test]
    fn test_goto_definition_let_binding() {
        let source = "let x = 42\nconfig { port = x }";
        let analysis = analyze(source, &crate::ParseOptions::default());
        let rope = Rope::from_str(source);
        let uri = Url::parse("file:///test.wcl").unwrap();
        // 'x' reference at offset 24 (in "port = x")
        let offset = source.find("= x").unwrap() + 2;
        let result = goto_definition(&analysis, offset, &rope, &uri);
        assert!(result.is_some());
    }

    #[test]
    fn test_goto_definition_none_for_literal() {
        let source = "config { port = 8080 }";
        let analysis = analyze(source, &crate::ParseOptions::default());
        let rope = Rope::from_str(source);
        let uri = Url::parse("file:///test.wcl").unwrap();
        // Offset at "8080" — no definition
        let offset = source.find("8080").unwrap();
        let result = goto_definition(&analysis, offset, &rope, &uri);
        assert!(result.is_none());
    }

    #[test]
    fn test_goto_definition_attribute() {
        let source = "server { host = \"localhost\" }";
        let analysis = analyze(source, &crate::ParseOptions::default());
        let rope = Rope::from_str(source);
        let uri = Url::parse("file:///test.wcl").unwrap();
        // Click on the attribute name "host"
        let offset = source.find("host").unwrap();
        let result = goto_definition(&analysis, offset, &rope, &uri);
        // Attribute goto-def should return its own span
        assert!(result.is_some());
        if let Some(GotoDefinitionResponse::Scalar(loc)) = result {
            assert_eq!(loc.uri, uri);
        } else {
            panic!("expected Scalar response");
        }
    }

    #[test]
    fn test_goto_definition_import_relative() {
        // Parse a source with an import — the parser produces an Import node
        let source = r#"import "./other.wcl""#;
        let analysis = analyze(source, &crate::ParseOptions::default());
        let rope = Rope::from_str(source);
        // Use a file URI with a directory so relative resolution works
        let uri = Url::parse("file:///home/user/project/main.wcl").unwrap();
        let offset = source.find("./other").unwrap();
        let result = goto_definition(&analysis, offset, &rope, &uri);
        // Should resolve to /home/user/project/other.wcl
        assert!(result.is_some());
        if let Some(GotoDefinitionResponse::Scalar(loc)) = result {
            assert_eq!(
                loc.uri,
                Url::parse("file:///home/user/project/other.wcl").unwrap()
            );
        } else {
            panic!("expected Scalar response");
        }
    }

    #[test]
    fn test_goto_definition_block_kind_returns_none() {
        let source = "server { port = 8080 }";
        let analysis = analyze(source, &crate::ParseOptions::default());
        let rope = Rope::from_str(source);
        let uri = Url::parse("file:///test.wcl").unwrap();
        // Click on the block kind "server" — should return None (it's not a reference)
        let offset = source.find("server").unwrap();
        let result = goto_definition(&analysis, offset, &rope, &uri);
        assert!(result.is_none());
    }

    #[test]
    fn test_goto_type_definition_block_to_schema() {
        let source = "schema \"server\" {\n    port: int\n}\nserver web { port = 8080 }";
        let analysis = analyze(source, &crate::ParseOptions::default());
        let rope = Rope::from_str(source);
        let uri = Url::parse("file:///test.wcl").unwrap();
        // Click on "server" block kind → should navigate to the schema
        let offset = source.rfind("server").unwrap();
        let result = goto_type_definition(&analysis, offset, &rope, &uri);
        assert!(
            result.is_some(),
            "expected goto_type_definition to find schema"
        );
    }

    #[test]
    fn test_goto_type_definition_no_schema() {
        let source = "server web { port = 8080 }";
        let analysis = analyze(source, &crate::ParseOptions::default());
        let rope = Rope::from_str(source);
        let uri = Url::parse("file:///test.wcl").unwrap();
        let offset = source.find("server").unwrap();
        let result = goto_type_definition(&analysis, offset, &rope, &uri);
        assert!(result.is_none());
    }
}
