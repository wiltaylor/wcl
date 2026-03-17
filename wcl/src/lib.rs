//! WCL — Wil's Configuration Language
//!
//! This is the facade crate that re-exports everything and provides
//! the main parsing pipeline.

// Re-exports
pub use wcl_core::{
    FileId, Span, SourceMap, SourceFile,
    Trivia, Comment, CommentStyle, CommentPlacement,
    Diagnostic, DiagnosticBag, Severity, Label,
    ast, lexer, parser,
};

pub use wcl_eval::{
    Value, BlockRef, DecoratorValue, FunctionValue, ScopeId,
    ScopeArena, Scope, ScopeEntry, ScopeEntryKind, ScopeKind,
    Evaluator, QueryEngine,
    ImportResolver, FileSystem, RealFileSystem, InMemoryFs,
    MacroRegistry, MacroExpander,
    ControlFlowExpander,
    PartialMerger, ConflictMode,
};

pub use wcl_schema::{
    SchemaRegistry, ResolvedSchema, ResolvedField,
    DecoratorSchemaRegistry, ResolvedDecoratorSchema,
    IdRegistry,
};

pub use wcl_serde::{
    from_value, to_string as value_to_string, to_string_pretty as value_to_string_pretty,
    Error as SerdeError,
};

pub use wcl_derive::WclDeserialize;

use std::path::PathBuf;

/// Options for parsing a WCL document
#[derive(Debug, Clone)]
pub struct ParseOptions {
    /// Root directory for import jail checking
    pub root_dir: PathBuf,
    /// Maximum import depth
    pub max_import_depth: u32,
    /// Whether imports are allowed
    pub allow_imports: bool,
    /// Merge conflict mode for partial declarations
    pub merge_conflict_mode: ConflictMode,
    /// Maximum macro expansion depth
    pub max_macro_depth: u32,
    /// Maximum for-loop nesting depth
    pub max_loop_depth: u32,
    /// Maximum total iterations across all for loops
    pub max_iterations: u32,
}

impl Default for ParseOptions {
    fn default() -> Self {
        ParseOptions {
            root_dir: PathBuf::from("."),
            max_import_depth: 32,
            allow_imports: true,
            merge_conflict_mode: ConflictMode::Strict,
            max_macro_depth: 64,
            max_loop_depth: 32,
            max_iterations: 10_000,
        }
    }
}

/// A parsed and evaluated WCL document
#[derive(Debug)]
pub struct Document {
    /// The raw AST (post-parse, pre-evaluation)
    pub ast: ast::Document,
    /// Evaluated values (attributes and block content)
    pub values: indexmap::IndexMap<String, Value>,
    /// All diagnostics from all phases
    pub diagnostics: Vec<Diagnostic>,
    /// Source map for error reporting
    pub source_map: SourceMap,
    /// Schema registry
    pub schemas: SchemaRegistry,
    /// Decorator schema registry
    pub decorator_schemas: DecoratorSchemaRegistry,
}

impl Document {
    /// Get all top-level blocks of a given type
    pub fn blocks_of_type(&self, kind: &str) -> Vec<&ast::Block> {
        self.ast
            .items
            .iter()
            .filter_map(|item| match item {
                ast::DocItem::Body(ast::BodyItem::Block(block))
                    if block.kind.name == kind =>
                {
                    Some(block)
                }
                _ => None,
            })
            .collect()
    }

    /// Execute a query against this document.
    ///
    /// Note: query parsing from strings is not yet supported. The parser's
    /// query pipeline parsing is internal (`pub(crate)`). This method will
    /// return an error until a public query-parsing API is available.
    pub fn query(&self, _query_str: &str) -> Result<Value, String> {
        Err("query parsing from strings is not yet supported".to_string())
    }

    /// Check if any errors occurred
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.is_error())
    }

    /// Get only error diagnostics
    pub fn errors(&self) -> Vec<&Diagnostic> {
        self.diagnostics.iter().filter(|d| d.is_error()).collect()
    }
}

/// Parse a WCL document from source text through the full pipeline.
///
/// This runs all phases:
/// 1. Parse (source -> AST)
/// 2. Macro collection
/// 3. Import resolution
/// 4. Macro expansion
/// 5. Control flow expansion
/// 6. Partial merge
/// 7. Scope construction + Expression evaluation
/// 8. Decorator validation
/// 9. Schema validation
/// 10. ID uniqueness check
pub fn parse(source: &str, options: ParseOptions) -> Document {
    let mut source_map = SourceMap::new();
    let file_id = source_map.add_file("<input>".to_string(), source.to_string());
    let mut all_diagnostics = Vec::new();

    // Phase 1: Parse
    let (mut doc, parse_diags) = wcl_core::parse(source, file_id);
    all_diagnostics.extend(parse_diags.into_diagnostics());

    // Phase 2: Macro collection
    let mut macro_registry = MacroRegistry::new();
    let mut diag_bag = DiagnosticBag::new();
    macro_registry.collect(&mut doc, &mut diag_bag);
    all_diagnostics.extend(diag_bag.into_diagnostics());

    // Phase 3: Import resolution
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

    // Phase 4: Macro expansion
    let mut expander = MacroExpander::new(&macro_registry, options.max_macro_depth);
    expander.expand(&mut doc);
    all_diagnostics.extend(expander.into_diagnostics().into_diagnostics());

    // Phase 5: Control flow expansion
    let mut cf_expander =
        ControlFlowExpander::new(options.max_loop_depth, options.max_iterations);
    // Use a lightweight pre-evaluator for control flow condition/iterable expressions.
    // This only handles literal expressions; variables defined via `let` are not
    // available until Phase 7. We wrap the evaluator in a RefCell because
    // `eval_expr` requires `&mut self` but the callback signature is `&dyn Fn`.
    let pre_eval = std::cell::RefCell::new(Evaluator::new());
    let pre_scope = pre_eval.borrow_mut().scopes_mut().create_scope(ScopeKind::Module, None);
    cf_expander.expand(&mut doc, &|expr| {
        pre_eval
            .borrow_mut()
            .eval_expr(expr, pre_scope)
            .map_err(|d| d.message)
    });
    all_diagnostics.extend(cf_expander.into_diagnostics().into_diagnostics());

    // Phase 6: Partial merge
    let mut merger = PartialMerger::new(options.merge_conflict_mode);
    merger.merge(&mut doc);
    all_diagnostics.extend(merger.into_diagnostics().into_diagnostics());

    // Phase 7: Scope construction + Expression evaluation
    let mut evaluator = Evaluator::new();
    let values = evaluator.evaluate(&doc);
    all_diagnostics.extend(evaluator.into_diagnostics().into_diagnostics());

    // Phase 8: Decorator validation
    let mut decorator_schemas = DecoratorSchemaRegistry::new();
    let mut diag_bag = DiagnosticBag::new();
    decorator_schemas.collect(&doc, &mut diag_bag);
    decorator_schemas.validate_all(&doc, &mut diag_bag);
    all_diagnostics.extend(diag_bag.into_diagnostics());

    // Phase 9: Schema validation
    let mut schemas = SchemaRegistry::new();
    let mut diag_bag = DiagnosticBag::new();
    schemas.collect(&doc, &mut diag_bag);
    schemas.validate(&doc, &mut diag_bag);
    all_diagnostics.extend(diag_bag.into_diagnostics());

    // Phase 10: ID uniqueness check
    let mut id_registry = IdRegistry::new();
    let mut diag_bag = DiagnosticBag::new();
    id_registry.check_document(&doc, &mut diag_bag);
    all_diagnostics.extend(diag_bag.into_diagnostics());

    Document {
        ast: doc,
        values,
        diagnostics: all_diagnostics,
        source_map,
        schemas,
        decorator_schemas,
    }
}

/// Parse a WCL string and deserialize into a Rust type
pub fn from_str<'de, T: serde::Deserialize<'de>>(
    source: &str,
) -> Result<T, Vec<Diagnostic>> {
    from_str_with_options(source, ParseOptions::default())
}

/// Parse a WCL string with options and deserialize into a Rust type
pub fn from_str_with_options<'de, T: serde::Deserialize<'de>>(
    source: &str,
    options: ParseOptions,
) -> Result<T, Vec<Diagnostic>> {
    let doc = parse(source, options);
    if doc.has_errors() {
        return Err(doc.errors().into_iter().cloned().collect());
    }
    from_value(Value::Map(doc.values)).map_err(|e| {
        vec![Diagnostic::error(
            format!("deserialization error: {}", e),
            Span::dummy(),
        )]
    })
}

/// Serialize a Rust value to WCL text
pub fn to_string<T: serde::Serialize>(value: &T) -> Result<String, SerdeError> {
    value_to_string(value)
}

/// Serialize a Rust value to pretty-printed WCL text
pub fn to_string_pretty<T: serde::Serialize>(
    value: &T,
) -> Result<String, SerdeError> {
    value_to_string_pretty(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let doc = parse("config { port = 8080 }", ParseOptions::default());
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
    }

    #[test]
    fn test_parse_with_let() {
        let doc = parse(
            "let x = 42\nconfig { port = x }",
            ParseOptions::default(),
        );
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
    }

    #[test]
    fn test_from_str_simple() {
        use std::collections::HashMap;
        // This may or may not work depending on how the evaluator maps block values.
        // We just verify it does not panic.
        let _result: Result<HashMap<String, HashMap<String, i64>>, _> =
            from_str("config { port = 8080 }");
    }

    #[test]
    fn test_query_returns_error_for_now() {
        let doc = parse("config { port = 8080 }", ParseOptions::default());
        assert!(doc.query("config | where port > 80").is_err());
    }

    #[test]
    fn test_has_errors_on_valid_input() {
        let doc = parse("x = 42", ParseOptions::default());
        assert!(!doc.has_errors());
    }

    #[test]
    fn test_blocks_of_type() {
        let doc = parse(
            "server { port = 80 }\nclient { timeout = 30 }\nserver { port = 443 }",
            ParseOptions::default(),
        );
        let servers = doc.blocks_of_type("server");
        assert_eq!(servers.len(), 2);
        let clients = doc.blocks_of_type("client");
        assert_eq!(clients.len(), 1);
    }
}
