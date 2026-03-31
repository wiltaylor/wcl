use crate::eval::{
    builtin_signatures, ControlFlowExpander, Evaluator, ImportResolver, MacroExpander,
    MacroRegistry, PartialMerger, RealFileSystem, ScopeEntry, ScopeEntryKind, ScopeKind,
};
use crate::lang::diagnostic::DiagnosticBag;
use crate::lang::span::SourceMap;
use crate::schema::{DecoratorSchemaRegistry, IdRegistry, SchemaRegistry};

use crate::lsp::state::AnalysisResult;

/// Run the full WCL pipeline, retaining intermediate products for LSP features.
/// This mirrors `crate::parse()` but keeps tokens, scopes, and macro registry.
pub fn analyze(source: &str, options: &crate::ParseOptions) -> AnalysisResult {
    let mut source_map = SourceMap::new();
    let file_id = source_map.add_file("<input>".to_string(), source.to_string());
    let mut all_diagnostics = Vec::new();

    // Lex (retain tokens for semantic tokens)
    let tokens = match crate::lang::lexer::lex(source, file_id) {
        Ok(tokens) => tokens,
        Err(lex_errors) => {
            all_diagnostics.extend(lex_errors);
            return AnalysisResult {
                ast: crate::lang::ast::Document {
                    items: Vec::new(),
                    trivia: crate::lang::Trivia::empty(),
                    span: crate::lang::Span::new(file_id, 0, source.len()),
                },
                tokens: Vec::new(),
                source_map,
                file_id,
                diagnostics: all_diagnostics,
                values: indexmap::IndexMap::new(),
                scopes: crate::eval::ScopeArena::new(),
                schemas: SchemaRegistry::new(),
                macro_registry: MacroRegistry::new(),
                function_signatures: builtin_signatures(),
            };
        }
    };

    // Parse
    let parser = crate::lang::parser::Parser::new(tokens.clone());
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
        let library_config = crate::eval::LibraryConfig {
            extra_paths: options.lib_paths.clone(),
            no_default_paths: options.no_default_lib_paths,
        };
        let mut resolver = ImportResolver::new(
            &fs,
            &mut source_map,
            options.root_dir.clone(),
            options.max_import_depth,
            options.allow_imports,
            library_config,
        );
        let import_diags = resolver.resolve(&mut doc, &options.root_dir.join("<input>"), 0);
        all_diagnostics.extend(import_diags.into_diagnostics());
    }

    // Phase 3a: Resolve import_table() expressions into inline tables
    {
        let mut diag_bag = DiagnosticBag::new();
        crate::eval::resolve_import_tables(
            &mut doc,
            &RealFileSystem,
            &options.root_dir,
            &mut diag_bag,
        );
        all_diagnostics.extend(diag_bag.into_diagnostics());
    }

    // Macro expansion
    let mut expander = MacroExpander::new(&macro_registry, options.max_macro_depth);
    expander.expand(&mut doc);
    all_diagnostics.extend(expander.into_diagnostics().into_diagnostics());

    // Control flow expansion
    let mut cf_expander = ControlFlowExpander::new(options.max_loop_depth, options.max_iterations);
    let pre_eval =
        std::cell::RefCell::new(Evaluator::with_functions(&options.functions, None, None));
    let pre_scope = pre_eval
        .borrow_mut()
        .scopes_mut()
        .create_scope(ScopeKind::Module, None);
    {
        let mut eval = pre_eval.borrow_mut();
        for item in &doc.items {
            if let crate::lang::ast::DocItem::Body(crate::lang::ast::BodyItem::LetBinding(lb)) =
                item
            {
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
    // Pre-register inline tables for control flow
    {
        let mut eval = pre_eval.borrow_mut();
        for item in &doc.items {
            if let crate::lang::ast::DocItem::Body(crate::lang::ast::BodyItem::Table(table)) = item
            {
                let name = table.inline_id.as_ref().and_then(|id| match id {
                    crate::lang::ast::InlineId::Literal(lit) => Some(lit.value.clone()),
                    _ => None,
                });
                if let Some(name) = name {
                    if table.import_expr.is_none() {
                        let col_names: Vec<String> =
                            table.columns.iter().map(|c| c.name.name.clone()).collect();
                        let mut rows = Vec::new();
                        for row in &table.rows {
                            let mut map = indexmap::IndexMap::new();
                            for (i, col_name) in col_names.iter().enumerate() {
                                if i < row.cells.len() {
                                    if let Ok(val) = eval.eval_expr(&row.cells[i], pre_scope) {
                                        map.insert(col_name.clone(), val);
                                    } else {
                                        map.insert(col_name.clone(), crate::eval::Value::Null);
                                    }
                                }
                            }
                            rows.push(crate::eval::Value::Map(map));
                        }
                        eval.scopes_mut().add_entry(
                            pre_scope,
                            ScopeEntry {
                                name,
                                kind: ScopeEntryKind::TableEntry,
                                value: Some(crate::eval::Value::List(rows)),
                                span: table.span,
                                dependencies: std::collections::HashSet::new(),
                                evaluated: true,
                                read_count: 0,
                            },
                        );
                    }
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
    let mut symbol_sets = crate::schema::SymbolSetRegistry::new();
    symbol_sets.collect(&doc, &mut diag_bag);
    schemas.validate(&doc, &values, &symbol_sets, &mut diag_bag);
    all_diagnostics.extend(diag_bag.into_diagnostics());

    // Table validation
    let mut diag_bag = DiagnosticBag::new();
    crate::schema::table::validate_tables(&doc, &mut diag_bag);
    all_diagnostics.extend(diag_bag.into_diagnostics());

    // ID uniqueness
    let mut id_registry = IdRegistry::new();
    let mut diag_bag = DiagnosticBag::new();
    id_registry.check_document(&doc, &mut diag_bag);
    all_diagnostics.extend(diag_bag.into_diagnostics());

    // Document validation
    let mut diag_bag = DiagnosticBag::new();
    crate::schema::document::validate_document(
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
        if let crate::lang::ast::DocItem::FunctionDecl(decl) = item {
            function_signatures.push(crate::eval::FunctionSignature {
                name: decl.name.name.clone(),
                params: decl
                    .params
                    .iter()
                    .map(|p| format!("{}: {}", p.name.name, type_expr_to_string(&p.type_expr)))
                    .collect(),
                return_type: decl
                    .return_type
                    .as_ref()
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

fn type_expr_to_string(te: &crate::lang::ast::TypeExpr) -> String {
    match te {
        crate::lang::ast::TypeExpr::String(_) => "string".into(),
        crate::lang::ast::TypeExpr::I8(_) => "i8".into(),
        crate::lang::ast::TypeExpr::U8(_) => "u8".into(),
        crate::lang::ast::TypeExpr::I16(_) => "i16".into(),
        crate::lang::ast::TypeExpr::U16(_) => "u16".into(),
        crate::lang::ast::TypeExpr::I32(_) => "i32".into(),
        crate::lang::ast::TypeExpr::U32(_) => "u32".into(),
        crate::lang::ast::TypeExpr::I64(_) => "i64".into(),
        crate::lang::ast::TypeExpr::U64(_) => "u64".into(),
        crate::lang::ast::TypeExpr::I128(_) => "i128".into(),
        crate::lang::ast::TypeExpr::U128(_) => "u128".into(),
        crate::lang::ast::TypeExpr::F32(_) => "f32".into(),
        crate::lang::ast::TypeExpr::F64(_) => "f64".into(),
        crate::lang::ast::TypeExpr::Date(_) => "date".into(),
        crate::lang::ast::TypeExpr::Duration(_) => "duration".into(),
        crate::lang::ast::TypeExpr::Bool(_) => "bool".into(),
        crate::lang::ast::TypeExpr::Null(_) => "null".into(),
        crate::lang::ast::TypeExpr::Identifier(_) => "identifier".into(),
        crate::lang::ast::TypeExpr::Any(_) => "any".into(),
        crate::lang::ast::TypeExpr::List(inner, _) => {
            format!("list({})", type_expr_to_string(inner))
        }
        crate::lang::ast::TypeExpr::Map(k, v, _) => format!(
            "map({}, {})",
            type_expr_to_string(k),
            type_expr_to_string(v)
        ),
        crate::lang::ast::TypeExpr::Set(inner, _) => format!("set({})", type_expr_to_string(inner)),
        crate::lang::ast::TypeExpr::Ref(_, _) => "ref".into(),
        crate::lang::ast::TypeExpr::Union(types, _) => {
            let parts: Vec<String> = types.iter().map(type_expr_to_string).collect();
            format!("union({})", parts.join(", "))
        }
        crate::lang::ast::TypeExpr::Symbol(_) => "symbol".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_simple() {
        let result = analyze("config { port = 8080 }", &crate::ParseOptions::default());
        assert!(!result.ast.items.is_empty());
        assert!(!result.tokens.is_empty());
        assert!(
            result.diagnostics.iter().all(|d| !d.is_error()),
            "unexpected errors: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn test_analyze_with_error() {
        let result = analyze("config { port = }", &crate::ParseOptions::default());
        assert!(result.diagnostics.iter().any(|d| d.is_error()));
    }

    #[test]
    fn test_analyze_retains_scopes() {
        let result = analyze(
            "let x = 42\nconfig { port = x }",
            &crate::ParseOptions::default(),
        );
        // Scopes should have at least one entry
        assert!(result.scopes.all_entries().count() > 0);
    }

    #[test]
    fn test_analyze_retains_tokens() {
        let result = analyze("let x = 42", &crate::ParseOptions::default());
        assert!(!result.tokens.is_empty());
    }

    #[test]
    fn test_analyze_retains_values() {
        let result = analyze("port = 8080", &crate::ParseOptions::default());
        assert!(result.values.contains_key("port"));
    }

    #[test]
    fn test_analyze_lex_error_returns_partial() {
        // Source with an unterminated string should not panic
        let result = analyze("name = \"unterminated", &crate::ParseOptions::default());
        assert!(result.diagnostics.iter().any(|d| d.is_error()));
    }
}
