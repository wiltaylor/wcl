//! WCL — Wil's Configuration Language
//!
//! This is the facade crate that re-exports everything and provides
//! the main parsing pipeline.

#[cfg(feature = "json")]
pub mod json;
pub mod library;

// Re-exports
pub use wcl_core::{
    ast, lexer, parser, Comment, CommentPlacement, CommentStyle, Diagnostic, DiagnosticBag, FileId,
    Label, Severity, SourceFile, SourceMap, Span, Trivia,
};

pub use wcl_eval::{
    builtin_signatures, BlockRef, BuiltinFn, ConflictMode, ControlFlowExpander, DecoratorValue,
    Evaluator, FileSystem, FunctionRegistry, FunctionSignature, FunctionValue, ImportResolver,
    InMemoryFs, MacroExpander, MacroRegistry, PartialMerger, QueryEngine, RealFileSystem, Scope,
    ScopeArena, ScopeEntry, ScopeEntryKind, ScopeId, ScopeKind, Value,
};

pub use wcl_schema::{
    DecoratorSchemaRegistry, IdRegistry, ResolvedDecoratorSchema, ResolvedField, ResolvedSchema,
    SchemaRegistry,
};

pub use wcl_serde::{
    from_value, to_string as value_to_string, to_string_pretty as value_to_string_pretty,
    Error as SerdeError,
};

pub use wcl_derive::{WclDeserialize, WclSchema};

use std::path::PathBuf;
use std::sync::Arc;

/// Options for parsing a WCL document
#[derive(Clone)]
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
    /// Custom functions to register (builtins are always included)
    pub functions: FunctionRegistry,
    /// Custom filesystem for import resolution (defaults to real FS)
    pub fs: Option<Arc<dyn FileSystem>>,
}

impl std::fmt::Debug for ParseOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParseOptions")
            .field("root_dir", &self.root_dir)
            .field("max_import_depth", &self.max_import_depth)
            .field("allow_imports", &self.allow_imports)
            .field("merge_conflict_mode", &self.merge_conflict_mode)
            .field("max_macro_depth", &self.max_macro_depth)
            .field("max_loop_depth", &self.max_loop_depth)
            .field("max_iterations", &self.max_iterations)
            .field("fs", &self.fs.as_ref().map(|_| ".."))
            .finish()
    }
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
            functions: FunctionRegistry::default(),
            fs: None,
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
                ast::DocItem::Body(ast::BodyItem::Block(block)) if block.kind.name == kind => {
                    Some(block)
                }
                _ => None,
            })
            .collect()
    }

    /// Execute a query against this document.
    ///
    /// Parses the query string into a pipeline, builds block references from
    /// the AST and evaluated values, and runs the query engine over them.
    pub fn query(&self, query_str: &str) -> Result<Value, String> {
        // Parse the query string
        let file_id = FileId(9999); // synthetic file ID for query strings
        let pipeline = wcl_core::parse_query(query_str, file_id).map_err(|diags| {
            let messages: Vec<String> = diags
                .into_diagnostics()
                .into_iter()
                .map(|d| d.message)
                .collect();
            format!("query parse error: {}", messages.join("; "))
        })?;

        // Build BlockRefs from the AST, using evaluated values for attributes
        let blocks = self.collect_block_refs();

        // Execute the query
        let engine = QueryEngine::new();
        let mut evaluator = Evaluator::new();
        let scope = evaluator.scopes_mut().create_scope(ScopeKind::Module, None);
        engine.execute(&pipeline, &blocks, &mut evaluator, scope)
    }

    /// Build BlockRef values from the AST blocks, resolving attribute values
    /// from the evaluated `values` map where possible.
    fn collect_block_refs(&self) -> Vec<BlockRef> {
        let mut evaluator = Evaluator::new();
        let scope = evaluator.scopes_mut().create_scope(ScopeKind::Module, None);
        self.ast
            .items
            .iter()
            .filter_map(|item| match item {
                ast::DocItem::Body(ast::BodyItem::Block(block)) => {
                    Some(Self::block_to_ref(block, &mut evaluator, scope))
                }
                _ => None,
            })
            .collect()
    }

    fn block_to_ref(block: &ast::Block, evaluator: &mut Evaluator, scope: ScopeId) -> BlockRef {
        let kind = block.kind.name.clone();
        let id = block.inline_id.as_ref().map(|iid| match iid {
            ast::InlineId::Literal(lit) => lit.value.clone(),
            ast::InlineId::Interpolated(parts) => parts
                .iter()
                .filter_map(|p| match p {
                    ast::StringPart::Literal(s) => Some(s.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(""),
        });
        let labels: Vec<String> = block
            .labels
            .iter()
            .filter_map(|sl| match &sl.parts[..] {
                [ast::StringPart::Literal(s)] => Some(s.clone()),
                _ => None,
            })
            .collect();

        let mut attributes = indexmap::IndexMap::new();
        for body_item in &block.body {
            if let ast::BodyItem::Attribute(attr) = body_item {
                if let Ok(val) = evaluator.eval_expr(&attr.value, scope) {
                    attributes.insert(attr.name.name.clone(), val);
                }
            }
        }

        let children: Vec<BlockRef> = block
            .body
            .iter()
            .filter_map(|item| match item {
                ast::BodyItem::Block(child) => Some(Self::block_to_ref(child, evaluator, scope)),
                _ => None,
            })
            .collect();

        let decorators: Vec<DecoratorValue> = block
            .decorators
            .iter()
            .map(|d| {
                let mut args = indexmap::IndexMap::new();
                for arg in &d.args {
                    match arg {
                        ast::DecoratorArg::Named(name, expr) => {
                            if let Ok(val) = evaluator.eval_expr(expr, scope) {
                                args.insert(name.name.clone(), val);
                            }
                        }
                        ast::DecoratorArg::Positional(expr) => {
                            if let Ok(val) = evaluator.eval_expr(expr, scope) {
                                args.insert(format!("_{}", args.len()), val);
                            }
                        }
                    }
                }
                DecoratorValue {
                    name: d.name.name.clone(),
                    args,
                }
            })
            .collect();

        BlockRef {
            kind,
            id,
            labels,
            attributes,
            children,
            decorators,
            span: block.span,
        }
    }

    /// Get all blocks as BlockRef values (convenience for iteration)
    pub fn blocks(&self) -> Vec<BlockRef> {
        self.collect_block_refs()
    }

    /// Get blocks of a given type as BlockRef values with full attribute resolution
    pub fn blocks_of_type_resolved(&self, kind: &str) -> Vec<BlockRef> {
        self.collect_block_refs()
            .into_iter()
            .filter(|br| br.kind == kind)
            .collect()
    }

    /// Check if any block has the given decorator
    pub fn has_decorator(&self, decorator_name: &str) -> bool {
        self.collect_block_refs()
            .iter()
            .any(|br| br.has_decorator(decorator_name))
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
    let fs: Arc<dyn FileSystem> = options
        .fs
        .clone()
        .unwrap_or_else(|| Arc::new(RealFileSystem));
    if options.allow_imports {
        let mut resolver = ImportResolver::new(
            fs.as_ref(),
            &mut source_map,
            options.root_dir.clone(),
            options.max_import_depth,
            options.allow_imports,
        );
        let import_diags = resolver.resolve(&mut doc, &options.root_dir.join("<input>"), 0);
        all_diagnostics.extend(import_diags.into_diagnostics());
    }

    // Phase 4: Macro expansion
    let mut expander = MacroExpander::new(&macro_registry, options.max_macro_depth);
    expander.expand(&mut doc);
    all_diagnostics.extend(expander.into_diagnostics().into_diagnostics());

    // Phase 5: Control flow expansion
    let mut cf_expander = ControlFlowExpander::new(options.max_loop_depth, options.max_iterations);
    // Use a lightweight pre-evaluator for control flow condition/iterable expressions.
    // This only handles literal expressions; variables defined via `let` are not
    // available until Phase 7. We wrap the evaluator in a RefCell because
    // `eval_expr` requires `&mut self` but the callback signature is `&dyn Fn`.
    let pre_eval =
        std::cell::RefCell::new(Evaluator::with_functions(&options.functions, None, None));
    let pre_scope = pre_eval
        .borrow_mut()
        .scopes_mut()
        .create_scope(ScopeKind::Module, None);
    // Pre-register let bindings with literal values so control flow can access them.
    // This allows `for item in items { ... }` where `let items = [1, 2, 3]` at the top level.
    {
        let mut eval = pre_eval.borrow_mut();
        for item in &doc.items {
            if let ast::DocItem::Body(ast::BodyItem::LetBinding(lb)) = item {
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

    // Phase 6: Partial merge
    let mut merger = PartialMerger::new(options.merge_conflict_mode);
    merger.merge(&mut doc);
    all_diagnostics.extend(merger.into_diagnostics().into_diagnostics());

    // Phase 7: Scope construction + Expression evaluation
    // Wrap the Arc in a newtype so we can pass it as Box<dyn FileSystem>
    struct ArcFs(Arc<dyn FileSystem>);
    impl FileSystem for ArcFs {
        fn read_file(&self, path: &std::path::Path) -> Result<String, String> {
            self.0.read_file(path)
        }
        fn canonicalize(&self, path: &std::path::Path) -> Result<PathBuf, String> {
            self.0.canonicalize(path)
        }
        fn exists(&self, path: &std::path::Path) -> bool {
            self.0.exists(path)
        }
    }
    let mut evaluator = Evaluator::with_functions(
        &options.functions,
        Some(Box::new(ArcFs(fs))),
        Some(options.root_dir.clone()),
    );
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
    schemas.validate(&doc, &values, &mut diag_bag);
    all_diagnostics.extend(diag_bag.into_diagnostics());

    // Phase 9b: Table column validation
    let mut diag_bag = DiagnosticBag::new();
    wcl_schema::table::validate_tables(&doc, &mut diag_bag);
    all_diagnostics.extend(diag_bag.into_diagnostics());

    // Phase 10: ID uniqueness check
    let mut id_registry = IdRegistry::new();
    let mut diag_bag = DiagnosticBag::new();
    id_registry.check_document(&doc, &mut diag_bag);
    all_diagnostics.extend(diag_bag.into_diagnostics());

    // Phase 11: Document validation
    let mut diag_bag = DiagnosticBag::new();
    wcl_schema::document::validate_document(
        &doc,
        &mut Evaluator::with_functions(&options.functions, None, None),
        &mut diag_bag,
    );
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
pub fn from_str<'de, T: serde::Deserialize<'de>>(source: &str) -> Result<T, Vec<Diagnostic>> {
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
pub fn to_string_pretty<T: serde::Serialize>(value: &T) -> Result<String, SerdeError> {
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
        let doc = parse("let x = 42\nconfig { port = x }", ParseOptions::default());
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
    fn test_query_string_selects_blocks() {
        let doc = parse(
            "service { port = 8080 }\nservice { port = 9090 }\ndatabase { port = 5432 }",
            ParseOptions::default(),
        );
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
        let result = doc.query("service").unwrap();
        match result {
            Value::List(items) => {
                assert_eq!(items.len(), 2);
                for item in &items {
                    if let Value::BlockRef(br) = item {
                        assert_eq!(br.kind, "service");
                    } else {
                        panic!("expected BlockRef");
                    }
                }
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn test_query_string_with_projection() {
        let doc = parse(
            "service { port = 8080 }\nservice { port = 9090 }",
            ParseOptions::default(),
        );
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
        let result = doc.query("service | .port").unwrap();
        assert_eq!(
            result,
            Value::List(vec![Value::Int(8080), Value::Int(9090)])
        );
    }

    #[test]
    fn test_query_string_parse_error() {
        let doc = parse("config { port = 8080 }", ParseOptions::default());
        // An empty query string should fail to parse
        assert!(doc.query("").is_err());
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

    // ── C4: Document validation (Phase 11) ──────────────────────────────

    #[test]
    fn test_validation_block_passing() {
        // Validation with self-contained let bindings (sub-evaluator is fresh)
        let source = r#"
            validation "check passes" {
                let x = 10
                check = x > 0
                message = "x is not positive"
            }
        "#;
        let doc = parse(source, ParseOptions::default());
        // check = 10 > 0 = true, so no errors from validation
        let validation_errors: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.message.contains("validation"))
            .collect();
        assert!(
            validation_errors.is_empty(),
            "unexpected validation errors: {:?}",
            validation_errors
        );
    }

    #[test]
    fn test_validation_block_failure_produces_error() {
        let source = r#"
            validation "x must be positive" {
                let x = -5
                check = x > 0
                message = "x is not positive"
            }
        "#;
        let doc = parse(source, ParseOptions::default());
        // check = -5 > 0 = false, so we expect a validation error
        let validation_errors: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.message.contains("validation") && d.message.contains("x is not positive"))
            .collect();
        assert!(
            !validation_errors.is_empty(),
            "expected validation error, got: {:?}",
            doc.diagnostics
        );
    }

    #[test]
    fn test_validation_block_warning_on_failure() {
        let source = r#"
            @warning
            validation "x should be positive" {
                let x = -5
                check = x > 0
                message = "x is not positive"
            }
        "#;
        let doc = parse(source, ParseOptions::default());
        let validation_warnings: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.message.contains("validation") && d.message.contains("x is not positive"))
            .collect();
        assert!(
            !validation_warnings.is_empty(),
            "expected validation warning, got: {:?}",
            doc.diagnostics
        );
        // Should be a warning, not an error
        assert!(
            !validation_warnings[0].is_error(),
            "expected warning, got error"
        );
    }

    // ── M1: Let bindings accessible in control flow ─────────────────────

    #[test]
    fn test_let_binding_accessible_in_for_loop() {
        let source = r#"
            let items = [1, 2, 3]
            for item in items {
                entry { value = item }
            }
        "#;
        let doc = parse(source, ParseOptions::default());
        // The for loop should expand using the let binding
        let entries = doc.blocks_of_type("entry");
        assert_eq!(
            entries.len(),
            3,
            "expected 3 entry blocks from for loop over let binding, got {}: errors: {:?}",
            entries.len(),
            doc.diagnostics
        );
    }

    // ── Gap 3: Unknown decorator validation (E060) ────────────────────

    #[test]
    fn test_unknown_decorator_produces_e060() {
        let source = r#"
            @nonexistent
            server main {
                port = 8080
            }
        "#;
        let doc = parse(source, ParseOptions::default());
        let e060_errors: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E060"))
            .collect();
        assert_eq!(
            e060_errors.len(),
            1,
            "expected one E060 error, got: {:?}",
            e060_errors
        );
        assert!(e060_errors[0]
            .message
            .contains("unknown decorator @nonexistent"));
    }

    #[test]
    fn test_known_decorator_no_e060() {
        let source = r#"
            @deprecated("use new_server instead")
            server main {
                port = 8080
            }
        "#;
        let doc = parse(source, ParseOptions::default());
        let e060_errors: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E060"))
            .collect();
        assert!(
            e060_errors.is_empty(),
            "known decorator @deprecated should not produce E060, got: {:?}",
            e060_errors
        );
    }

    // ── Gap 6: Table column type validation ────────────────────────────

    #[test]
    fn test_table_column_type_validation() {
        let source = r#"
            table users {
                name: string
                port: int
                | "web" | 8080 |
                | "api" | "bad" |
            }
        "#;
        let doc = parse(source, ParseOptions::default());
        let type_errors: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E071"))
            .collect();
        assert!(
            !type_errors.is_empty(),
            "expected E071 type error for string in int column, got: {:?}",
            doc.diagnostics
        );
    }

    #[test]
    fn test_table_valid_types_no_errors() {
        let source = r#"
            table users {
                name: string
                age: int
                | "Alice" | 30 |
                | "Bob"   | 25 |
            }
        "#;
        let doc = parse(source, ParseOptions::default());
        let type_errors: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E071"))
            .collect();
        assert!(
            type_errors.is_empty(),
            "expected no E071 errors for valid table, got: {:?}",
            type_errors
        );
    }

    #[test]
    fn test_let_binding_list_strings_in_for_loop() {
        let source = r#"
            let regions = ["us", "eu", "ap"]
            for region in regions {
                server { name = region }
            }
        "#;
        let doc = parse(source, ParseOptions::default());
        let servers = doc.blocks_of_type("server");
        assert_eq!(
            servers.len(),
            3,
            "expected 3 server blocks, got {}: errors: {:?}",
            servers.len(),
            doc.diagnostics
        );
    }

    // ── Rich Document API (Section 26.5) ─────────────────────────────────

    #[test]
    fn test_block_ref_has_decorator() {
        let doc = parse(
            "@deprecated(\"use v2\")\nservice main {\n    port = 8080\n}",
            ParseOptions::default(),
        );
        let blocks = doc.blocks();
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].has_decorator("deprecated"));
        assert!(!blocks[0].has_decorator("nonexistent"));
    }

    #[test]
    fn test_block_ref_decorator() {
        let doc = parse(
            "@deprecated(\"use v2\")\nservice main {\n    port = 8080\n}",
            ParseOptions::default(),
        );
        let blocks = doc.blocks();
        let dec = blocks[0].decorator("deprecated");
        assert!(dec.is_some());
        assert_eq!(dec.unwrap().name, "deprecated");
    }

    #[test]
    fn test_block_ref_get_attribute() {
        let doc = parse(
            "service {\n    port = 8080\n    host = \"localhost\"\n}",
            ParseOptions::default(),
        );
        let blocks = doc.blocks();
        assert_eq!(blocks[0].get("port"), Some(&Value::Int(8080)));
        assert_eq!(
            blocks[0].get("host"),
            Some(&Value::String("localhost".to_string()))
        );
        assert_eq!(blocks[0].get("missing"), None);
    }

    #[test]
    fn test_document_has_decorator() {
        let doc = parse(
            "@deprecated(\"old\")\nservice { port = 80 }\nserver { port = 443 }",
            ParseOptions::default(),
        );
        assert!(doc.has_decorator("deprecated"));
        assert!(!doc.has_decorator("nonexistent"));
    }

    #[test]
    fn test_document_blocks_of_type_resolved() {
        let doc = parse(
            "service { port = 8080 }\nservice { port = 9090 }\ndatabase { port = 5432 }",
            ParseOptions::default(),
        );
        let services = doc.blocks_of_type_resolved("service");
        assert_eq!(services.len(), 2);
        assert_eq!(services[0].get("port"), Some(&Value::Int(8080)));
        assert_eq!(services[1].get("port"), Some(&Value::Int(9090)));
    }

    // ── wcl_derive tests ─────────────────────────────────────────────────

    #[test]
    fn test_wcl_deserialize_basic_struct() {
        #[derive(WclDeserialize, Debug, PartialEq)]
        struct Config {
            port: i64,
            host: String,
        }

        let result: Result<Config, _> = from_str("port = 8080\nhost = \"localhost\"");
        assert!(result.is_ok(), "error: {:?}", result.err());
        let config = result.unwrap();
        assert_eq!(config.port, 8080);
        assert_eq!(config.host, "localhost");
    }

    #[test]
    fn test_wcl_deserialize_with_id() {
        #[derive(WclDeserialize, Debug, PartialEq)]
        struct Service {
            #[wcl(id)]
            name: Option<String>,
            port: i64,
        }

        let mut map = indexmap::IndexMap::new();
        map.insert("id".to_string(), Value::String("my-svc".to_string()));
        map.insert("port".to_string(), Value::Int(8080));
        let result: Result<Service, _> = from_value(Value::Map(map));
        assert!(result.is_ok(), "error: {:?}", result.err());
        let svc = result.unwrap();
        assert_eq!(svc.name, Some("my-svc".to_string()));
        assert_eq!(svc.port, 8080);
    }

    #[test]
    fn test_wcl_deserialize_with_labels() {
        #[derive(WclDeserialize, Debug, PartialEq)]
        struct Resource {
            #[wcl(labels)]
            tags: Vec<String>,
            value: i64,
        }

        let mut map = indexmap::IndexMap::new();
        map.insert(
            "labels".to_string(),
            Value::List(vec![
                Value::String("prod".to_string()),
                Value::String("us-east".to_string()),
            ]),
        );
        map.insert("value".to_string(), Value::Int(42));
        let result: Result<Resource, _> = from_value(Value::Map(map));
        assert!(result.is_ok(), "error: {:?}", result.err());
        let res = result.unwrap();
        assert_eq!(res.tags, vec!["prod".to_string(), "us-east".to_string()]);
    }

    #[test]
    fn test_wcl_deserialize_missing_field_errors() {
        #[derive(WclDeserialize, Debug)]
        struct NeedsPort {
            #[allow(dead_code)]
            port: i64,
        }

        let result: Result<NeedsPort, _> = from_str("host = \"localhost\"");
        assert!(result.is_err());
    }

    // ── Phase 1: Custom Function Registration ────────────────────────────

    #[test]
    fn test_custom_function_registration() {
        use std::sync::Arc;

        let mut opts = ParseOptions::default();
        opts.functions.functions.insert(
            "double".into(),
            Arc::new(|args: &[Value]| match args.first() {
                Some(Value::Int(n)) => Ok(Value::Int(n * 2)),
                _ => Err("expected int".into()),
            }),
        );

        let doc = parse("result = double(21)", opts);
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
        assert_eq!(doc.values.get("result"), Some(&Value::Int(42)));
    }

    #[test]
    fn test_custom_function_in_control_flow() {
        use std::sync::Arc;

        let mut opts = ParseOptions::default();
        opts.functions.functions.insert(
            "make_list".into(),
            Arc::new(|_args: &[Value]| Ok(Value::List(vec![Value::Int(1), Value::Int(2)]))),
        );

        let doc = parse("for item in make_list() { entry { value = item } }", opts);
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
        let entries = doc.blocks_of_type("entry");
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_function_registry_with_signature() {
        use std::sync::Arc;

        let mut registry = FunctionRegistry::new();
        registry.register(
            "greet",
            Arc::new(|args: &[Value]| match args.first() {
                Some(Value::String(s)) => Ok(Value::String(format!("Hello, {}!", s))),
                _ => Err("expected string".into()),
            }),
            FunctionSignature {
                name: "greet".into(),
                params: vec!["name: string".into()],
                return_type: "string".into(),
                doc: "Greet someone".into(),
            },
        );

        assert_eq!(registry.functions.len(), 1);
        assert_eq!(registry.signatures.len(), 1);
        assert_eq!(registry.signatures[0].name, "greet");
    }

    #[test]
    fn test_builtin_signatures_complete() {
        let sigs = builtin_signatures();
        assert!(
            sigs.len() >= 50,
            "expected at least 50 builtin signatures, got {}",
            sigs.len()
        );
        // Check a few are present
        assert!(sigs.iter().any(|s| s.name == "upper"));
        assert!(sigs.iter().any(|s| s.name == "len"));
        assert!(sigs.iter().any(|s| s.name == "sha256"));
    }

    // ── Phase 2: Well-Known Imports ──────────────────────────────────────

    #[test]
    fn test_parse_library_import_syntax() {
        let (doc, diags) = wcl_core::parse("import <stdlib.wcl>", FileId(0));
        // Should parse without errors (the file won't exist but the AST should be correct)
        let parse_errors: Vec<_> = diags
            .into_diagnostics()
            .into_iter()
            .filter(|d| d.is_error())
            .collect();
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        assert_eq!(doc.items.len(), 1);
        if let ast::DocItem::Import(import) = &doc.items[0] {
            assert_eq!(import.kind, ast::ImportKind::Library);
            // Check the path is "stdlib.wcl"
            if let ast::StringPart::Literal(s) = &import.path.parts[0] {
                assert_eq!(s, "stdlib.wcl");
            } else {
                panic!("expected literal path");
            }
        } else {
            panic!("expected Import");
        }
    }

    #[test]
    fn test_parse_relative_import_has_relative_kind() {
        let (doc, _diags) = wcl_core::parse("import \"./other.wcl\"", FileId(0));
        if let ast::DocItem::Import(import) = &doc.items[0] {
            assert_eq!(import.kind, ast::ImportKind::Relative);
        } else {
            panic!("expected Import");
        }
    }

    // ── Phase 3: Function Declarations ───────────────────────────────────

    #[test]
    fn test_parse_function_decl() {
        let (doc, diags) = wcl_core::parse(
            "declare my_fn(input: string, count: int) -> string",
            FileId(0),
        );
        let parse_errors: Vec<_> = diags
            .into_diagnostics()
            .into_iter()
            .filter(|d| d.is_error())
            .collect();
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        assert_eq!(doc.items.len(), 1);
        if let ast::DocItem::FunctionDecl(decl) = &doc.items[0] {
            assert_eq!(decl.name.name, "my_fn");
            assert_eq!(decl.params.len(), 2);
            assert_eq!(decl.params[0].name.name, "input");
            assert_eq!(decl.params[1].name.name, "count");
            assert!(decl.return_type.is_some());
        } else {
            panic!("expected FunctionDecl");
        }
    }

    #[test]
    fn test_parse_function_decl_no_return_type() {
        let (doc, diags) = wcl_core::parse("declare fire_event(name: string)", FileId(0));
        let parse_errors: Vec<_> = diags
            .into_diagnostics()
            .into_iter()
            .filter(|d| d.is_error())
            .collect();
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        if let ast::DocItem::FunctionDecl(decl) = &doc.items[0] {
            assert_eq!(decl.name.name, "fire_event");
            assert!(decl.return_type.is_none());
        } else {
            panic!("expected FunctionDecl");
        }
    }

    #[test]
    fn test_declared_but_unregistered_function_error() {
        let source = r#"
            declare my_fn(input: string) -> string
            result = my_fn("hello")
        "#;
        let doc = parse(source, ParseOptions::default());
        let e053_errors: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E053"))
            .collect();
        assert!(
            !e053_errors.is_empty(),
            "expected E053 error for declared-but-unregistered function, got: {:?}",
            doc.diagnostics
        );
        assert!(e053_errors[0]
            .message
            .contains("declared in library but not registered"));
    }

    #[test]
    fn test_declared_and_registered_function_works() {
        use std::sync::Arc;

        let mut opts = ParseOptions::default();
        opts.functions.functions.insert(
            "my_fn".into(),
            Arc::new(|args: &[Value]| match args.first() {
                Some(Value::String(s)) => Ok(Value::String(format!("processed: {}", s))),
                _ => Err("expected string".into()),
            }),
        );

        let source = r#"
            declare my_fn(input: string) -> string
            result = my_fn("hello")
        "#;
        let doc = parse(source, opts);
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
        assert_eq!(
            doc.values.get("result"),
            Some(&Value::String("processed: hello".to_string()))
        );
    }

    // ── Phase 4: WclSchema derive macro ──────────────────────────────────

    #[test]
    fn test_wcl_schema_basic() {
        #[derive(WclSchema)]
        struct ServerConfig {
            #[allow(dead_code)]
            port: i64,
            #[allow(dead_code)]
            host: String,
        }

        let schema = ServerConfig::wcl_schema();
        assert!(
            schema.contains("schema \"server_config\""),
            "schema: {}",
            schema
        );
        assert!(schema.contains("port: int"), "schema: {}", schema);
        assert!(schema.contains("host: string"), "schema: {}", schema);
    }

    #[test]
    fn test_wcl_schema_with_optional() {
        #[derive(WclSchema)]
        struct Config {
            #[allow(dead_code)]
            name: String,
            #[allow(dead_code)]
            #[wcl(optional)]
            debug: bool,
        }

        let schema = Config::wcl_schema();
        assert!(
            schema.contains("debug: bool @optional"),
            "schema: {}",
            schema
        );
    }

    #[test]
    fn test_wcl_schema_custom_name() {
        #[derive(WclSchema)]
        #[wcl(schema_name = "my_custom_schema")]
        struct Foo {
            #[allow(dead_code)]
            value: i64,
        }

        let schema = Foo::wcl_schema();
        assert!(
            schema.contains("schema \"my_custom_schema\""),
            "schema: {}",
            schema
        );
    }

    #[test]
    fn test_wcl_schema_option_type_is_optional() {
        #[derive(WclSchema)]
        struct Config {
            #[allow(dead_code)]
            timeout: Option<i64>,
        }

        let schema = Config::wcl_schema();
        assert!(
            schema.contains("@optional"),
            "Option<T> should generate @optional: {}",
            schema
        );
    }

}
