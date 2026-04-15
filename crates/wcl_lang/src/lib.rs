//! WCL — Wil's Configuration Language
//!
//! Language library: parser, evaluator, schema validation, serde.
//! This crate contains the core language pipeline without CLI or LSP.
#![allow(clippy::result_large_err, clippy::large_enum_variant)]

pub mod eval;
pub mod fmt;
pub mod fmt_value;
pub mod lang;
pub mod schema;
pub mod serde_impl;
pub mod transform;

pub mod json;
pub mod library;

// Re-exports
pub use crate::lang::{
    ast, lexer, parser, Comment, CommentPlacement, CommentStyle, Diagnostic, DiagnosticBag, FileId,
    Label, Severity, SourceFile, SourceMap, Span, Trivia,
};

pub use crate::eval::{
    builtin_signatures, call_lambda, BlockRef, BuiltinFn, ConflictMode, ControlFlowExpander,
    DecoratorValue, Evaluator, FileSystem, FunctionRegistry, FunctionSignature, FunctionValue,
    ImportResolver, InMemoryFs, LibraryConfig, MacroExpander, MacroRegistry, PartialMerger,
    QueryEngine, RealFileSystem, Scope, ScopeArena, ScopeEntry, ScopeEntryKind, ScopeId, ScopeKind,
    Value,
};

pub use crate::schema::type_name;
pub use crate::schema::{
    ChildConstraint, DecoratorSchemaRegistry, IdRegistry, ResolvedDecoratorSchema, ResolvedField,
    ResolvedSchema, ResolvedVariant, SchemaRegistry, StructRegistry, SymbolSetInfo,
    SymbolSetRegistry, ValidateConstraints,
};

pub use crate::serde_impl::{
    from_value, to_string as value_to_string, to_string_compact as value_to_string_compact,
    to_string_pretty as value_to_string_pretty, Error as SerdeError,
};

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
    /// External variables injected before evaluation
    pub variables: indexmap::IndexMap<String, Value>,
    /// Extra library search paths (searched before defaults)
    pub lib_paths: Vec<PathBuf>,
    /// If true, skip the default XDG/system library search paths
    pub no_default_lib_paths: bool,
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
            .field("variables", &self.variables)
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
            variables: indexmap::IndexMap::new(),
            lib_paths: Vec::new(),
            no_default_lib_paths: false,
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
    /// Symbol set registry
    pub symbol_sets: SymbolSetRegistry,
    /// Struct type registry
    pub struct_registry: StructRegistry,
    /// Layout definition registry
    pub layout_registry: crate::schema::LayoutRegistry,
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

    /// Evaluate a standalone WCL expression against this document.
    ///
    /// The expression is parsed, then evaluated in a fresh module scope
    /// seeded with this document's top-level evaluated values (so it can
    /// reference blocks and let bindings by name).
    pub fn eval_expression(&self, src: &str) -> Result<Value, String> {
        let file_id = FileId(9998);
        let expr = crate::lang::parse_expression(src, file_id).map_err(|diags| {
            let messages: Vec<String> = diags
                .into_diagnostics()
                .into_iter()
                .map(|d| d.message)
                .collect();
            format!("expression parse error: {}", messages.join("; "))
        })?;

        let mut evaluator = Evaluator::new();
        let scope = evaluator.scopes_mut().create_scope(ScopeKind::Module, None);
        let span = Span::new(file_id, 0, src.len());
        for (name, value) in &self.values {
            evaluator.scopes_mut().add_entry(
                scope,
                ScopeEntry {
                    name: name.clone(),
                    kind: ScopeEntryKind::LetBinding,
                    value: Some(value.clone()),
                    span,
                    dependencies: Default::default(),
                    evaluated: true,
                    read_count: 0,
                },
            );
        }

        evaluator
            .eval_expr(&expr, scope)
            .map_err(|diag| format!("expression eval error: {}", diag.message))
    }

    /// Execute a query against this document.
    ///
    /// Parses the query string into a pipeline, builds block references from
    /// the AST and evaluated values, and runs the query engine over them.
    pub fn query(&self, query_str: &str) -> Result<Value, String> {
        // Parse the query string
        let file_id = FileId(9999); // synthetic file ID for query strings
        let pipeline = crate::lang::parse_query(query_str, file_id).map_err(|diags| {
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
                    Some(self.block_to_ref(block, &mut evaluator, scope, None))
                }
                ast::DocItem::Body(ast::BodyItem::Table(table)) => Some(self.table_to_ref(table)),
                _ => None,
            })
            .collect()
    }

    /// Convert an evaluated table into a pseudo-BlockRef.
    /// Each row becomes a `__row` child BlockRef with column values as attributes.
    fn table_to_ref(&self, table: &ast::Table) -> BlockRef {
        let name = table.inline_id.as_ref().map(|id| match id {
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

        let children = self.table_rows_to_refs(name.as_deref());
        BlockRef {
            kind: "table".to_string(),
            id: name,
            qualified_id: None,
            attributes: indexmap::IndexMap::new(),
            children,
            decorators: Vec::new(),
            span: table.span,
        }
    }

    /// Build row BlockRefs for a table nested inside a block.
    /// Looks up `self.values[block_id] -> BlockRef.attributes[table_name]`.
    fn nested_table_rows_to_refs(
        &self,
        block_id: Option<&str>,
        table_name: Option<&str>,
    ) -> Vec<BlockRef> {
        let Some(block_id) = block_id else {
            return Vec::new();
        };
        let Some(table_name) = table_name else {
            return Vec::new();
        };
        let Some(Value::BlockRef(br)) = self.values.get(block_id) else {
            return Vec::new();
        };
        let Some(Value::List(rows)) = br.attributes.get(table_name) else {
            return Vec::new();
        };
        rows.iter()
            .filter_map(|row| {
                if let Value::Map(m) = row {
                    Some(BlockRef {
                        kind: "__row".to_string(),
                        id: None,
                        qualified_id: None,
                        attributes: m.clone(),
                        children: Vec::new(),
                        decorators: Vec::new(),
                        span: Span::dummy(),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    /// Build row BlockRefs from evaluated table values looked up by name.
    fn table_rows_to_refs(&self, name: Option<&str>) -> Vec<BlockRef> {
        let Some(name) = name else { return Vec::new() };
        let Some(Value::List(rows)) = self.values.get(name) else {
            return Vec::new();
        };
        rows.iter()
            .filter_map(|row| {
                if let Value::Map(m) = row {
                    Some(BlockRef {
                        kind: "__row".to_string(),
                        id: None,
                        qualified_id: None,
                        attributes: m.clone(),
                        children: Vec::new(),
                        decorators: Vec::new(),
                        span: Span::dummy(),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    fn block_to_ref(
        &self,
        block: &ast::Block,
        evaluator: &mut Evaluator,
        scope: ScopeId,
        parent_qualified_id: Option<&str>,
    ) -> BlockRef {
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

        // Compute qualified ID from parent's qualified ID and this block's inline ID.
        let qualified_id = id.as_ref().map(|bare_id| match parent_qualified_id {
            Some(pqid) => format!("{}.{}", pqid, bare_id),
            None => bare_id.clone(),
        });
        let mut attributes = indexmap::IndexMap::new();

        let evaluated_args: Vec<Value> = block
            .inline_args
            .iter()
            .filter_map(|e| evaluator.eval_expr(e, scope).ok())
            .collect();
        if !evaluated_args.is_empty() {
            attributes.insert("_args".to_string(), Value::List(evaluated_args));
        }
        for body_item in &block.body {
            if let ast::BodyItem::Attribute(attr) = body_item {
                if let Ok(val) = evaluator.eval_expr(&attr.value, scope) {
                    attributes.insert(attr.name.name.clone(), val);
                }
            }
        }

        let mut children: Vec<BlockRef> = block
            .body
            .iter()
            .filter_map(|item| match item {
                ast::BodyItem::Block(child) => {
                    Some(self.block_to_ref(child, evaluator, scope, qualified_id.as_deref()))
                }
                _ => None,
            })
            .collect();

        // Include tables inside blocks as pseudo-BlockRef children.
        // Look up table values from the block's evaluated value.
        let block_id_str = id.clone();
        for body_item in &block.body {
            if let ast::BodyItem::Table(table) = body_item {
                let table_name = table.inline_id.as_ref().map(|tid| match tid {
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
                let row_children =
                    self.nested_table_rows_to_refs(block_id_str.as_deref(), table_name.as_deref());
                children.push(BlockRef {
                    kind: "table".to_string(),
                    id: table_name,
                    qualified_id: None,
                    attributes: indexmap::IndexMap::new(),
                    children: row_children,
                    decorators: Vec::new(),
                    span: table.span,
                });
            }
        }

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
            qualified_id,
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

    /// List names of exported functions.
    pub fn exported_function_names(&self) -> Vec<&str> {
        self.values
            .iter()
            .filter_map(|(name, val)| {
                if matches!(val, Value::Function(_)) {
                    Some(name.as_str())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Call an exported function by name with the given arguments.
    ///
    /// The function must have been defined via `export let name = params => body`
    /// in the WCL source.
    pub fn call_function(&self, name: &str, args: &[Value]) -> Result<Value, String> {
        let func_val = self
            .values
            .get(name)
            .ok_or_else(|| format!("exported function '{}' not found", name))?;

        let func = match func_val {
            Value::Function(f) => f,
            _ => {
                return Err(format!(
                    "'{}' is not a function, it's a {}",
                    name,
                    func_val.type_name()
                ))
            }
        };

        // Validate argument count
        if args.len() != func.params.len() {
            return Err(format!(
                "function '{}' expects {} argument(s), got {}",
                name,
                func.params.len(),
                args.len()
            ));
        }

        // Create a temporary evaluator and evaluate the function body
        let mut evaluator = crate::eval::evaluator::Evaluator::new();
        let scope_id = evaluator
            .scopes_mut()
            .create_scope(crate::eval::scope::ScopeKind::Lambda, None);

        // Bind parameters
        for (i, param_name) in func.params.iter().enumerate() {
            evaluator.scopes_mut().add_entry(
                scope_id,
                crate::eval::scope::ScopeEntry {
                    name: param_name.clone(),
                    kind: crate::eval::scope::ScopeEntryKind::LetBinding,
                    value: Some(args[i].clone()),
                    span: crate::lang::span::Span::dummy(),
                    dependencies: std::collections::HashSet::new(),
                    evaluated: true,
                    read_count: 0,
                },
            );
        }

        // Evaluate the function body
        match &func.body {
            crate::eval::value::FunctionBody::UserDefined(expr) => evaluator
                .eval_expr(expr, scope_id)
                .map_err(|d| d.message.clone()),
            crate::eval::value::FunctionBody::BlockExpr(lets, final_expr) => {
                // Evaluate let bindings first
                for (let_name, let_expr) in lets {
                    let val = evaluator
                        .eval_expr(let_expr, scope_id)
                        .map_err(|d| d.message.clone())?;
                    evaluator.scopes_mut().add_entry(
                        scope_id,
                        crate::eval::scope::ScopeEntry {
                            name: let_name.clone(),
                            kind: crate::eval::scope::ScopeEntryKind::LetBinding,
                            value: Some(val),
                            span: crate::lang::span::Span::dummy(),
                            dependencies: std::collections::HashSet::new(),
                            evaluated: true,
                            read_count: 0,
                        },
                    );
                }
                evaluator
                    .eval_expr(final_expr, scope_id)
                    .map_err(|d| d.message.clone())
            }
            crate::eval::value::FunctionBody::Builtin(name) => {
                Err(format!("cannot call builtin function '{}' directly", name))
            }
        }
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
    let (mut doc, parse_diags) = crate::lang::parse(source, file_id);
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
    let library_config = LibraryConfig {
        extra_paths: options.lib_paths.clone(),
        no_default_paths: options.no_default_lib_paths,
    };
    let mut imported_paths = std::collections::HashSet::new();
    if options.allow_imports {
        let mut resolver = ImportResolver::new(
            fs.as_ref(),
            &mut source_map,
            options.root_dir.clone(),
            options.max_import_depth,
            options.allow_imports,
            library_config,
        );
        let import_diags = resolver.resolve(&mut doc, &options.root_dir.join("<input>"), 0);
        all_diagnostics.extend(import_diags.into_diagnostics());

        // Phase 3-lazy: Resolve lazy imports whose namespaces are referenced
        let lazy_imports = resolver.take_lazy_imports();
        if !lazy_imports.is_empty() {
            let lazy_ns_strings: Vec<String> = lazy_imports
                .iter()
                .filter_map(|li| {
                    li.import
                        .lazy_namespace
                        .as_ref()
                        .map(|path| ast::join_path(path))
                })
                .collect();
            let referenced = crate::eval::find_lazy_namespace_references(&doc, &lazy_ns_strings);
            if !referenced.is_empty() {
                let lazy_diags = resolver.resolve_lazy(&mut doc, &lazy_imports, &referenced);
                all_diagnostics.extend(lazy_diags.into_diagnostics());
            }
        }

        imported_paths = resolver.loaded_paths().clone();
    }

    // Phase 3a: Resolve import_table() expressions into inline tables
    {
        let mut diag_bag = crate::lang::diagnostic::DiagnosticBag::new();
        crate::eval::resolve_import_tables(&mut doc, fs.as_ref(), &options.root_dir, &mut diag_bag);
        all_diagnostics.extend(diag_bag.into_diagnostics());
    }

    // Phase 3b: Namespace resolution — qualify names, flatten namespace wrappers
    let namespace_aliases = {
        let mut diag_bag = crate::lang::diagnostic::DiagnosticBag::new();
        let aliases = crate::eval::namespaces::resolve(&mut doc, &mut diag_bag);
        all_diagnostics.extend(diag_bag.into_diagnostics());
        aliases
    };

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
    // Pre-register inline tables so control flow can iterate over them.
    // import_table() tables were already resolved to inline in Phase 3a.
    {
        let mut eval = pre_eval.borrow_mut();
        for item in &doc.items {
            if let ast::DocItem::Body(ast::BodyItem::Table(table)) = item {
                let name = table.inline_id.as_ref().and_then(|id| match id {
                    ast::InlineId::Literal(lit) => Some(lit.value.clone()),
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
                                        map.insert(col_name.clone(), Value::Null);
                                    }
                                }
                            }
                            rows.push(Value::Map(map));
                        }
                        eval.scopes_mut().add_entry(
                            pre_scope,
                            ScopeEntry {
                                name,
                                kind: ScopeEntryKind::TableEntry,
                                value: Some(Value::List(rows)),
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
    // Inject external variables after let bindings so they override defaults.
    {
        let mut eval = pre_eval.borrow_mut();
        for (name, value) in &options.variables {
            eval.scopes_mut().add_entry(
                pre_scope,
                ScopeEntry {
                    name: name.clone(),
                    kind: ScopeEntryKind::LetBinding,
                    value: Some(value.clone()),
                    span: Span::dummy(),
                    dependencies: std::collections::HashSet::new(),
                    evaluated: true,
                    read_count: 0,
                },
            );
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

    // Phase 6a: Assign auto-ids to anonymous blocks whose schema opts in
    // via `@auto_id`. Runs before scope construction so every downstream
    // consumer sees the minted id as a real `inline_id`.
    crate::eval::auto_id::assign_auto_ids(&mut doc, &namespace_aliases);

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
        fn glob(&self, pattern: &std::path::Path) -> Result<Vec<PathBuf>, String> {
            self.0.glob(pattern)
        }
    }
    // Pre-scan schema names for has_schema() introspection during evaluation
    let schema_names: std::collections::HashSet<String> = doc
        .items
        .iter()
        .filter_map(|item| {
            if let ast::DocItem::Body(ast::BodyItem::Schema(s)) = item {
                Some(
                    s.name
                        .parts
                        .iter()
                        .filter_map(|p| {
                            if let ast::StringPart::Literal(l) = p {
                                Some(l.clone())
                            } else {
                                None
                            }
                        })
                        .collect::<String>(),
                )
            } else {
                None
            }
        })
        .collect();

    let mut evaluator = Evaluator::with_functions(
        &options.functions,
        Some(Box::new(ArcFs(fs))),
        Some(options.root_dir.clone()),
    );
    evaluator.set_variables(options.variables.clone());
    evaluator.set_imported_paths(imported_paths);
    evaluator.set_schema_names(schema_names);
    evaluator.set_namespace_aliases(namespace_aliases.clone());
    let values = evaluator.evaluate(&doc);
    all_diagnostics.extend(evaluator.into_diagnostics().into_diagnostics());

    // Phase 8: Decorator validation
    let mut decorator_schemas = DecoratorSchemaRegistry::new();
    decorator_schemas.namespace_aliases = namespace_aliases.aliases.clone();
    let mut diag_bag = DiagnosticBag::new();
    decorator_schemas.collect(&doc, &mut diag_bag);
    decorator_schemas.validate_all(&doc, &mut diag_bag);
    all_diagnostics.extend(diag_bag.into_diagnostics());

    // Phase 9: Schema validation
    let mut schemas = SchemaRegistry::new();
    schemas.namespace_aliases = namespace_aliases.aliases;
    let mut diag_bag = DiagnosticBag::new();
    schemas.collect(&doc, &mut diag_bag);

    // Phase 9a: @inline(N) mapping — remap _args entries to named attributes
    let mut values = values;
    apply_inline_mappings(&schemas, &mut values);

    // Phase 9a2: Symbol set collection
    let mut symbol_sets = SymbolSetRegistry::new();
    symbol_sets.collect(&doc, &mut diag_bag);

    schemas.validate(&doc, &values, &symbol_sets, &mut diag_bag);
    all_diagnostics.extend(diag_bag.into_diagnostics());

    // Phase 9b: Table column validation
    let mut diag_bag = DiagnosticBag::new();
    crate::schema::table::validate_tables(&doc, &mut diag_bag);
    all_diagnostics.extend(diag_bag.into_diagnostics());

    // Phase 10: ID uniqueness check
    let mut id_registry = IdRegistry::new();
    let mut diag_bag = DiagnosticBag::new();
    id_registry.check_document(&doc, &mut diag_bag);
    all_diagnostics.extend(diag_bag.into_diagnostics());

    // Phase 11: Document validation
    let mut diag_bag = DiagnosticBag::new();
    crate::schema::document::validate_document(
        &doc,
        &mut Evaluator::with_functions(&options.functions, None, None),
        &mut diag_bag,
    );
    all_diagnostics.extend(diag_bag.into_diagnostics());

    // Phase 9d: Collect struct definitions
    let mut struct_registry = StructRegistry::new();
    let mut struct_diag_bag = DiagnosticBag::new();
    struct_registry.collect(&doc, &mut struct_diag_bag);
    all_diagnostics.extend(struct_diag_bag.into_diagnostics());

    // Phase 9e: Collect layout definitions
    let mut layout_registry = crate::schema::LayoutRegistry::new();
    let mut layout_diag_bag = DiagnosticBag::new();
    layout_registry.collect(&doc, &mut layout_diag_bag);
    all_diagnostics.extend(layout_diag_bag.into_diagnostics());

    Document {
        ast: doc,
        values,
        diagnostics: all_diagnostics,
        source_map,
        schemas,
        decorator_schemas,
        symbol_sets,
        struct_registry,
        layout_registry,
    }
}

/// Walk all evaluated values and remap `_args` entries to named attributes
/// based on `@inline(N)` schema field decorators.
fn apply_inline_mappings(schemas: &SchemaRegistry, values: &mut indexmap::IndexMap<String, Value>) {
    for value in values.values_mut() {
        apply_inline_to_value(schemas, value, None);
    }
}

fn apply_inline_to_value(schemas: &SchemaRegistry, value: &mut Value, parent_kind: Option<&str>) {
    match value {
        Value::BlockRef(br) => apply_inline_to_blockref(schemas, br, parent_kind),
        Value::List(items) => {
            for item in items {
                apply_inline_to_value(schemas, item, parent_kind);
            }
        }
        _ => {}
    }
}

fn apply_inline_to_blockref(
    schemas: &SchemaRegistry,
    br: &mut BlockRef,
    parent_kind: Option<&str>,
) {
    // Recurse into children (current block is their parent)
    let kind = br.kind.clone();
    for child in &mut br.children {
        apply_inline_to_blockref(schemas, child, Some(&kind));
    }

    // Look up schema for this block kind, scoped to parent
    if let Some(schema) = schemas.get_schema(&kind, parent_kind) {
        // Find fields with @inline(N)
        let inline_fields: Vec<(String, usize)> = schema
            .fields
            .iter()
            .filter_map(|f| f.inline_index.map(|idx| (f.name.clone(), idx)))
            .collect();

        if !inline_fields.is_empty() {
            // Build the full positional args: inline_id at index 0, then _args
            let mut all_args: Vec<Value> = Vec::new();
            if let Some(ref id_str) = br.id {
                all_args.push(Value::Identifier(id_str.clone()));
            }
            if let Some(Value::List(args)) = br.attributes.shift_remove("_args") {
                all_args.extend(args);
            }

            for (field_name, idx) in &inline_fields {
                if let Some(val) = all_args.get(*idx) {
                    br.attributes.insert(field_name.clone(), val.clone());
                }
            }
            // Remaining unmapped args go back as _args (excluding index 0 if it was the id)
            let mapped_indices: std::collections::HashSet<usize> =
                inline_fields.iter().map(|(_, idx)| *idx).collect();
            let id_index = if br.id.is_some() { 0 } else { usize::MAX };
            let remaining: Vec<Value> = all_args
                .into_iter()
                .enumerate()
                .filter(|(i, _)| !mapped_indices.contains(i) && *i != id_index)
                .map(|(_, v)| v)
                .collect();
            if !remaining.is_empty() {
                br.attributes
                    .insert("_args".to_string(), Value::List(remaining));
            }
        }
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

/// Serialize a Rust value to pretty-printed WCL text (same as to_string)
pub fn to_string_pretty<T: serde::Serialize>(value: &T) -> Result<String, SerdeError> {
    value_to_string_pretty(value)
}

/// Serialize a Rust value to compact (inline) WCL text
pub fn to_string_compact<T: serde::Serialize>(value: &T) -> Result<String, SerdeError> {
    value_to_string_compact(value)
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
                port: i64
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
                age: i64
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
    fn test_qualified_ids_for_nested_blocks() {
        let doc = parse(
            r#"
            service alpha {
                port http {
                    weight = 100
                }
                port grpc {
                    weight = 50
                }
            }
            service beta {
                port https {
                    weight = 200
                }
            }
            "#,
            ParseOptions::default(),
        );
        let blocks = doc.blocks();
        // Top-level blocks get their bare ID as qualified_id
        let alpha = blocks
            .iter()
            .find(|b| b.id.as_deref() == Some("alpha"))
            .unwrap();
        assert_eq!(alpha.qualified_id.as_deref(), Some("alpha"));

        let beta = blocks
            .iter()
            .find(|b| b.id.as_deref() == Some("beta"))
            .unwrap();
        assert_eq!(beta.qualified_id.as_deref(), Some("beta"));

        // Nested blocks get dotted qualified IDs
        let http = alpha
            .children
            .iter()
            .find(|c| c.id.as_deref() == Some("http"))
            .unwrap();
        assert_eq!(http.qualified_id.as_deref(), Some("alpha.http"));

        let grpc = alpha
            .children
            .iter()
            .find(|c| c.id.as_deref() == Some("grpc"))
            .unwrap();
        assert_eq!(grpc.qualified_id.as_deref(), Some("alpha.grpc"));

        let https = beta
            .children
            .iter()
            .find(|c| c.id.as_deref() == Some("https"))
            .unwrap();
        assert_eq!(https.qualified_id.as_deref(), Some("beta.https"));
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
        let (doc, diags) = crate::lang::parse("import <stdlib.wcl>", FileId(0));
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
        let (doc, _diags) = crate::lang::parse("import \"./other.wcl\"", FileId(0));
        if let ast::DocItem::Import(import) = &doc.items[0] {
            assert_eq!(import.kind, ast::ImportKind::Relative);
        } else {
            panic!("expected Import");
        }
    }

    // ── Phase 3: Function Declarations ───────────────────────────────────

    #[test]
    fn test_parse_function_decl() {
        let (doc, diags) = crate::lang::parse(
            "declare my_fn(input: string, count: i64) -> string",
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
        let (doc, diags) = crate::lang::parse("declare fire_event(name: string)", FileId(0));
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

    // ── Table refactoring tests ──────────────────────────────────────

    #[test]
    fn test_table_with_schema_ref_parses() {
        let source = r#"
            table users : user_row {
                | "Alice" | 30 |
            }
        "#;
        let doc = parse(source, ParseOptions::default());
        // Should parse without errors (schema validation may warn about missing schema)
        let parse_errors: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E002"))
            .collect();
        assert!(
            parse_errors.is_empty(),
            "unexpected parse errors: {:?}",
            parse_errors
        );
    }

    #[test]
    fn test_table_with_schema_decorator_parses() {
        let source = r#"
            @schema("user_row")
            table users {
                | "Alice" | 30 |
            }
        "#;
        let doc = parse(source, ParseOptions::default());
        let parse_errors: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E002"))
            .collect();
        assert!(
            parse_errors.is_empty(),
            "unexpected parse errors: {:?}",
            parse_errors
        );
    }

    #[test]
    fn test_table_import_table_assignment() {
        use std::sync::Arc;

        let mut opts = ParseOptions::default();
        let mut fs = InMemoryFs::new();
        fs.add_file(
            std::path::PathBuf::from("./data.csv"),
            "name,age\nAlice,30\nBob,25",
        );
        opts.fs = Some(Arc::new(fs));

        let source = r#"table users = import_table("data.csv")"#;
        let doc = parse(source, opts);
        let parse_errors: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E002"))
            .collect();
        assert!(
            parse_errors.is_empty(),
            "unexpected parse errors: {:?}",
            parse_errors
        );
    }

    #[test]
    fn test_import_table_headers_false_named_arg() {
        let source = r#"val = import_table("data.csv", headers=false)"#;
        let (doc, diags) = crate::lang::parse(source, FileId(0));
        assert!(
            !diags.has_errors(),
            "diagnostics: {:?}",
            diags.into_diagnostics()
        );
        match &doc.items[0] {
            ast::DocItem::Body(ast::BodyItem::Attribute(attr)) => match &attr.value {
                ast::Expr::ImportTable(args, _) => {
                    assert_eq!(args.headers, Some(false));
                }
                other => panic!("expected ImportTable, got {:?}", other),
            },
            other => panic!("expected Attribute, got {:?}", other),
        }
    }

    #[test]
    fn test_import_table_columns_named_arg() {
        let source = r#"val = import_table("data.csv", headers=false, columns=["x", "y"])"#;
        let (doc, diags) = crate::lang::parse(source, FileId(0));
        assert!(
            !diags.has_errors(),
            "diagnostics: {:?}",
            diags.into_diagnostics()
        );
        match &doc.items[0] {
            ast::DocItem::Body(ast::BodyItem::Attribute(attr)) => match &attr.value {
                ast::Expr::ImportTable(args, _) => {
                    assert_eq!(args.headers, Some(false));
                    assert!(args.columns.is_some());
                    assert_eq!(args.columns.as_ref().unwrap().len(), 2);
                }
                other => panic!("expected ImportTable, got {:?}", other),
            },
            other => panic!("expected Attribute, got {:?}", other),
        }
    }

    #[test]
    fn test_table_schema_ref_plus_inline_columns_e092() {
        let source = r#"
            table users : user_row {
                name : string
                | "Alice" |
            }
        "#;
        let doc = parse(source, ParseOptions::default());
        let e092_errors: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E092"))
            .collect();
        assert_eq!(
            e092_errors.len(),
            1,
            "expected E092 error, got: {:?}",
            doc.diagnostics
        );
    }

    // ── Text block integration tests ──────────────────────────────────────

    #[test]
    fn text_block_end_to_end() {
        let source = r#"
schema "readme" {
    content: string @text
}

readme my-doc <<EOF
# Hello World
This is content.
EOF
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(
            doc.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            doc.diagnostics
        );

        let block_val = doc.values.get("my-doc").expect("block should exist");
        match block_val {
            crate::eval::value::Value::BlockRef(br) => {
                assert_eq!(br.kind, "readme");
                assert_eq!(br.id, Some("my-doc".to_string()));
                let content = br.attributes.get("content").expect("content should exist");
                match content {
                    crate::eval::value::Value::String(s) => {
                        assert!(s.contains("Hello World"));
                        assert!(s.contains("This is content."));
                    }
                    other => panic!("expected String, got {:?}", other),
                }
            }
            other => panic!("expected BlockRef, got {:?}", other),
        }
    }

    #[test]
    fn text_block_string_end_to_end() {
        let source = r#"
schema "readme" {
    content: string @text
}

readme my-doc "Simple content"
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(
            doc.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            doc.diagnostics
        );

        let block_val = doc.values.get("my-doc").expect("block should exist");
        match block_val {
            crate::eval::value::Value::BlockRef(br) => {
                assert_eq!(
                    br.attributes.get("content"),
                    Some(&crate::eval::value::Value::String(
                        "Simple content".to_string()
                    ))
                );
            }
            other => panic!("expected BlockRef, got {:?}", other),
        }
    }

    #[test]
    fn text_block_with_interpolation_end_to_end() {
        let source = r#"
schema "readme" {
    content: string @text
}

let name = "World"
readme my-doc "Hello ${name}!"
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(
            doc.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            doc.diagnostics
        );

        let block_val = doc.values.get("my-doc").expect("block should exist");
        match block_val {
            crate::eval::value::Value::BlockRef(br) => {
                assert_eq!(
                    br.attributes.get("content"),
                    Some(&crate::eval::value::Value::String(
                        "Hello World!".to_string()
                    ))
                );
            }
            other => panic!("expected BlockRef, got {:?}", other),
        }
    }

    #[test]
    fn text_block_e093_no_text_schema() {
        let source = r#"
schema "readme" {
    name: string
}

readme my-doc "content here"
        "#;
        let doc = parse(source, ParseOptions::default());
        let e093: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E093"))
            .collect();
        assert_eq!(e093.len(), 1, "expected E093, got: {:?}", doc.diagnostics);
    }

    // ── Containment integration tests ────────────────────────────────────

    #[test]
    fn containment_end_to_end() {
        let source = r#"
@children(["endpoint"])
schema "service" {
    name: string
}

service main {
    name = "api"
    endpoint health {
        path = "/health"
    }
}
        "#;
        let doc = parse(source, ParseOptions::default());
        let containment_errors: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E095") || d.code.as_deref() == Some("E096"))
            .collect();
        assert!(
            containment_errors.is_empty(),
            "unexpected containment errors: {:?}",
            containment_errors
        );
    }

    #[test]
    fn containment_e095_end_to_end() {
        let source = r#"
@children(["endpoint"])
schema "service" {}

service main {
    middleware auth {}
}
        "#;
        let doc = parse(source, ParseOptions::default());
        let e095: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E095"))
            .collect();
        assert_eq!(e095.len(), 1, "expected E095, got: {:?}", doc.diagnostics);
    }

    #[test]
    fn containment_e096_end_to_end() {
        let source = r#"
@parent(["service"])
schema "endpoint" {}

endpoint orphan {}
        "#;
        let doc = parse(source, ParseOptions::default());
        let e096: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E096"))
            .collect();
        assert_eq!(e096.len(), 1, "expected E096, got: {:?}", doc.diagnostics);
    }

    #[test]
    fn containment_root_end_to_end() {
        let source = r#"
@children(["service"])
schema "_root" {}

config main {}
        "#;
        let doc = parse(source, ParseOptions::default());
        let e095: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E095"))
            .collect();
        assert_eq!(e095.len(), 1, "expected E095, got: {:?}", doc.diagnostics);
    }

    #[test]
    fn containment_table_e095() {
        let source = r#"
@children(["endpoint"])
schema "service" {}

service main {
    table users {
        | "Alice" |
    }
}
        "#;
        let doc = parse(source, ParseOptions::default());
        let e095: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E095"))
            .collect();
        assert_eq!(
            e095.len(),
            1,
            "expected E095 for table, got: {:?}",
            doc.diagnostics
        );
    }

    #[test]
    fn containment_table_e096() {
        let source = r#"
@parent(["data"])
schema "table" {}

table users {
    | "Alice" |
}
        "#;
        let doc = parse(source, ParseOptions::default());
        let e096: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E096"))
            .collect();
        assert_eq!(
            e096.len(),
            1,
            "expected E096 for table, got: {:?}",
            doc.diagnostics
        );
    }

    // ── External variable overrides ─────────────────────────────────────

    #[test]
    fn test_variable_override_basic() {
        let mut opts = ParseOptions::default();
        opts.variables.insert("PORT".to_string(), Value::Int(8080));
        let doc = parse("port = PORT", opts);
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
        assert_eq!(doc.values.get("port"), Some(&Value::Int(8080)));
    }

    #[test]
    fn test_variable_override_overrides_let() {
        let mut opts = ParseOptions::default();
        opts.variables.insert("x".to_string(), Value::Int(99));
        let doc = parse("let x = 2\nresult = x", opts);
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
        assert_eq!(doc.values.get("result"), Some(&Value::Int(99)));
    }

    #[test]
    fn test_variable_in_control_flow() {
        let mut opts = ParseOptions::default();
        opts.variables.insert(
            "items".to_string(),
            Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
        );
        let doc = parse("for item in items { entry { value = item } }", opts);
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
        let entries = doc.blocks_of_type("entry");
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn test_variable_types() {
        let mut opts = ParseOptions::default();
        opts.variables
            .insert("s".to_string(), Value::String("hello".to_string()));
        opts.variables.insert("i".to_string(), Value::Int(42));
        opts.variables.insert("f".to_string(), Value::Float(3.14));
        opts.variables.insert("b".to_string(), Value::Bool(true));
        opts.variables.insert("n".to_string(), Value::Null);
        opts.variables.insert(
            "l".to_string(),
            Value::List(vec![Value::Int(1), Value::Int(2)]),
        );
        let doc = parse("vs = s\nvi = i\nvf = f\nvb = b\nvn = n\nvl = l", opts);
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
        assert_eq!(
            doc.values.get("vs"),
            Some(&Value::String("hello".to_string()))
        );
        assert_eq!(doc.values.get("vi"), Some(&Value::Int(42)));
        assert_eq!(doc.values.get("vf"), Some(&Value::Float(3.14)));
        assert_eq!(doc.values.get("vb"), Some(&Value::Bool(true)));
        assert_eq!(doc.values.get("vn"), Some(&Value::Null));
        assert_eq!(
            doc.values.get("vl"),
            Some(&Value::List(vec![Value::Int(1), Value::Int(2)]))
        );
    }

    #[test]
    fn test_no_variables_backwards_compat() {
        let doc = parse("x = 42", ParseOptions::default());
        assert!(!doc.has_errors());
        assert_eq!(doc.values.get("x"), Some(&Value::Int(42)));
    }

    #[test]
    fn text_block_e094_schema_expects_text() {
        let source = r#"
schema "readme" {
    content: string @text
}

readme my-doc {
    content = "using brace body"
}
        "#;
        let doc = parse(source, ParseOptions::default());
        let e094: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E094"))
            .collect();
        assert_eq!(e094.len(), 1, "expected E094, got: {:?}", doc.diagnostics);
    }

    #[test]
    fn test_attr_macro_remove_child_block() {
        let source = r#"
macro @secure() {
    remove [endpoint#debug]
}

@secure()
service main {
    port = 8080
    endpoint health {
        path = "/health"
    }
    endpoint debug {
        path = "/debug"
    }
}
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(
            doc.diagnostics.is_empty(),
            "diagnostics: {:?}",
            doc.diagnostics
        );
        let block_val = doc.values.get("main").expect("main block should exist");
        if let Value::BlockRef(br) = block_val {
            // health child should exist, debug should not
            let child_ids: Vec<Option<&str>> =
                br.children.iter().map(|c| c.id.as_deref()).collect();
            assert!(
                child_ids.contains(&Some("health")),
                "health endpoint should exist"
            );
            assert!(
                !child_ids.contains(&Some("debug")),
                "debug endpoint should be removed"
            );
        } else {
            panic!("expected BlockRef, got {:?}", block_val);
        }
    }

    #[test]
    fn test_attr_macro_update_child_block() {
        let source = r#"
macro @secure() {
    update endpoint#health {
        set {
            tls = true
        }
    }
}

@secure()
service main {
    port = 8080
    endpoint health {
        path = "/health"
    }
}
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(
            doc.diagnostics.is_empty(),
            "diagnostics: {:?}",
            doc.diagnostics
        );
        let block_val = doc.values.get("main").expect("main block should exist");
        if let Value::BlockRef(br) = block_val {
            let health = br
                .children
                .iter()
                .find(|c| c.id.as_deref() == Some("health"))
                .expect("health child should exist");
            assert_eq!(
                health.attributes.get("tls"),
                Some(&Value::Bool(true)),
                "tls should be set to true"
            );
            assert_eq!(
                health.attributes.get("path"),
                Some(&Value::String("/health".to_string())),
                "path should be preserved"
            );
        } else {
            panic!("expected BlockRef, got {:?}", block_val);
        }
    }

    #[test]
    fn test_attr_macro_table_row_ops() {
        let source = r#"
macro @filter() {
    update table#users {
        remove_rows where role == "guest"
        inject_rows {
            | "admin" | "admin" |
        }
    }
}

@filter()
service main {
    table users {
        name : string
        role : string
        | "alice" | "admin" |
        | "bob"   | "guest" |
    }
}
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(
            doc.diagnostics.is_empty(),
            "diagnostics: {:?}",
            doc.diagnostics
        );
        let block_val = doc.values.get("main").expect("main block");
        if let Value::BlockRef(br) = block_val {
            let users = br
                .attributes
                .get("users")
                .expect("users table should exist in attributes");
            if let Value::List(rows) = users {
                // bob/guest removed, admin/admin added → alice/admin + admin/admin
                assert_eq!(rows.len(), 2, "expected 2 rows, got: {:?}", rows);
                // First row: alice/admin
                if let Value::Map(r) = &rows[0] {
                    assert_eq!(r.get("name"), Some(&Value::String("alice".to_string())));
                    assert_eq!(r.get("role"), Some(&Value::String("admin".to_string())));
                }
                // Second row: admin/admin
                if let Value::Map(r) = &rows[1] {
                    assert_eq!(r.get("name"), Some(&Value::String("admin".to_string())));
                    assert_eq!(r.get("role"), Some(&Value::String("admin".to_string())));
                }
            } else {
                panic!("expected List, got: {:?}", users);
            }
        } else {
            panic!("expected BlockRef");
        }
    }

    // ── Table evaluation tests ──────────────────────────────────────────

    #[test]
    fn test_inline_table_evaluates_to_list_of_maps() {
        let source = r#"
table users {
    name : string
    age : i64
    | "alice" | 25 |
    | "bob"   | 30 |
}
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(
            doc.diagnostics.is_empty(),
            "diagnostics: {:?}",
            doc.diagnostics
        );
        let users = doc.values.get("users").expect("users table should exist");
        if let Value::List(rows) = users {
            assert_eq!(rows.len(), 2);
            if let Value::Map(row0) = &rows[0] {
                assert_eq!(row0.get("name"), Some(&Value::String("alice".to_string())));
                assert_eq!(row0.get("age"), Some(&Value::Int(25)));
            } else {
                panic!("expected row as Map, got: {:?}", rows[0]);
            }
            if let Value::Map(row1) = &rows[1] {
                assert_eq!(row1.get("name"), Some(&Value::String("bob".to_string())));
                assert_eq!(row1.get("age"), Some(&Value::Int(30)));
            } else {
                panic!("expected row as Map, got: {:?}", rows[1]);
            }
        } else {
            panic!("expected List, got: {:?}", users);
        }
    }

    #[test]
    fn test_inline_table_in_block_evaluates() {
        let source = r#"
service main {
    port = 8080
    table users {
        name : string
        role : string
        | "alice" | "admin" |
    }
}
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(
            doc.diagnostics.is_empty(),
            "diagnostics: {:?}",
            doc.diagnostics
        );
        let block_val = doc.values.get("main").expect("main block");
        if let Value::BlockRef(br) = block_val {
            let users = br
                .attributes
                .get("users")
                .expect("users should be in attributes");
            if let Value::List(rows) = users {
                assert_eq!(rows.len(), 1);
                if let Value::Map(row) = &rows[0] {
                    assert_eq!(row.get("name"), Some(&Value::String("alice".to_string())));
                    assert_eq!(row.get("role"), Some(&Value::String("admin".to_string())));
                } else {
                    panic!("expected Map");
                }
            } else {
                panic!("expected List, got: {:?}", users);
            }
        } else {
            panic!("expected BlockRef");
        }
    }

    #[test]
    fn test_inline_table_empty_rows() {
        let source = r#"
table empty {
    name : string
}
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(
            doc.diagnostics.is_empty(),
            "diagnostics: {:?}",
            doc.diagnostics
        );
        let val = doc.values.get("empty").expect("empty table");
        assert_eq!(val, &Value::List(vec![]));
    }

    #[test]
    fn test_inline_table_with_expressions() {
        let source = r#"
let base = 100
table config {
    key : string
    value : i64
    | "port" | base + 80 |
    | "debug" | 0 |
}
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(
            doc.diagnostics.is_empty(),
            "diagnostics: {:?}",
            doc.diagnostics
        );
        let val = doc.values.get("config").expect("config table");
        if let Value::List(rows) = val {
            assert_eq!(rows.len(), 2);
            if let Value::Map(row0) = &rows[0] {
                assert_eq!(row0.get("value"), Some(&Value::Int(180)));
            } else {
                panic!("expected Map");
            }
        } else {
            panic!("expected List");
        }
    }

    #[test]
    fn test_inline_table_bool_cells() {
        let source = r#"
table flags {
    key    : string
    active : bool
    count  : i64
    | "a" | true  | 10 |
    | "b" | false | 20 |
}
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(
            doc.diagnostics.is_empty(),
            "diagnostics: {:?}",
            doc.diagnostics
        );
        let val = doc.values.get("flags").expect("flags table");
        if let Value::List(rows) = val {
            assert_eq!(rows.len(), 2);
            if let Value::Map(r0) = &rows[0] {
                assert_eq!(r0.get("active"), Some(&Value::Bool(true)));
                assert_eq!(r0.get("count"), Some(&Value::Int(10)));
            }
            if let Value::Map(r1) = &rows[1] {
                assert_eq!(r1.get("active"), Some(&Value::Bool(false)));
                assert_eq!(r1.get("count"), Some(&Value::Int(20)));
            }
        } else {
            panic!("expected List");
        }
    }

    #[test]
    fn test_inline_table_multiple_tables_in_block() {
        let source = r#"
service main {
    port = 8080
    table users {
        name : string
        | "alice" |
    }
    table roles {
        role : string
        | "admin" |
    }
}
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(
            doc.diagnostics.is_empty(),
            "diagnostics: {:?}",
            doc.diagnostics
        );
        let block_val = doc.values.get("main").expect("main block");
        if let Value::BlockRef(br) = block_val {
            assert!(br.attributes.contains_key("users"));
            assert!(br.attributes.contains_key("roles"));
            if let Value::List(users) = &br.attributes["users"] {
                assert_eq!(users.len(), 1);
            }
            if let Value::List(roles) = &br.attributes["roles"] {
                assert_eq!(roles.len(), 1);
            }
        } else {
            panic!("expected BlockRef");
        }
    }

    #[test]
    fn test_inline_table_at_top_level() {
        let source = r#"
table config {
    key   : string
    value : string
    | "host" | "localhost" |
    | "port" | "8080"      |
}
name = "my-app"
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(
            doc.diagnostics.is_empty(),
            "diagnostics: {:?}",
            doc.diagnostics
        );
        assert!(doc.values.contains_key("config"));
        assert_eq!(
            doc.values.get("name"),
            Some(&Value::String("my-app".to_string()))
        );
        if let Value::List(rows) = doc.values.get("config").unwrap() {
            assert_eq!(rows.len(), 2);
        }
    }

    #[test]
    fn test_inline_table_float_cells() {
        let source = r#"
table prices {
    item  : string
    price : f64
    | "apple"  | 1.50 |
    | "banana" | 0.75 |
}
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(
            doc.diagnostics.is_empty(),
            "diagnostics: {:?}",
            doc.diagnostics
        );
        if let Value::List(rows) = doc.values.get("prices").unwrap() {
            if let Value::Map(r0) = &rows[0] {
                assert_eq!(r0.get("price"), Some(&Value::Float(1.50)));
            }
        }
    }

    #[test]
    fn test_attr_macro_remove_all_tables() {
        let source = r#"
macro @no_tables() {
    remove [table#*]
}

@no_tables()
service main {
    port = 8080
    table users {
        name : string
        | "alice" |
    }
    table roles {
        role : string
        | "admin" |
    }
}
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(
            doc.diagnostics.is_empty(),
            "diagnostics: {:?}",
            doc.diagnostics
        );
        let block_val = doc.values.get("main").expect("main block");
        if let Value::BlockRef(br) = block_val {
            assert!(!br.attributes.contains_key("users"));
            assert!(!br.attributes.contains_key("roles"));
            assert_eq!(br.attributes.get("port"), Some(&Value::Int(8080)));
        } else {
            panic!("expected BlockRef");
        }
    }

    #[test]
    fn test_attr_macro_update_table_clear_rows() {
        let source = r#"
macro @clear_data() {
    update table#users {
        clear_rows
    }
}

@clear_data()
service main {
    table users {
        name : string
        | "alice" |
        | "bob"   |
    }
}
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(
            doc.diagnostics.is_empty(),
            "diagnostics: {:?}",
            doc.diagnostics
        );
        let block_val = doc.values.get("main").expect("main block");
        if let Value::BlockRef(br) = block_val {
            let users = br.attributes.get("users").expect("users table");
            assert_eq!(users, &Value::List(vec![]));
        } else {
            panic!("expected BlockRef");
        }
    }

    #[test]
    fn test_attr_macro_combined_block_and_table_ops() {
        let source = r#"
macro @secure() {
    remove [endpoint#debug]
    update endpoint#health {
        set { tls = true }
    }
    update table#users {
        remove_rows where role == "guest"
        inject_rows {
            | "admin" | "admin" |
        }
    }
}

@secure()
service main {
    port = 8080
    endpoint health { path = "/health" }
    endpoint debug { path = "/debug" }
    table users {
        name : string
        role : string
        | "alice" | "admin" |
        | "bob"   | "guest" |
    }
}
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(
            doc.diagnostics.is_empty(),
            "diagnostics: {:?}",
            doc.diagnostics
        );
        let block_val = doc.values.get("main").expect("main block");
        if let Value::BlockRef(br) = block_val {
            // debug endpoint should be removed
            assert!(
                !br.children.iter().any(|c| c.id.as_deref() == Some("debug")),
                "debug should be removed"
            );
            // health endpoint should have tls = true
            let health = br
                .children
                .iter()
                .find(|c| c.id.as_deref() == Some("health"))
                .expect("health child");
            assert_eq!(health.attributes.get("tls"), Some(&Value::Bool(true)));
            // users table: bob removed, admin added
            let users = br.attributes.get("users").expect("users table");
            if let Value::List(rows) = users {
                assert_eq!(rows.len(), 2);
                // Should be alice/admin and admin/admin
                if let Value::Map(r0) = &rows[0] {
                    assert_eq!(r0.get("name"), Some(&Value::String("alice".to_string())));
                }
                if let Value::Map(r1) = &rows[1] {
                    assert_eq!(r1.get("name"), Some(&Value::String("admin".to_string())));
                }
            }
        } else {
            panic!("expected BlockRef");
        }
    }

    #[test]
    fn test_table_name_collision_with_attribute() {
        let source = r#"
service main {
    users = "something"
    table users {
        name : string
        | "alice" |
    }
}
        "#;
        let doc = parse(source, ParseOptions::default());
        let e030: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E030"))
            .collect();
        assert!(
            !e030.is_empty(),
            "expected E030 for table/attribute name collision, got: {:?}",
            doc.diagnostics
        );
    }

    // ── Table query tests ─────────────────────────────────────────────────

    #[test]
    fn test_query_table_by_id() {
        let source = r#"
table users {
    name : string
    age  : i64
    | "alice" | 25 |
    | "bob"   | 30 |
}
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);

        let result = doc.query("table#users | .name == \"alice\"").unwrap();
        match result {
            Value::List(items) => {
                assert_eq!(items.len(), 1);
                if let Value::BlockRef(br) = &items[0] {
                    assert_eq!(br.kind, "__row");
                    assert_eq!(
                        br.attributes.get("name"),
                        Some(&Value::String("alice".to_string()))
                    );
                    assert_eq!(br.attributes.get("age"), Some(&Value::Int(25)));
                } else {
                    panic!("expected BlockRef, got {:?}", items[0]);
                }
            }
            _ => panic!("expected list, got {:?}", result),
        }
    }

    #[test]
    fn test_query_table_projection() {
        let source = r#"
table users {
    name : string
    age  : i64
    | "alice" | 25 |
    | "bob"   | 30 |
}
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);

        let result = doc.query("table#users | .name").unwrap();
        assert_eq!(
            result,
            Value::List(vec![
                Value::String("alice".to_string()),
                Value::String("bob".to_string()),
            ])
        );
    }

    #[test]
    fn test_query_table_empty_result() {
        let source = r#"
table users {
    name : string
    age  : i64
    | "alice" | 25 |
    | "bob"   | 30 |
}
        "#;
        let doc = parse(source, ParseOptions::default());
        let result = doc.query("table#users | .name == \"charlie\"").unwrap();
        assert_eq!(result, Value::List(vec![]));
    }

    #[test]
    fn test_query_table_in_block() {
        let source = r#"
service main {
    port = 8080
    table users {
        name : string
        role : string
        | "alice" | "admin"  |
        | "bob"   | "viewer" |
    }
}
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);

        let result = doc.query("table#users | .role == \"admin\"").unwrap();
        match result {
            Value::List(items) => {
                assert_eq!(items.len(), 1);
                if let Value::BlockRef(br) = &items[0] {
                    assert_eq!(
                        br.attributes.get("name"),
                        Some(&Value::String("alice".to_string()))
                    );
                } else {
                    panic!("expected BlockRef");
                }
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn test_query_table_comparison_operators() {
        let source = r#"
table users {
    name : string
    age  : i64
    | "alice" | 25 |
    | "bob"   | 30 |
    | "carol" | 20 |
}
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);

        // Greater than
        let result = doc.query("table#users | .age > 24").unwrap();
        match &result {
            Value::List(items) => assert_eq!(items.len(), 2),
            _ => panic!("expected list"),
        }

        // Less than
        let result = doc.query("table#users | .age < 26").unwrap();
        match &result {
            Value::List(items) => assert_eq!(items.len(), 2),
            _ => panic!("expected list"),
        }

        // Not equal
        let result = doc.query("table#users | .name != \"bob\"").unwrap();
        match &result {
            Value::List(items) => assert_eq!(items.len(), 2),
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn test_query_table_in_expression() {
        let source = r#"
table users {
    name : string
    role : string
    | "alice" | "admin"  |
    | "bob"   | "viewer" |
}
admins = table#users | .role == "admin" | .name
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
        assert_eq!(
            doc.values.get("admins"),
            Some(&Value::List(vec![Value::String("alice".to_string())]))
        );
    }

    #[test]
    fn test_doc_query_table() {
        let source = r#"
table users {
    name : string
    role : string
    | "alice" | "admin"  |
    | "bob"   | "viewer" |
    | "carol" | "admin"  |
}
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);

        let result = doc
            .query("table#users | .role == \"admin\" | .name")
            .unwrap();
        assert_eq!(
            result,
            Value::List(vec![
                Value::String("alice".to_string()),
                Value::String("carol".to_string()),
            ])
        );
    }

    #[test]
    fn test_table_with_let_dependency() {
        let source = r#"
let prefix = "svc"

table services {
    name : string
    port : i64
    | prefix + "-api"    | 8080 |
    | prefix + "-admin"  | 9090 |
}
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(
            doc.diagnostics.is_empty(),
            "diagnostics: {:?}",
            doc.diagnostics
        );
        if let Value::List(rows) = doc.values.get("services").unwrap() {
            assert_eq!(rows.len(), 2);
            if let Value::Map(r0) = &rows[0] {
                assert_eq!(r0.get("name"), Some(&Value::String("svc-api".to_string())));
            }
        }
    }

    // ── Inline args tests ───────────────────────────────────────────────

    #[test]
    fn inline_args_without_schema_produce_args_attr() {
        let doc = parse(
            r#"server web 8080 "prod" { host = "localhost" }"#,
            ParseOptions::default(),
        );
        assert!(!doc.has_errors(), "errors: {:?}", doc.errors());
        // Find the server block in values
        let val = doc
            .values
            .values()
            .find(|v| matches!(v, Value::BlockRef(br) if br.kind == "server"))
            .unwrap();
        if let Value::BlockRef(br) = val {
            assert_eq!(
                br.get("_args"),
                Some(&Value::List(vec![
                    Value::Int(8080),
                    Value::String("prod".to_string()),
                ]))
            );
            assert_eq!(
                br.get("host"),
                Some(&Value::String("localhost".to_string()))
            );
        } else {
            panic!("expected BlockRef, got {:?}", val);
        }
    }

    #[test]
    fn inline_schema_maps_args_to_named_attributes() {
        let src = r#"
schema "server" {
    id: identifier @inline(0)
    port: i64 @inline(1)
    env: string @inline(2)
    host: string
}
server web 8080 "prod" {
    host = "localhost"
}
"#;
        let doc = parse(src, ParseOptions::default());
        assert!(!doc.has_errors(), "errors: {:?}", doc.errors());
        let val = doc
            .values
            .values()
            .find(|v| matches!(v, Value::BlockRef(br) if br.kind == "server"))
            .unwrap();
        if let Value::BlockRef(br) = val {
            assert_eq!(br.get("id"), Some(&Value::Identifier("web".to_string())));
            assert_eq!(br.get("port"), Some(&Value::Int(8080)));
            assert_eq!(br.get("env"), Some(&Value::String("prod".to_string())));
            assert_eq!(
                br.get("host"),
                Some(&Value::String("localhost".to_string()))
            );
            // _args should be removed since all args are mapped
            assert!(br.get("_args").is_none());
        } else {
            panic!("expected BlockRef, got {:?}", val);
        }
    }

    #[test]
    fn inline_schema_partial_mapping_keeps_remaining_args() {
        let src = r#"
schema "server" {
    port: i64 @inline(1)
    host: string
}
server web 8080 "extra" {
    host = "localhost"
}
"#;
        let doc = parse(src, ParseOptions::default());
        assert!(!doc.has_errors(), "errors: {:?}", doc.errors());
        let val = doc
            .values
            .values()
            .find(|v| matches!(v, Value::BlockRef(br) if br.kind == "server"))
            .unwrap();
        if let Value::BlockRef(br) = val {
            assert_eq!(br.get("port"), Some(&Value::Int(8080)));
            assert_eq!(
                br.get("_args"),
                Some(&Value::List(vec![Value::String("extra".to_string()),]))
            );
        } else {
            panic!("expected BlockRef, got {:?}", val);
        }
    }

    // ── Symbol tests ─────────────────────────────────────────────────────

    #[test]
    fn test_symbol_literal_evaluation() {
        let doc = parse("method = :GET", ParseOptions::default());
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
        assert_eq!(doc.values.get("method"), Some(&Value::Symbol("GET".into())));
    }

    #[test]
    fn test_symbol_set_collection() {
        let doc = parse(
            "symbol_set http_method { :GET :POST :PUT :DELETE }",
            ParseOptions::default(),
        );
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
        assert!(doc.symbol_sets.set_exists("http_method"));
        assert!(doc.symbol_sets.contains("http_method", "GET"));
        assert!(!doc.symbol_sets.contains("http_method", "PATCH"));
    }

    #[test]
    fn test_symbol_set_valid_usage() {
        let src = r#"
symbol_set http_method { :GET :POST }
schema "operation" {
    method: symbol @symbol_set("http_method")
}
operation "x" {
    method = :GET
}
"#;
        let doc = parse(src, ParseOptions::default());
        let errors = doc.errors();
        assert!(errors.is_empty(), "expected no errors: {:?}", errors);
    }

    #[test]
    fn test_symbol_set_invalid_member_e100() {
        let src = r#"
symbol_set http_method { :GET :POST }
schema "operation" {
    method: symbol @symbol_set("http_method")
}
operation "x" {
    method = :PATCH
}
"#;
        let doc = parse(src, ParseOptions::default());
        let e100: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E100"))
            .collect();
        assert_eq!(e100.len(), 1, "expected E100: {:?}", doc.diagnostics);
    }

    #[test]
    fn test_symbol_set_missing_set_e101() {
        let src = r#"
schema "operation" {
    method: symbol @symbol_set("nonexistent")
}
operation "x" {
    method = :GET
}
"#;
        let doc = parse(src, ParseOptions::default());
        let e101: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E101"))
            .collect();
        assert_eq!(e101.len(), 1, "expected E101: {:?}", doc.diagnostics);
    }

    #[test]
    fn test_symbol_set_duplicate_e102() {
        let src = "symbol_set x { :a }\nsymbol_set x { :b }";
        let doc = parse(src, ParseOptions::default());
        let e102: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E102"))
            .collect();
        assert_eq!(e102.len(), 1, "expected E102: {:?}", doc.diagnostics);
    }

    #[test]
    fn test_symbol_set_duplicate_member_e103() {
        let src = "symbol_set x { :a :b :a }";
        let doc = parse(src, ParseOptions::default());
        let e103: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E103"))
            .collect();
        assert_eq!(e103.len(), 1, "expected E103: {:?}", doc.diagnostics);
    }

    #[test]
    fn test_symbol_set_all_accepts_any() {
        let src = r#"
schema "thing" {
    kind: symbol @symbol_set("all")
}
thing "x" {
    kind = :whatever
}
"#;
        let doc = parse(src, ParseOptions::default());
        let errors = doc.errors();
        assert!(
            errors.is_empty(),
            "\"all\" should accept any symbol: {:?}",
            errors
        );
    }

    #[test]
    fn test_symbol_type_mismatch_e071() {
        let src = r#"
schema "thing" {
    kind: symbol
}
thing "x" {
    kind = "not_a_symbol"
}
"#;
        let doc = parse(src, ParseOptions::default());
        let e071: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E071"))
            .collect();
        assert_eq!(e071.len(), 1, "expected E071: {:?}", doc.diagnostics);
    }

    #[test]
    fn test_symbol_set_value_mapping() {
        let src = r#"
symbol_set multi {
    :zero_or_one = "0..1"
    :one = "1"
    :many
}
"#;
        let doc = parse(src, ParseOptions::default());
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
        assert_eq!(
            doc.symbol_sets.serialize_symbol("multi", "zero_or_one"),
            "0..1"
        );
        assert_eq!(doc.symbol_sets.serialize_symbol("multi", "one"), "1");
        assert_eq!(doc.symbol_sets.serialize_symbol("multi", "many"), "many");
    }

    #[test]
    fn test_symbol_json_serialization() {
        let doc = parse("method = :GET", ParseOptions::default());
        let json = crate::json::value_to_json(doc.values.get("method").unwrap());
        assert_eq!(json, serde_json::json!("GET"));
    }

    // ── Glob imports ────────────────────────────────────────────────────

    #[test]
    fn test_glob_import_matches_files() {
        let mut fs = InMemoryFs::new();
        fs.add_file(
            std::path::PathBuf::from("/project/schemas/a.wcl"),
            "schema \"a\" { name: string }",
        );
        fs.add_file(
            std::path::PathBuf::from("/project/schemas/b.wcl"),
            "schema \"b\" { port: i64 }",
        );
        // non-wcl file should be filtered out
        fs.add_file(
            std::path::PathBuf::from("/project/schemas/readme.md"),
            "# Readme",
        );
        let opts = ParseOptions {
            root_dir: std::path::PathBuf::from("/project"),
            fs: Some(Arc::new(fs)),
            ..ParseOptions::default()
        };
        let doc = parse("import \"./schemas/*.wcl\"", opts);
        assert!(!doc.has_errors(), "errors: {:?}", doc.errors());
        // Both schemas should be imported
        assert!(doc.schemas.schemas.contains_key("a"));
        assert!(doc.schemas.schemas.contains_key("b"));
    }

    #[test]
    fn test_glob_import_no_match_emits_e016() {
        let fs = InMemoryFs::new();
        let opts = ParseOptions {
            root_dir: std::path::PathBuf::from("/project"),
            fs: Some(Arc::new(fs)),
            ..ParseOptions::default()
        };
        let doc = parse("import \"./schemas/*.wcl\"", opts);
        let e016: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E016"))
            .collect();
        assert_eq!(e016.len(), 1);
    }

    // ── Optional imports ────────────────────────────────────────────────

    #[test]
    fn test_optional_import_missing_file_no_error() {
        let fs = InMemoryFs::new();
        let opts = ParseOptions {
            root_dir: std::path::PathBuf::from("/project"),
            fs: Some(Arc::new(fs)),
            ..ParseOptions::default()
        };
        let doc = parse("import? \"./nonexistent.wcl\"", opts);
        assert!(!doc.has_errors(), "errors: {:?}", doc.errors());
    }

    #[test]
    fn test_optional_import_existing_file_imported() {
        let mut fs = InMemoryFs::new();
        fs.add_file(
            std::path::PathBuf::from("/project/extra.wcl"),
            "schema \"extra\" { name: string }",
        );
        let opts = ParseOptions {
            root_dir: std::path::PathBuf::from("/project"),
            fs: Some(Arc::new(fs)),
            ..ParseOptions::default()
        };
        let doc = parse("import? \"./extra.wcl\"", opts);
        assert!(!doc.has_errors(), "errors: {:?}", doc.errors());
        assert!(doc.schemas.schemas.contains_key("extra"));
    }

    #[test]
    fn test_optional_glob_no_matches_no_error() {
        let fs = InMemoryFs::new();
        let opts = ParseOptions {
            root_dir: std::path::PathBuf::from("/project"),
            fs: Some(Arc::new(fs)),
            ..ParseOptions::default()
        };
        let doc = parse("import? \"./env/*.wcl\"", opts);
        assert!(!doc.has_errors(), "errors: {:?}", doc.errors());
    }

    // ── Introspection functions ─────────────────────────────────────────

    #[test]
    fn test_has_schema_true() {
        let source = r#"
            schema "service" { port: i64 }
            found = has_schema("service")
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(!doc.has_errors(), "errors: {:?}", doc.errors());
        assert_eq!(doc.values.get("found"), Some(&Value::Bool(true)));
    }

    #[test]
    fn test_has_schema_false() {
        let source = "found = has_schema(\"nonexistent\")";
        let doc = parse(source, ParseOptions::default());
        assert!(!doc.has_errors(), "errors: {:?}", doc.errors());
        assert_eq!(doc.values.get("found"), Some(&Value::Bool(false)));
    }

    #[test]
    fn test_is_imported_with_imported_file() {
        let mut fs = InMemoryFs::new();
        fs.add_file(
            std::path::PathBuf::from("/project/auth.wcl"),
            "schema \"auth\" { token: string }",
        );
        let opts = ParseOptions {
            root_dir: std::path::PathBuf::from("/project"),
            fs: Some(Arc::new(fs)),
            ..ParseOptions::default()
        };
        let source = r#"
            import "./auth.wcl"
            has_auth = is_imported("./auth.wcl")
        "#;
        let doc = parse(source, opts);
        assert!(!doc.has_errors(), "errors: {:?}", doc.errors());
        assert_eq!(doc.values.get("has_auth"), Some(&Value::Bool(true)));
    }

    #[test]
    fn test_is_imported_false_for_non_imported() {
        let opts = ParseOptions {
            root_dir: std::path::PathBuf::from("/project"),
            ..ParseOptions::default()
        };
        let source = "has_auth = is_imported(\"./auth.wcl\")";
        let doc = parse(source, opts);
        assert!(!doc.has_errors(), "errors: {:?}", doc.errors());
        assert_eq!(doc.values.get("has_auth"), Some(&Value::Bool(false)));
    }

    // ── Partial let bindings ────────────────────────────────────────────

    #[test]
    fn test_partial_let_concatenates_lists() {
        let source = r#"
            partial let tags = ["api", "public"]
            partial let tags = ["v2"]
            all_tags = tags
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(!doc.has_errors(), "errors: {:?}", doc.errors());
        if let Some(Value::List(items)) = doc.values.get("all_tags") {
            assert_eq!(items.len(), 3);
        } else {
            panic!("expected list, got {:?}", doc.values.get("all_tags"));
        }
    }

    #[test]
    fn test_partial_let_three_fragments() {
        let source = r#"
            partial let items = [1]
            partial let items = [2]
            partial let items = [3]
            count = len(items)
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(!doc.has_errors(), "errors: {:?}", doc.errors());
        assert_eq!(doc.values.get("count"), Some(&Value::Int(3)));
    }

    #[test]
    fn test_partial_let_single_clears_flag() {
        let source = r#"
            partial let tags = ["api"]
            t = tags
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(!doc.has_errors(), "errors: {:?}", doc.errors());
        assert_eq!(
            doc.values.get("t"),
            Some(&Value::List(vec![Value::String("api".to_string())]))
        );
    }

    #[test]
    fn test_partial_let_non_list_emits_e038() {
        let source = "partial let x = 42";
        let doc = parse(source, ParseOptions::default());
        let e038: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E038"))
            .collect();
        assert_eq!(e038.len(), 1);
    }

    #[test]
    fn test_partial_let_mixed_emits_e039() {
        let source = r#"
            partial let tags = ["api"]
            let tags = ["fixed"]
        "#;
        let doc = parse(source, ParseOptions::default());
        let e039: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E039"))
            .collect();
        assert_eq!(e039.len(), 1);
    }

    // ── For loops on tables ─────────────────────────────────────────────

    #[test]
    fn test_for_loop_over_inline_table() {
        let source = r#"
            table users {
                name : string
                role : string
                | "alice" | "admin" |
                | "bob"   | "user"  |
            }

            for user in users {
                service ${user.name}-svc {
                    owner = user.name
                    role  = user.role
                }
            }
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(
            doc.diagnostics.is_empty(),
            "expected no errors, got: {:?}",
            doc.diagnostics
        );

        // Should have two service blocks (values are BlockRef at top level)
        let alice_svc = doc.values.get("alice-svc");
        let bob_svc = doc.values.get("bob-svc");
        assert!(alice_svc.is_some(), "expected alice-svc block");
        assert!(bob_svc.is_some(), "expected bob-svc block");

        // BlockRef attributes are accessible via the block ref
        let alice = alice_svc
            .unwrap()
            .as_block_ref()
            .expect("expected block ref");
        assert_eq!(
            alice.attributes.get("owner"),
            Some(&Value::String("alice".to_string()))
        );
        assert_eq!(
            alice.attributes.get("role"),
            Some(&Value::String("admin".to_string()))
        );

        let bob = bob_svc.unwrap().as_block_ref().expect("expected block ref");
        assert_eq!(
            bob.attributes.get("owner"),
            Some(&Value::String("bob".to_string()))
        );
        assert_eq!(
            bob.attributes.get("role"),
            Some(&Value::String("user".to_string()))
        );
    }

    #[test]
    fn test_for_loop_over_import_table() {
        use std::sync::Arc;

        let mut fs = InMemoryFs::new();
        fs.add_file(
            std::path::PathBuf::from("/project/users.csv"),
            "name,role\nalice,admin\nbob,user",
        );

        let mut opts = ParseOptions::default();
        opts.root_dir = std::path::PathBuf::from("/project");
        opts.fs = Some(Arc::new(fs));

        let source = r#"
            table users = import_table("users.csv")

            for user in users {
                service ${user.name}-svc {
                    role = user.role
                }
            }
        "#;
        let doc = parse(source, opts);
        assert!(
            doc.diagnostics.is_empty(),
            "expected no errors, got: {:?}",
            doc.diagnostics
        );

        let alice_svc = doc.values.get("alice-svc");
        assert!(
            alice_svc.is_some(),
            "expected alice-svc block, got keys: {:?}",
            doc.values.keys().collect::<Vec<_>>()
        );
        let alice = alice_svc
            .unwrap()
            .as_block_ref()
            .expect("expected block ref");
        assert_eq!(
            alice.attributes.get("role"),
            Some(&Value::String("admin".to_string()))
        );
    }

    #[test]
    fn test_for_loop_over_inline_table_with_index() {
        let source = r#"
            table items {
                label : string
                | "a" |
                | "b" |
                | "c" |
            }

            for item, idx in items {
                entry ${item.label}-${idx} {
                    pos = idx
                }
            }
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(
            doc.diagnostics.is_empty(),
            "expected no errors, got: {:?}",
            doc.diagnostics
        );

        assert!(
            doc.values.get("a-0").is_some(),
            "expected a-0 block, got: {:?}",
            doc.values.keys().collect::<Vec<_>>()
        );
        assert!(doc.values.get("b-1").is_some(), "expected b-1 block");
        assert!(doc.values.get("c-2").is_some(), "expected c-2 block");
    }

    // ── Let-bound import_table + for loops ────────────────────────────

    fn has_errors(diags: &[crate::Diagnostic]) -> bool {
        diags
            .iter()
            .any(|d| d.severity == crate::lang::diagnostic::Severity::Error)
    }

    fn error_diags(diags: &[crate::Diagnostic]) -> Vec<&crate::Diagnostic> {
        diags
            .iter()
            .filter(|d| d.severity == crate::lang::diagnostic::Severity::Error)
            .collect()
    }

    #[test]
    fn test_for_loop_over_let_import_table() {
        use std::sync::Arc;

        let mut fs = InMemoryFs::new();
        fs.add_file(
            std::path::PathBuf::from("/project/users.csv"),
            "name,role\nalice,admin\nbob,user",
        );

        let mut opts = ParseOptions::default();
        opts.root_dir = std::path::PathBuf::from("/project");
        opts.fs = Some(Arc::new(fs));

        let source = r#"
            let data = import_table("users.csv")

            for row in data {
                service ${row.name}-svc {
                    role = row.role
                }
            }
        "#;
        let doc = parse(source, opts);
        assert!(
            !has_errors(&doc.diagnostics),
            "expected no errors, got: {:?}",
            error_diags(&doc.diagnostics)
        );

        let alice_svc = doc.values.get("alice-svc");
        assert!(
            alice_svc.is_some(),
            "expected alice-svc block, got keys: {:?}",
            doc.values.keys().collect::<Vec<_>>()
        );
        let alice = alice_svc
            .unwrap()
            .as_block_ref()
            .expect("expected block ref");
        assert_eq!(
            alice.attributes.get("role"),
            Some(&Value::String("admin".to_string()))
        );
    }

    #[test]
    fn test_let_import_table_not_in_output() {
        use std::sync::Arc;

        let mut fs = InMemoryFs::new();
        fs.add_file(
            std::path::PathBuf::from("/project/data.csv"),
            "name,value\nalice,42",
        );

        let mut opts = ParseOptions::default();
        opts.root_dir = std::path::PathBuf::from("/project");
        opts.fs = Some(Arc::new(fs));

        let source = r#"
            let data = import_table("data.csv")
            x = 1
        "#;
        let doc = parse(source, opts);
        assert!(
            !has_errors(&doc.diagnostics),
            "expected no errors, got: {:?}",
            error_diags(&doc.diagnostics)
        );

        // let bindings should not appear in output values
        assert!(
            !doc.values.contains_key("data"),
            "let-bound table should not appear in output"
        );
        assert_eq!(doc.values.get("x"), Some(&Value::Int(1)));
    }

    #[test]
    fn test_find_on_table() {
        use std::sync::Arc;

        let mut fs = InMemoryFs::new();
        fs.add_file(
            std::path::PathBuf::from("/project/users.csv"),
            "name,role\nalice,admin\nbob,user",
        );

        let mut opts = ParseOptions::default();
        opts.root_dir = std::path::PathBuf::from("/project");
        opts.fs = Some(Arc::new(fs));

        let source = r#"
            let data = import_table("users.csv")
            let row = find(data, "name", "alice")
            result = row.role
        "#;
        let doc = parse(source, opts);
        assert!(
            !has_errors(&doc.diagnostics),
            "expected no errors, got: {:?}",
            error_diags(&doc.diagnostics)
        );
        assert_eq!(
            doc.values.get("result"),
            Some(&Value::String("admin".to_string()))
        );
    }

    #[test]
    fn test_filter_lambda_on_table() {
        use std::sync::Arc;

        let mut fs = InMemoryFs::new();
        fs.add_file(
            std::path::PathBuf::from("/project/users.csv"),
            "name,role\nalice,admin\nbob,user\ncharlie,admin",
        );

        let mut opts = ParseOptions::default();
        opts.root_dir = std::path::PathBuf::from("/project");
        opts.fs = Some(Arc::new(fs));

        let source = r#"
            let data = import_table("users.csv")
            let admins = filter(data, (r) => r.role == "admin")
            count = len(admins)
        "#;
        let doc = parse(source, opts);
        assert!(
            !has_errors(&doc.diagnostics),
            "expected no errors, got: {:?}",
            error_diags(&doc.diagnostics)
        );
        assert_eq!(doc.values.get("count"), Some(&Value::Int(2)));
    }

    #[test]
    fn test_insert_row_in_for_loop() {
        use std::sync::Arc;

        let mut fs = InMemoryFs::new();
        fs.add_file(
            std::path::PathBuf::from("/project/users.csv"),
            "name,role\nalice,admin",
        );

        let mut opts = ParseOptions::default();
        opts.root_dir = std::path::PathBuf::from("/project");
        opts.fs = Some(Arc::new(fs));

        let source = r#"
            let data = import_table("users.csv")
            let extended = insert_row(data, {name = "bob", role = "user"})

            for row in extended {
                service ${row.name}-svc {
                    role = row.role
                }
            }
        "#;
        let doc = parse(source, opts);
        assert!(
            !has_errors(&doc.diagnostics),
            "expected no errors, got: {:?}",
            error_diags(&doc.diagnostics)
        );

        assert!(doc.values.get("alice-svc").is_some(), "expected alice-svc");
        assert!(doc.values.get("bob-svc").is_some(), "expected bob-svc");
    }

    #[test]
    fn test_ref_decorator_parses() {
        let source = r#"
schema "endpoint" {
    service_ref: string @ref("service")
}

service api { port = 8080 }

endpoint e1 {
    service_ref = "api"
}
"#;
        let doc = parse(source, ParseOptions::default());
        assert!(
            !has_errors(&doc.diagnostics),
            "expected no errors but got: {:?}",
            error_diags(&doc.diagnostics)
        );
    }

    #[test]
    fn test_ref_across_import_no_false_e076() {
        let mut fs = InMemoryFs::new();
        fs.add_file(
            std::path::PathBuf::from("/project/services.wcl"),
            "service api { port = 8080 }",
        );

        let opts = ParseOptions {
            root_dir: std::path::PathBuf::from("/project"),
            fs: Some(Arc::new(fs)),
            ..ParseOptions::default()
        };

        let source = r#"
import "./services.wcl"

schema "endpoint" {
    service_ref: string @ref("service")
}

endpoint e1 {
    service_ref = "api"
}
"#;

        let doc = parse(source, opts);
        assert!(
            !has_errors(&doc.diagnostics),
            "expected no errors but got: {:?}",
            error_diags(&doc.diagnostics)
        );
    }

    // ── Struct definition parsing ─────────────────────────────────────────

    #[test]
    fn test_parse_struct_def() {
        let source = r#"
            struct "Point" {
                x : f64
                y : f64
            }
        "#;
        let (doc, diags) = crate::lang::parse(source, crate::lang::span::FileId(0));
        let parse_errors: Vec<_> = diags
            .into_diagnostics()
            .into_iter()
            .filter(|d| d.is_error())
            .collect();
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        assert_eq!(doc.items.len(), 1);
        if let ast::DocItem::Body(ast::BodyItem::StructDef(ref s)) = doc.items[0] {
            // Check name is "Point"
            assert_eq!(s.name.parts.len(), 1);
            if let ast::StringPart::Literal(ref name) = s.name.parts[0] {
                assert_eq!(name, "Point");
            } else {
                panic!("expected literal string part");
            }
            assert_eq!(s.fields.len(), 2);
            assert_eq!(s.fields[0].name.name, "x");
            assert_eq!(s.fields[1].name.name, "y");
            assert!(s.variants.is_empty());
        } else {
            panic!("expected StructDef, got {:?}", doc.items[0]);
        }
    }

    #[test]
    fn test_parse_struct_with_variants() {
        let source = r#"
            @tagged("type")
            struct "Section" {
                type : u32
                size : u64

                variant "1" {
                    data : list(u8)
                }
                variant "2" {
                    name : string
                }
            }
        "#;
        let (doc, diags) = crate::lang::parse(source, crate::lang::span::FileId(0));
        let parse_errors: Vec<_> = diags
            .into_diagnostics()
            .into_iter()
            .filter(|d| d.is_error())
            .collect();
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        if let ast::DocItem::Body(ast::BodyItem::StructDef(ref s)) = doc.items[0] {
            assert_eq!(s.fields.len(), 2);
            assert_eq!(s.variants.len(), 2);
            assert!(s.decorators.len() == 1);
            assert_eq!(s.decorators[0].name.name, "tagged");
        } else {
            panic!("expected StructDef");
        }
    }

    #[test]
    fn test_struct_type_in_schema() {
        let source = r#"
            struct "Point" {
                x : f64
                y : f64
            }
            schema "sprite" {
                position : Point @required
                name     : string @required
            }
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(
            !has_errors(&doc.diagnostics),
            "errors: {:?}",
            error_diags(&doc.diagnostics)
        );
    }

    #[test]
    fn test_pattern_type_in_schema() {
        let source = r#"
            schema "route" {
                path : pattern @required
                method : string @required
            }
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(
            !has_errors(&doc.diagnostics),
            "errors: {:?}",
            error_diags(&doc.diagnostics)
        );
    }

    #[test]
    fn test_plus_equals_attribute() {
        let source = r#"
            total += 1
        "#;
        let (doc, diags) = crate::lang::parse(source, crate::lang::span::FileId(0));
        let parse_errors: Vec<_> = diags
            .into_diagnostics()
            .into_iter()
            .filter(|d| d.is_error())
            .collect();
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        if let ast::DocItem::Body(ast::BodyItem::Attribute(ref attr)) = doc.items[0] {
            assert_eq!(attr.name.name, "total");
            assert_eq!(attr.assign_op, ast::AssignOp::AddAssign);
        } else {
            panic!("expected Attribute with +=");
        }
    }

    #[test]
    fn test_call_exported_function() {
        let source = r#"
            export let double = x => x * 2
        "#;
        let doc = parse(source, ParseOptions::default());
        assert!(
            !has_errors(&doc.diagnostics),
            "errors: {:?}",
            error_diags(&doc.diagnostics)
        );

        // Check that the function is listed
        let fn_names = doc.exported_function_names();
        assert!(
            fn_names.contains(&"double"),
            "expected 'double' in {:?}",
            fn_names
        );

        // Call the function
        let result = doc.call_function("double", &[Value::Int(21)]).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_call_function_wrong_args() {
        let source = r#"
            export let add = (a, b) => a + b
        "#;
        let doc = parse(source, ParseOptions::default());
        let result = doc.call_function("add", &[Value::Int(1)]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects 2 argument(s), got 1"));
    }

    #[test]
    fn test_call_function_not_found() {
        let source = r#"
            name = "hello"
        "#;
        let doc = parse(source, ParseOptions::default());
        let result = doc.call_function("missing", &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_decorated_export_let() {
        let source = r#"
            @stateful
            export let my_fn = x => x + 1
        "#;
        let (doc, diags) = crate::lang::parse(source, crate::lang::span::FileId(0));
        let parse_errors: Vec<_> = diags
            .into_diagnostics()
            .into_iter()
            .filter(|d| d.is_error())
            .collect();
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        if let ast::DocItem::ExportLet(ref el) = doc.items[0] {
            assert_eq!(el.decorators.len(), 1);
            assert_eq!(el.decorators[0].name.name, "stateful");
            assert_eq!(el.name.name, "my_fn");
        } else {
            panic!("expected ExportLet, got {:?}", doc.items[0]);
        }
    }

    // ── Namespace integration tests ──────────────────────────────────────

    #[test]
    fn test_namespace_braced_evaluates() {
        let src = r#"
namespace networking {
    service web {
        port = 8080
    }
}
"#;
        let doc = parse(src, ParseOptions::default());
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
        let val = doc.values.get("web");
        assert!(
            val.is_some(),
            "expected 'web' in values, got: {:?}",
            doc.values.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_namespace_file_level_evaluates() {
        let src = r#"
namespace myns

service api {
    port = 3000
}
"#;
        let doc = parse(src, ParseOptions::default());
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
        let val = doc.values.get("api");
        assert!(
            val.is_some(),
            "expected 'api' in values, got: {:?}",
            doc.values.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_namespace_schema_validation() {
        let src = r#"
namespace net {
    schema "service" {
        port: i64
    }

    service "web" {
        port = 8080
    }
}
"#;
        let doc = parse(src, ParseOptions::default());
        // Filter out any non-E071 errors (type mismatch false positives are a separate issue)
        let real_errors: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.severity == crate::lang::diagnostic::Severity::Error)
            .collect();
        assert!(real_errors.is_empty(), "errors: {:?}", real_errors);
    }

    #[test]
    fn test_namespace_qualified_access_in_expr() {
        let src = r#"
namespace config {
    let base_port = 8000
}

service "web" {
    port = config::base_port + 80
}
"#;
        let doc = parse(src, ParseOptions::default());
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
    }

    #[test]
    fn test_use_unknown_namespace_error() {
        let src = r#"
use nonexistent::thing
"#;
        let doc = parse(src, ParseOptions::default());
        let errors: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E121"))
            .collect();
        assert_eq!(errors.len(), 1, "expected E121: {:?}", doc.diagnostics);
    }

    #[test]
    fn test_use_unknown_target_error() {
        let src = r#"
namespace net {
    schema "service" {
        port: i64
    }
}

use net::nonexistent
"#;
        let doc = parse(src, ParseOptions::default());
        let errors: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E120"))
            .collect();
        assert_eq!(errors.len(), 1, "expected E120: {:?}", doc.diagnostics);
    }

    // ── Heredoc in table cells ───────────────────────────────────────────

    #[test]
    fn table_heredoc_basic() {
        let src = r#"table docs {
  title : string
  body : string
  | "Setup" | <<-EOF
    Hello world
    EOF
  |
}"#;
        let doc = parse(src, ParseOptions::default());
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
        let json = crate::json::values_to_json(&doc.values);
        let rows = json["docs"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["title"], "Setup");
        assert_eq!(rows[0]["body"], "Hello world");
    }

    #[test]
    fn table_heredoc_non_indented() {
        let src = "table t {\n  col : string\n  | <<EOF\nline1\nline2\nEOF\n  |\n}";
        let doc = parse(src, ParseOptions::default());
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
        let json = crate::json::values_to_json(&doc.values);
        let rows = json["t"].as_array().unwrap();
        assert_eq!(rows[0]["col"], "line1\nline2");
    }

    #[test]
    fn table_heredoc_raw() {
        let src = "table t {\n  col : string\n  | <<'RAW'\nno ${interp}\nRAW\n  |\n}";
        let doc = parse(src, ParseOptions::default());
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
        let json = crate::json::values_to_json(&doc.values);
        let rows = json["t"].as_array().unwrap();
        assert_eq!(rows[0]["col"], "no ${interp}");
    }

    #[test]
    fn table_heredoc_multiple_rows() {
        let src = r#"table t {
  name : string
  desc : string
  | "a" | <<-EOF
    first
    EOF
  |
  | "b" | <<-EOF
    second
    EOF
  |
}"#;
        let doc = parse(src, ParseOptions::default());
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
        let json = crate::json::values_to_json(&doc.values);
        let rows = json["t"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["desc"], "first");
        assert_eq!(rows[1]["desc"], "second");
    }

    #[test]
    fn table_heredoc_mixed_cells() {
        let src = r#"table t {
  a : string
  b : i32
  c : string
  | <<-EOF
    text
    EOF
  | 42 | "hello" |
}"#;
        let doc = parse(src, ParseOptions::default());
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
        let json = crate::json::values_to_json(&doc.values);
        let rows = json["t"].as_array().unwrap();
        assert_eq!(rows[0]["a"], "text");
        assert_eq!(rows[0]["b"], 42);
        assert_eq!(rows[0]["c"], "hello");
    }

    #[test]
    fn table_heredoc_with_interpolation() {
        let src = r#"let name = "world"
table t {
  col : string
  | <<EOF
hello ${name}!
EOF
  |
}"#;
        let doc = parse(src, ParseOptions::default());
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
        let json = crate::json::values_to_json(&doc.values);
        let rows = json["t"].as_array().unwrap();
        assert_eq!(rows[0]["col"], "hello world!");
    }

    #[test]
    fn table_heredoc_preserves_tag() {
        let src = "table t {\n  col : string\n  | <<SCRIPT\nhello\nSCRIPT\n  |\n}";
        let doc = parse(src, ParseOptions::default());
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
        // Verify the AST preserves the tag
        if let Some(ast::DocItem::Body(ast::BodyItem::Table(table))) = doc.ast.items.first() {
            if let Some(row) = table.rows.first() {
                if let ast::Expr::StringLit(s) = &row.cells[0] {
                    let info = s.heredoc.as_ref().expect("should have heredoc info");
                    assert_eq!(info.tag, "SCRIPT");
                } else {
                    panic!("expected StringLit");
                }
            }
        }
    }

    #[test]
    fn test_lazy_import_loaded_when_referenced() {
        let mut fs = InMemoryFs::new();
        fs.add_file(
            std::path::PathBuf::from("/project/lib.wcl"),
            "schema \"service\" { port: i64 }".to_string(),
        );
        let opts = ParseOptions {
            root_dir: std::path::PathBuf::from("/project"),
            fs: Some(std::sync::Arc::new(fs)),
            ..Default::default()
        };
        let source = r#"
import "./lib.wcl" lazy(net)
use net::{service}
service "my-api" { port = 8080 }
"#;
        let doc = parse(source, opts);
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
    }

    #[test]
    fn test_lazy_import_not_loaded_when_unreferenced() {
        // The file doesn't exist, but since it's lazy and unreferenced, no error
        let fs = InMemoryFs::new();
        let opts = ParseOptions {
            root_dir: std::path::PathBuf::from("/project"),
            fs: Some(std::sync::Arc::new(fs)),
            ..Default::default()
        };
        let source = r#"
import "./nonexistent.wcl" lazy(unused)
config { port = 8080 }
"#;
        let doc = parse(source, opts);
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
    }

    #[test]
    fn test_lazy_import_with_nested_namespace() {
        let mut fs = InMemoryFs::new();
        fs.add_file(
            std::path::PathBuf::from("/project/lib.wcl"),
            "export let port = 9090".to_string(),
        );
        let opts = ParseOptions {
            root_dir: std::path::PathBuf::from("/project"),
            fs: Some(std::sync::Arc::new(fs)),
            ..Default::default()
        };
        let source = r#"
import "./lib.wcl" lazy(infra::net)
use infra::net::{port}
config { listen_port = port }
"#;
        let doc = parse(source, opts);
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
    }

    #[test]
    fn test_lazy_import_optional_missing_file() {
        let fs = InMemoryFs::new();
        let opts = ParseOptions {
            root_dir: std::path::PathBuf::from("/project"),
            fs: Some(std::sync::Arc::new(fs)),
            ..Default::default()
        };
        // Optional lazy import: referenced but file missing — should not error
        let source = r#"
import? "./missing.wcl" lazy(opt)
use opt::{thing}
config { port = 8080 }
"#;
        let doc = parse(source, opts);
        // The use declaration will fail since namespace doesn't exist,
        // but the import itself should not produce an E010 error
        let import_errors: Vec<_> = doc
            .diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("E010"))
            .collect();
        assert!(
            import_errors.is_empty(),
            "should not have file-not-found error for optional lazy import"
        );
    }

    #[test]
    fn test_lazy_import_use_triggers_load() {
        let mut fs = InMemoryFs::new();
        fs.add_file(
            std::path::PathBuf::from("/project/lib.wcl"),
            "schema \"endpoint\" { path: string }".to_string(),
        );
        let opts = ParseOptions {
            root_dir: std::path::PathBuf::from("/project"),
            fs: Some(std::sync::Arc::new(fs)),
            ..Default::default()
        };
        // `use net::endpoint` should trigger lazy loading
        let source = r#"
import "./lib.wcl" lazy(net)
use net::{endpoint}
endpoint "api" { path = "/api" }
"#;
        let doc = parse(source, opts);
        assert!(!doc.has_errors(), "errors: {:?}", doc.diagnostics);
    }
}
