use wcl_core::diagnostic::DiagnosticBag;
use wcl_core::span::SourceMap;
use wcl_eval::{
    ControlFlowExpander, Evaluator, ImportResolver, MacroExpander, MacroRegistry,
    PartialMerger, RealFileSystem, ScopeEntry, ScopeEntryKind, ScopeKind,
    builtin_signatures,
};
use wcl_schema::{DecoratorSchemaRegistry, IdRegistry, SchemaRegistry};

use crate::state::AnalysisResult;

/// Run the full WCL pipeline, retaining intermediate products for LSP features.
/// This mirrors `wcl::parse()` but keeps tokens, scopes, and macro registry.
pub fn analyze(source: &str, options: &wcl::ParseOptions) -> AnalysisResult {
    let mut source_map = SourceMap::new();
    let file_id = source_map.add_file("<input>".to_string(), source.to_string());
    let mut all_diagnostics = Vec::new();

    // Lex (retain tokens for semantic tokens)
    let tokens = match wcl_core::lexer::lex(source, file_id) {
        Ok(tokens) => tokens,
        Err(lex_errors) => {
            all_diagnostics.extend(lex_errors);
            return AnalysisResult {
                ast: wcl_core::ast::Document {
                    items: Vec::new(),
                    trivia: wcl_core::Trivia::empty(),
                    span: wcl_core::Span::new(file_id, 0, source.len()),
                },
                tokens: Vec::new(),
                source_map,
                file_id,
                diagnostics: all_diagnostics,
                values: indexmap::IndexMap::new(),
                scopes: wcl_eval::ScopeArena::new(),
                schemas: SchemaRegistry::new(),
                macro_registry: MacroRegistry::new(),
                function_signatures: builtin_signatures(),
            };
        }
    };

    // Parse
    let parser = wcl_core::parser::Parser::new(tokens.clone());
    let (mut doc, parse_diags) = parser.parse_document();
    all_diagnostics.extend(parse_diags.into_diagnostics());

    // Macro collection
    let mut macro_registry = MacroRegistry::new();
    let mut diag_bag = DiagnosticBag::new();
    macro_registry.collect(&mut doc, &mut diag_bag);
    all_diagnostics.extend(diag_bag.into_diagnostics());

    // Import resolution
    if options.allow_imports {
        let fs = RealFileSystem;
        let mut resolver = ImportResolver::new(
            &fs,
            &mut source_map,
            options.root_dir.clone(),
            options.max_import_depth,
            options.allow_imports,
        );
        let import_diags = resolver.resolve(
            &mut doc,
            &options.root_dir.join("<input>"),
            0,
        );
        all_diagnostics.extend(import_diags.into_diagnostics());
    }

    // Macro expansion
    let mut expander = MacroExpander::new(&macro_registry, options.max_macro_depth);
    expander.expand(&mut doc);
    all_diagnostics.extend(expander.into_diagnostics().into_diagnostics());

    // Control flow expansion
    let mut cf_expander =
        ControlFlowExpander::new(options.max_loop_depth, options.max_iterations);
    let pre_eval = std::cell::RefCell::new(Evaluator::with_functions(
        &options.functions, None, None,
    ));
    let pre_scope = pre_eval
        .borrow_mut()
        .scopes_mut()
        .create_scope(ScopeKind::Module, None);
    {
        let mut eval = pre_eval.borrow_mut();
        for item in &doc.items {
            if let wcl_core::ast::DocItem::Body(wcl_core::ast::BodyItem::LetBinding(lb)) = item {
                if let Ok(val) = eval.eval_expr(&lb.value, pre_scope) {
                    eval.scopes_mut().add_entry(
                        pre_scope,
                        ScopeEntry {
                            name: lb.name.name.clone(),
                            kind: ScopeEntryKind::LetBinding,
                            value: Some(val),
                            span: lb.span,
                            dependencies: std::collections::HashSet::new(),
                            evaluated: true,
                            read_count: 0,
                        },
                    );
                }
            }
        }
    }
    cf_expander.expand(&mut doc, &|expr| {
        pre_eval
            .borrow_mut()
            .eval_expr(expr, pre_scope)
            .map_err(|d| d.message)
    });
    all_diagnostics.extend(cf_expander.into_diagnostics().into_diagnostics());

    // Partial merge
    let mut merger = PartialMerger::new(options.merge_conflict_mode);
    merger.merge(&mut doc);
    all_diagnostics.extend(merger.into_diagnostics().into_diagnostics());

    // Scope construction + evaluation (retain scopes)
    let mut evaluator = Evaluator::with_functions(
        &options.functions,
        Some(Box::new(RealFileSystem)),
        Some(options.root_dir.clone()),
    );
    let values = evaluator.evaluate(&doc);
    let (scopes, eval_diags) = evaluator.into_parts();
    all_diagnostics.extend(eval_diags.into_diagnostics());

    // Decorator validation
    let mut decorator_schemas = DecoratorSchemaRegistry::new();
    let mut diag_bag = DiagnosticBag::new();
    decorator_schemas.collect(&doc, &mut diag_bag);
    decorator_schemas.validate_all(&doc, &mut diag_bag);
    all_diagnostics.extend(diag_bag.into_diagnostics());

    // Schema validation
    let mut schemas = SchemaRegistry::new();
    let mut diag_bag = DiagnosticBag::new();
    schemas.collect(&doc, &mut diag_bag);
    schemas.validate(&doc, &values, &mut diag_bag);
    all_diagnostics.extend(diag_bag.into_diagnostics());

    // Table validation
    let mut diag_bag = DiagnosticBag::new();
    wcl_schema::table::validate_tables(&doc, &mut diag_bag);
    all_diagnostics.extend(diag_bag.into_diagnostics());

    // ID uniqueness
    let mut id_registry = IdRegistry::new();
    let mut diag_bag = DiagnosticBag::new();
    id_registry.check_document(&doc, &mut diag_bag);
    all_diagnostics.extend(diag_bag.into_diagnostics());

    // Document validation
    let mut diag_bag = DiagnosticBag::new();
    wcl_schema::document::validate_document(
        &doc,
        &mut Evaluator::with_functions(&options.functions, None, None),
        &mut diag_bag,
    );
    all_diagnostics.extend(diag_bag.into_diagnostics());

    // Build function signatures: builtins + custom from options + declared in AST
    let mut function_signatures = builtin_signatures();
    function_signatures.extend(options.functions.signatures.clone());
    // Extract signatures from `declare` statements in the document
    for item in &doc.items {
        if let wcl_core::ast::DocItem::FunctionDecl(decl) = item {
            function_signatures.push(wcl_eval::FunctionSignature {
                name: decl.name.name.clone(),
                params: decl.params.iter().map(|p| {
                    format!("{}: {}", p.name.name, type_expr_to_string(&p.type_expr))
                }).collect(),
                return_type: decl.return_type.as_ref()
                    .map(type_expr_to_string)
                    .unwrap_or_else(|| "any".into()),
                doc: decl.doc.clone().unwrap_or_default(),
            });
        }
    }

    AnalysisResult {
        ast: doc,
        tokens,
        source_map,
        file_id,
        diagnostics: all_diagnostics,
        values,
        scopes,
        schemas,
        macro_registry,
        function_signatures,
    }
}

fn type_expr_to_string(te: &wcl_core::ast::TypeExpr) -> String {
    match te {
        wcl_core::ast::TypeExpr::String(_) => "string".into(),
        wcl_core::ast::TypeExpr::Int(_) => "int".into(),
        wcl_core::ast::TypeExpr::Float(_) => "float".into(),
        wcl_core::ast::TypeExpr::Bool(_) => "bool".into(),
        wcl_core::ast::TypeExpr::Null(_) => "null".into(),
        wcl_core::ast::TypeExpr::Identifier(_) => "identifier".into(),
        wcl_core::ast::TypeExpr::Any(_) => "any".into(),
        wcl_core::ast::TypeExpr::List(inner, _) => format!("list({})", type_expr_to_string(inner)),
        wcl_core::ast::TypeExpr::Map(k, v, _) => format!("map({}, {})", type_expr_to_string(k), type_expr_to_string(v)),
        wcl_core::ast::TypeExpr::Set(inner, _) => format!("set({})", type_expr_to_string(inner)),
        wcl_core::ast::TypeExpr::Ref(_, _) => "ref".into(),
        wcl_core::ast::TypeExpr::Union(types, _) => {
            let parts: Vec<String> = types.iter().map(type_expr_to_string).collect();
            format!("union({})", parts.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_simple() {
        let result = analyze("config { port = 8080 }", &wcl::ParseOptions::default());
        assert!(!result.ast.items.is_empty());
        assert!(!result.tokens.is_empty());
        assert!(result.diagnostics.iter().all(|d| !d.is_error()),
            "unexpected errors: {:?}", result.diagnostics);
    }

    #[test]
    fn test_analyze_with_error() {
        let result = analyze("config { port = }", &wcl::ParseOptions::default());
        assert!(result.diagnostics.iter().any(|d| d.is_error()));
    }

    #[test]
    fn test_analyze_retains_scopes() {
        let result = analyze("let x = 42\nconfig { port = x }", &wcl::ParseOptions::default());
        // Scopes should have at least one entry
        assert!(result.scopes.all_entries().count() > 0);
    }

    #[test]
    fn test_analyze_retains_tokens() {
        let result = analyze("let x = 42", &wcl::ParseOptions::default());
        assert!(!result.tokens.is_empty());
    }

    #[test]
    fn test_analyze_retains_values() {
        let result = analyze("port = 8080", &wcl::ParseOptions::default());
        assert!(result.values.contains_key("port"));
    }

    #[test]
    fn test_analyze_lex_error_returns_partial() {
        // Source with an unterminated string should not panic
        let result = analyze("name = \"unterminated", &wcl::ParseOptions::default());
        assert!(result.diagnostics.iter().any(|d| d.is_error()));
    }
}
