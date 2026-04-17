use wcl_lang::eval::{
    builtin_signatures, ControlFlowExpander, Evaluator, ImportResolver, MacroExpander,
    MacroRegistry, PartialMerger, RealFileSystem, ScopeEntry, ScopeEntryKind, ScopeKind,
};
use wcl_lang::lang::diagnostic::DiagnosticBag;
use wcl_lang::lang::span::SourceMap;
use wcl_lang::schema::{DecoratorSchemaRegistry, IdRegistry, SchemaRegistry};

use crate::state::AnalysisResult;

/// Run the full WCL pipeline, retaining intermediate products for LSP features.
/// This mirrors `crate::parse()` but keeps tokens, scopes, and macro registry.
pub fn analyze(source: &str, options: &wcl_lang::ParseOptions) -> AnalysisResult {
    let mut source_map = SourceMap::new();
    let file_id = source_map.add_file("<input>".to_string(), source.to_string());
    let mut all_diagnostics = Vec::new();

    // Lex (retain tokens for semantic tokens)
    let tokens = match wcl_lang::lang::lexer::lex(source, file_id) {
        Ok(tokens) => tokens,
        Err(lex_errors) => {
            all_diagnostics.extend(lex_errors);
            return AnalysisResult {
                ast: wcl_lang::lang::ast::Document {
                    items: Vec::new(),
                    trivia: wcl_lang::lang::Trivia::empty(),
                    span: wcl_lang::lang::Span::new(file_id, 0, source.len()),
                },
                tokens: Vec::new(),
                source_map,
                file_id,
                diagnostics: all_diagnostics,
                values: indexmap::IndexMap::new(),
                scopes: wcl_lang::eval::ScopeArena::new(),
                schemas: SchemaRegistry::new(),
                macro_registry: MacroRegistry::new(),
                function_signatures: builtin_signatures(),
            };
        }
    };

    // Parse (reclaim tokens afterwards for semantic tokens, avoiding a clone).
    let parser = wcl_lang::lang::parser::Parser::new(tokens);
    let (mut doc, parse_diags, tokens) = parser.parse_document();
    all_diagnostics.extend(parse_diags.into_diagnostics());

    // Macro collection
    let mut macro_registry = MacroRegistry::new();
    let mut diag_bag = DiagnosticBag::new();
    macro_registry.collect(&mut doc, &mut diag_bag);
    all_diagnostics.extend(diag_bag.into_diagnostics());

    // Import resolution
    if options.allow_imports {
        let fs = RealFileSystem;
        let library_config = wcl_lang::eval::LibraryConfig {
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
        wcl_lang::eval::resolve_import_tables(
            &mut doc,
            &RealFileSystem,
            &options.root_dir,
            &mut diag_bag,
        );
        all_diagnostics.extend(diag_bag.into_diagnostics());
    }

    // Phase 3b: Namespace resolution
    let namespace_aliases = {
        let mut diag_bag = DiagnosticBag::new();
        let aliases = wcl_lang::eval::namespaces::resolve(&mut doc, &mut diag_bag);
        all_diagnostics.extend(diag_bag.into_diagnostics());
        aliases
    };

    // Macro expansion
    let mut expander = MacroExpander::new(&macro_registry, options.max_macro_depth);
    expander.expand(&mut doc);
    all_diagnostics.extend(expander.into_diagnostics().into_diagnostics());

    // Control flow expansion (tolerant — defers block-query iterables
    // to the retry pass after evaluation, matching wcl_lang::parse).
    let mut cf_expander = ControlFlowExpander::new(options.max_loop_depth, options.max_iterations)
        .with_tolerate_missing(true);
    let pre_eval =
        std::cell::RefCell::new(Evaluator::with_functions(&options.functions, None, None));
    let pre_scope = pre_eval
        .borrow_mut()
        .scopes_mut()
        .create_scope(ScopeKind::Module, None);
    {
        let mut eval = pre_eval.borrow_mut();
        for item in &doc.items {
            if let wcl_lang::lang::ast::DocItem::Body(wcl_lang::lang::ast::BodyItem::LetBinding(
                lb,
            )) = item
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
            if let wcl_lang::lang::ast::DocItem::Body(wcl_lang::lang::ast::BodyItem::Table(table)) =
                item
            {
                let name = table.inline_id.as_ref().and_then(|id| match id {
                    wcl_lang::lang::ast::InlineId::Literal(lit) => Some(lit.value.clone()),
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
                                        map.insert(col_name.clone(), wcl_lang::eval::Value::Null);
                                    }
                                }
                            }
                            rows.push(wcl_lang::eval::Value::Map(map));
                        }
                        eval.scopes_mut().add_entry(
                            pre_scope,
                            ScopeEntry {
                                name,
                                kind: ScopeEntryKind::TableEntry,
                                value: Some(wcl_lang::eval::Value::List(rows)),
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

    // Phase 6a: Auto-id assignment. Must mirror the main pipeline in
    // `wcl_lang::parse` — otherwise anonymous sibling blocks collide, the
    // second sibling is silently dropped, and any interpolation dependency
    // it carries gets mis-reported as an unused variable.
    wcl_lang::eval::auto_id::assign_auto_ids(&mut doc, &namespace_aliases);

    // Scope construction + evaluation (retain scopes)
    let build_evaluator = || {
        Evaluator::with_functions(
            &options.functions,
            Some(Box::new(RealFileSystem)),
            Some(options.root_dir.clone()),
        )
    };
    let mut evaluator = build_evaluator();
    let mut values = evaluator.evaluate(&doc);

    // Phase 7a retry — mirrors wcl_lang::parse. If any ForLoops were
    // deferred (block-query iterables), re-expand with the real
    // evaluator and re-run phases 6, 6a, 7 on the mutated document.
    let (scopes, eval_diags) = if wcl_lang::eval::control_flow::has_remaining_for_loops(&doc) {
        let module_scope = evaluator
            .module_scope_id()
            .expect("evaluator.evaluate() sets module_scope_id");
        let eval_cell = std::cell::RefCell::new(evaluator);
        let mut retry_expander =
            ControlFlowExpander::new(options.max_loop_depth, options.max_iterations);
        retry_expander.expand(&mut doc, &|expr| {
            eval_cell
                .borrow_mut()
                .eval_expr(expr, module_scope)
                .map_err(|d| d.message)
        });
        all_diagnostics.extend(retry_expander.into_diagnostics().into_diagnostics());
        let (_, retry_eval_diags) = eval_cell.into_inner().into_parts();
        all_diagnostics.extend(retry_eval_diags.into_diagnostics());

        let mut merger2 = PartialMerger::new(options.merge_conflict_mode);
        merger2.merge(&mut doc);
        all_diagnostics.extend(merger2.into_diagnostics().into_diagnostics());
        wcl_lang::eval::auto_id::assign_auto_ids(&mut doc, &namespace_aliases);

        let mut new_evaluator = build_evaluator();
        values = new_evaluator.evaluate(&doc);
        new_evaluator.into_parts()
    } else {
        evaluator.into_parts()
    };
    all_diagnostics.extend(eval_diags.into_diagnostics());

    // Decorator validation
    let mut decorator_schemas = DecoratorSchemaRegistry::new();
    let mut diag_bag = DiagnosticBag::new();
    decorator_schemas.collect(&doc, &mut diag_bag);
    decorator_schemas.validate_all(&doc, &mut diag_bag);
    all_diagnostics.extend(diag_bag.into_diagnostics());

    // Schema validation
    let mut schemas = SchemaRegistry::new();
    schemas.namespace_aliases = namespace_aliases.aliases;
    let mut diag_bag = DiagnosticBag::new();
    schemas.collect(&doc, &mut diag_bag);
    let mut symbol_sets = wcl_lang::schema::SymbolSetRegistry::new();
    symbol_sets.collect(&doc, &mut diag_bag);
    schemas.validate(&doc, &values, &symbol_sets, &mut diag_bag);
    all_diagnostics.extend(diag_bag.into_diagnostics());

    // Table validation
    let mut diag_bag = DiagnosticBag::new();
    wcl_lang::schema::table::validate_tables(&doc, &mut diag_bag);
    all_diagnostics.extend(diag_bag.into_diagnostics());

    // ID uniqueness
    let mut id_registry = IdRegistry::new();
    let mut diag_bag = DiagnosticBag::new();
    id_registry.check_document(&doc, &mut diag_bag);
    all_diagnostics.extend(diag_bag.into_diagnostics());

    // Document validation
    let mut diag_bag = DiagnosticBag::new();
    wcl_lang::schema::document::validate_document(
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
        if let wcl_lang::lang::ast::DocItem::FunctionDecl(decl) = item {
            function_signatures.push(wcl_lang::eval::FunctionSignature {
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

fn type_expr_to_string(te: &wcl_lang::lang::ast::TypeExpr) -> String {
    match te {
        wcl_lang::lang::ast::TypeExpr::String(_) => "string".into(),
        wcl_lang::lang::ast::TypeExpr::I8(_) => "i8".into(),
        wcl_lang::lang::ast::TypeExpr::U8(_) => "u8".into(),
        wcl_lang::lang::ast::TypeExpr::I16(_) => "i16".into(),
        wcl_lang::lang::ast::TypeExpr::U16(_) => "u16".into(),
        wcl_lang::lang::ast::TypeExpr::I32(_) => "i32".into(),
        wcl_lang::lang::ast::TypeExpr::U32(_) => "u32".into(),
        wcl_lang::lang::ast::TypeExpr::I64(_) => "i64".into(),
        wcl_lang::lang::ast::TypeExpr::U64(_) => "u64".into(),
        wcl_lang::lang::ast::TypeExpr::I128(_) => "i128".into(),
        wcl_lang::lang::ast::TypeExpr::U128(_) => "u128".into(),
        wcl_lang::lang::ast::TypeExpr::F32(_) => "f32".into(),
        wcl_lang::lang::ast::TypeExpr::F64(_) => "f64".into(),
        wcl_lang::lang::ast::TypeExpr::Date(_) => "date".into(),
        wcl_lang::lang::ast::TypeExpr::Duration(_) => "duration".into(),
        wcl_lang::lang::ast::TypeExpr::Bool(_) => "bool".into(),
        wcl_lang::lang::ast::TypeExpr::Null(_) => "null".into(),
        wcl_lang::lang::ast::TypeExpr::Identifier(_) => "identifier".into(),
        wcl_lang::lang::ast::TypeExpr::Any(_) => "any".into(),
        wcl_lang::lang::ast::TypeExpr::List(inner, _) => {
            format!("list({})", type_expr_to_string(inner))
        }
        wcl_lang::lang::ast::TypeExpr::Map(k, v, _) => format!(
            "map({}, {})",
            type_expr_to_string(k),
            type_expr_to_string(v)
        ),
        wcl_lang::lang::ast::TypeExpr::Set(inner, _) => {
            format!("set({})", type_expr_to_string(inner))
        }
        wcl_lang::lang::ast::TypeExpr::Ref(_, _) => "ref".into(),
        wcl_lang::lang::ast::TypeExpr::Union(types, _) => {
            let parts: Vec<String> = types.iter().map(type_expr_to_string).collect();
            format!("union({})", parts.join(", "))
        }
        wcl_lang::lang::ast::TypeExpr::Symbol(_) => "symbol".into(),
        wcl_lang::lang::ast::TypeExpr::StructType(ident, _) => ident.name.clone(),
        wcl_lang::lang::ast::TypeExpr::Pattern(_) => "pattern".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_simple() {
        let result = analyze("config { port = 8080 }", &wcl_lang::ParseOptions::default());
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
        let result = analyze("config { port = }", &wcl_lang::ParseOptions::default());
        assert!(result.diagnostics.iter().any(|d| d.is_error()));
    }

    #[test]
    fn test_analyze_retains_scopes() {
        let result = analyze(
            "let x = 42\nconfig { port = x }",
            &wcl_lang::ParseOptions::default(),
        );
        // Scopes should have at least one entry
        assert!(result.scopes.all_entries().count() > 0);
    }

    #[test]
    fn test_analyze_retains_tokens() {
        let result = analyze("let x = 42", &wcl_lang::ParseOptions::default());
        assert!(!result.tokens.is_empty());
    }

    #[test]
    fn test_analyze_retains_values() {
        let result = analyze("port = 8080", &wcl_lang::ParseOptions::default());
        assert!(result.values.contains_key("port"));
    }

    #[test]
    fn test_analyze_lex_error_returns_partial() {
        // Source with an unterminated string should not panic
        let result = analyze("name = \"unterminated", &wcl_lang::ParseOptions::default());
        assert!(result.diagnostics.iter().any(|d| d.is_error()));
    }
}
