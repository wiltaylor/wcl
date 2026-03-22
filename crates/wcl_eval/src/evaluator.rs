use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use indexmap::IndexMap;
use wcl_core::ast::*;
use wcl_core::diagnostic::{Diagnostic, DiagnosticBag};
use wcl_core::span::Span;

use crate::functions::{builtin_registry, BuiltinFn, FunctionRegistry};
use crate::imports::FileSystem;
use crate::scope::*;
use crate::value::*;

pub struct Evaluator {
    scopes: ScopeArena,
    builtins: HashMap<String, BuiltinFn>,
    diagnostics: DiagnosticBag,
    fs: Option<Box<dyn FileSystem>>,
    base_dir: Option<PathBuf>,
    /// Maps (parent_scope, block_name) -> child_scope_id for block evaluation
    block_scope_map: HashMap<(ScopeId, String), ScopeId>,
    /// Set of function names declared via `declare` in library imports
    declared_functions: HashSet<String>,
    /// External variables to inject before evaluation
    variables: IndexMap<String, Value>,
    /// Files that were actually imported (for `is_imported()` introspection)
    imported_paths: HashSet<PathBuf>,
    /// Schema names declared in the document (for `has_schema()` introspection)
    schema_names: HashSet<String>,
}

impl Evaluator {
    pub fn new() -> Self {
        Evaluator {
            scopes: ScopeArena::new(),
            builtins: builtin_registry(),
            diagnostics: DiagnosticBag::new(),
            fs: None,
            base_dir: None,
            block_scope_map: HashMap::new(),
            declared_functions: HashSet::new(),
            variables: IndexMap::new(),
            imported_paths: HashSet::new(),
            schema_names: HashSet::new(),
        }
    }

    pub fn with_fs(fs: Box<dyn FileSystem>, base_dir: PathBuf) -> Self {
        Evaluator {
            scopes: ScopeArena::new(),
            builtins: builtin_registry(),
            diagnostics: DiagnosticBag::new(),
            fs: Some(fs),
            base_dir: Some(base_dir),
            block_scope_map: HashMap::new(),
            declared_functions: HashSet::new(),
            variables: IndexMap::new(),
            imported_paths: HashSet::new(),
            schema_names: HashSet::new(),
        }
    }

    /// Create an evaluator with custom functions from a `FunctionRegistry`.
    pub fn with_functions(
        registry: &FunctionRegistry,
        fs: Option<Box<dyn FileSystem>>,
        base_dir: Option<PathBuf>,
    ) -> Self {
        let mut builtins = builtin_registry();
        for (name, f) in &registry.functions {
            builtins.insert(name.clone(), f.clone());
        }
        Evaluator {
            scopes: ScopeArena::new(),
            builtins,
            diagnostics: DiagnosticBag::new(),
            fs,
            base_dir,
            block_scope_map: HashMap::new(),
            declared_functions: HashSet::new(),
            variables: IndexMap::new(),
            imported_paths: HashSet::new(),
            schema_names: HashSet::new(),
        }
    }

    /// Register a custom function at runtime.
    pub fn register_function(&mut self, name: impl Into<String>, f: BuiltinFn) {
        self.builtins.insert(name.into(), f);
    }

    /// Set external variables to inject before evaluation.
    pub fn set_variables(&mut self, vars: IndexMap<String, Value>) {
        self.variables = vars;
    }

    /// Set the imported file paths (for `is_imported()` introspection).
    pub fn set_imported_paths(&mut self, paths: HashSet<PathBuf>) {
        self.imported_paths = paths;
    }

    /// Set the schema names (for `has_schema()` introspection).
    pub fn set_schema_names(&mut self, names: HashSet<String>) {
        self.schema_names = names;
    }

    /// Add a declared function name (from `declare` statements in library imports).
    pub fn add_declared_function(&mut self, name: impl Into<String>) {
        self.declared_functions.insert(name.into());
    }

    /// Evaluate a full document. Returns the evaluated document as a list of
    /// (key, Value) pairs representing the resolved content.
    pub fn evaluate(&mut self, doc: &Document) -> IndexMap<String, Value> {
        let module_scope = self.scopes.create_scope(ScopeKind::Module, None);

        // Phase 1: Register all names in scope (let bindings, attributes, blocks)
        self.register_doc_items(&doc.items, module_scope);

        // Inject external variables (after doc items so they override defaults)
        for (name, value) in &self.variables {
            self.scopes.add_entry(
                module_scope,
                ScopeEntry {
                    name: name.clone(),
                    kind: ScopeEntryKind::LetBinding,
                    value: Some(value.clone()),
                    span: Span::dummy(),
                    dependencies: HashSet::new(),
                    evaluated: true,
                    read_count: 0,
                },
            );
        }

        // Phase 2: Topological sort within scope
        match self.scopes.topo_sort(module_scope) {
            Ok(order) => {
                // Phase 3: Evaluate in dependency order.
                // We need the original AST items to evaluate, so we walk the
                // doc items for each name in topo order.
                for name in &order {
                    self.evaluate_doc_entry(&doc.items, module_scope, name);
                }
            }
            Err(cycle) => {
                self.diagnostics.error_with_code(
                    format!("cyclic dependency detected: {}", cycle.join(" -> ")),
                    Span::dummy(),
                    "E041",
                );
            }
        }

        // Check for unused variables (W002)
        self.check_unused_variables();

        // Collect evaluated values (skip let bindings for serde output)
        self.collect_output(module_scope)
    }

    // ------------------------------------------------------------------
    // Registration: walk the AST and populate scope entries (unevaluated)
    // ------------------------------------------------------------------

    fn register_doc_items(&mut self, items: &[DocItem], scope_id: ScopeId) {
        for item in items {
            match item {
                DocItem::Body(body_item) => self.register_body_item(body_item, scope_id),
                DocItem::ExportLet(el) => {
                    let deps = self.find_dependencies(&el.value);
                    self.scopes.add_entry(
                        scope_id,
                        ScopeEntry {
                            name: el.name.name.clone(),
                            kind: ScopeEntryKind::ExportLet,
                            value: None,
                            span: el.span,
                            dependencies: deps,
                            evaluated: false,
                            read_count: 0,
                        },
                    );
                }
                DocItem::FunctionDecl(decl) => {
                    // Track declared function names so we can give a helpful error
                    // if they are called but not registered by the host application.
                    self.declared_functions.insert(decl.name.name.clone());
                }
                _ => {}
            }
        }
    }

    fn register_body_item(&mut self, item: &BodyItem, scope_id: ScopeId) {
        match item {
            BodyItem::Attribute(attr) => {
                let deps = self.find_dependencies(&attr.value);
                self.scopes.add_entry(
                    scope_id,
                    ScopeEntry {
                        name: attr.name.name.clone(),
                        kind: ScopeEntryKind::Attribute,
                        value: None,
                        span: attr.span,
                        dependencies: deps,
                        evaluated: false,
                        read_count: 0,
                    },
                );
            }
            BodyItem::LetBinding(lb) => {
                // Check for shadowing (W001)
                if let Some(shadowed_span) = self.scopes.check_shadowing(scope_id, &lb.name.name) {
                    if !has_allow_decorator(&lb.decorators, "shadowing") {
                        self.diagnostics.add(
                            wcl_core::diagnostic::Diagnostic::warning(
                                format!(
                                    "variable '{}' shadows a binding in an outer scope",
                                    lb.name.name
                                ),
                                lb.span,
                            )
                            .with_code("W001")
                            .with_label(shadowed_span, "previously defined here"),
                        );
                    }
                }
                let deps = self.find_dependencies(&lb.value);
                self.scopes.add_entry(
                    scope_id,
                    ScopeEntry {
                        name: lb.name.name.clone(),
                        kind: ScopeEntryKind::LetBinding,
                        value: None,
                        span: lb.span,
                        dependencies: deps,
                        evaluated: false,
                        read_count: 0,
                    },
                );
            }
            BodyItem::Block(block) => {
                let child_scope = self.scopes.create_scope(ScopeKind::Block, Some(scope_id));
                let name = block
                    .inline_id
                    .as_ref()
                    .map(|id| match id {
                        InlineId::Literal(lit) => lit.value.clone(),
                        InlineId::Interpolated(_) => "?interpolated?".to_string(),
                    })
                    .unwrap_or_else(|| format!("__block_{}", block.kind.name));
                self.block_scope_map
                    .insert((scope_id, name.clone()), child_scope);
                // Collect external dependencies: names referenced inside the
                // block body that are not defined within the block itself.
                let mut external_deps = self.collect_block_external_deps(&block.body);
                // Also collect deps from text_content interpolations
                if let Some(ref tc) = block.text_content {
                    self.collect_string_lit_deps(tc, &mut external_deps);
                }
                self.scopes.add_entry(
                    scope_id,
                    ScopeEntry {
                        name,
                        kind: ScopeEntryKind::BlockChild,
                        value: None,
                        span: block.span,
                        dependencies: external_deps,
                        evaluated: false,
                        read_count: 0,
                    },
                );
                self.register_block_body(&block.body, child_scope);
            }
            BodyItem::Table(table) => {
                let name = table
                    .inline_id
                    .as_ref()
                    .map(|id| match id {
                        InlineId::Literal(lit) => lit.value.clone(),
                        InlineId::Interpolated(_) => "?interpolated?".to_string(),
                    })
                    .unwrap_or_else(|| "__table".to_string());
                // Check for name collision with existing non-table entries
                if self.scopes.get(scope_id).entries.contains_key(&name) {
                    let existing = &self.scopes.get(scope_id).entries[&name];
                    if existing.kind != ScopeEntryKind::TableEntry {
                        self.diagnostics.error_with_code(
                            format!(
                                "table '{}' conflicts with an existing {} of the same name",
                                name,
                                match existing.kind {
                                    ScopeEntryKind::Attribute => "attribute",
                                    ScopeEntryKind::LetBinding => "let binding",
                                    ScopeEntryKind::BlockChild => "block",
                                    _ => "entry",
                                }
                            ),
                            table.span,
                            "E030",
                        );
                    }
                }
                let mut deps = HashSet::new();
                // Collect deps from cell expressions
                for row in &table.rows {
                    for cell in &row.cells {
                        deps.extend(self.find_dependencies(cell));
                    }
                }
                // Collect deps from import_expr
                if let Some(ref expr) = table.import_expr {
                    deps.extend(self.find_dependencies(expr));
                }
                self.scopes.add_entry(
                    scope_id,
                    ScopeEntry {
                        name,
                        kind: ScopeEntryKind::TableEntry,
                        value: None,
                        span: table.span,
                        dependencies: deps,
                        evaluated: false,
                        read_count: 0,
                    },
                );
            }
            _ => {}
        }
    }

    fn register_block_body(&mut self, body: &[BodyItem], scope_id: ScopeId) {
        for item in body {
            self.register_body_item(item, scope_id);
        }
    }

    /// Collect names referenced inside a block body that are NOT defined within
    /// the block itself. These become the block's external dependencies so the
    /// topo sort in the parent scope evaluates them first.
    fn collect_block_external_deps(&self, body: &[BodyItem]) -> HashSet<String> {
        let mut all_deps = HashSet::new();
        let mut local_names = HashSet::new();

        for item in body {
            match item {
                BodyItem::Attribute(attr) => {
                    local_names.insert(attr.name.name.clone());
                    let deps = self.find_dependencies(&attr.value);
                    all_deps.extend(deps);
                }
                BodyItem::LetBinding(lb) => {
                    local_names.insert(lb.name.name.clone());
                    let deps = self.find_dependencies(&lb.value);
                    all_deps.extend(deps);
                }
                BodyItem::Block(block) => {
                    let bname = block
                        .inline_id
                        .as_ref()
                        .map(|id| match id {
                            InlineId::Literal(lit) => lit.value.clone(),
                            InlineId::Interpolated(_) => "?interpolated?".to_string(),
                        })
                        .unwrap_or_else(|| format!("__block_{}", block.kind.name));
                    local_names.insert(bname);
                    // Recurse into nested block bodies
                    let nested = self.collect_block_external_deps(&block.body);
                    all_deps.extend(nested);
                }
                BodyItem::Table(table) => {
                    let tname = table
                        .inline_id
                        .as_ref()
                        .map(|id| match id {
                            InlineId::Literal(lit) => lit.value.clone(),
                            InlineId::Interpolated(_) => "?interpolated?".to_string(),
                        })
                        .unwrap_or_else(|| "__table".to_string());
                    local_names.insert(tname);
                    for row in &table.rows {
                        for cell in &row.cells {
                            all_deps.extend(self.find_dependencies(cell));
                        }
                    }
                    if let Some(ref expr) = table.import_expr {
                        all_deps.extend(self.find_dependencies(expr));
                    }
                }
                _ => {}
            }
        }

        // Remove locally defined names — only keep external references
        all_deps.retain(|dep| !local_names.contains(dep));
        all_deps
    }

    // ------------------------------------------------------------------
    // Evaluate a named entry from the document items
    // ------------------------------------------------------------------

    fn evaluate_doc_entry(&mut self, items: &[DocItem], scope_id: ScopeId, name: &str) {
        // Skip entries already evaluated (e.g., injected external variables)
        if let Some((_, entry)) = self.scopes.resolve(scope_id, name) {
            if entry.evaluated {
                return;
            }
        }

        // Find the AST node that corresponds to this name
        for item in items {
            match item {
                DocItem::Body(BodyItem::Attribute(attr)) if attr.name.name == name => {
                    let val = self.eval_expr(&attr.value, scope_id);
                    match val {
                        Ok(v) => {
                            if let Some((_, entry)) = self.scopes.resolve_mut(scope_id, name) {
                                entry.value = Some(v);
                                entry.evaluated = true;
                            }
                        }
                        Err(diag) => self.diagnostics.add(diag),
                    }
                    return;
                }
                DocItem::Body(BodyItem::LetBinding(lb)) if lb.name.name == name => {
                    let val = self.eval_expr(&lb.value, scope_id);
                    match val {
                        Ok(v) => {
                            if let Some((_, entry)) = self.scopes.resolve_mut(scope_id, name) {
                                entry.value = Some(v);
                                entry.evaluated = true;
                            }
                        }
                        Err(diag) => self.diagnostics.add(diag),
                    }
                    return;
                }
                DocItem::ExportLet(el) if el.name.name == name => {
                    let val = self.eval_expr(&el.value, scope_id);
                    match val {
                        Ok(v) => {
                            if let Some((_, entry)) = self.scopes.resolve_mut(scope_id, name) {
                                entry.value = Some(v);
                                entry.evaluated = true;
                            }
                        }
                        Err(diag) => self.diagnostics.add(diag),
                    }
                    return;
                }
                DocItem::Body(BodyItem::Block(block)) => {
                    let block_name = block
                        .inline_id
                        .as_ref()
                        .map(|id| match id {
                            InlineId::Literal(lit) => lit.value.clone(),
                            InlineId::Interpolated(_) => "?interpolated?".to_string(),
                        })
                        .unwrap_or_else(|| format!("__block_{}", block.kind.name));
                    if block_name == name {
                        // Evaluate the block's child scope and build a BlockRef
                        let block_ref = self.build_block_ref(block, scope_id);
                        if let Some((_, entry)) = self.scopes.resolve_mut(scope_id, name) {
                            entry.value = Some(Value::BlockRef(block_ref));
                            entry.evaluated = true;
                        }
                        return;
                    }
                }
                DocItem::Body(BodyItem::Table(table)) => {
                    let table_name = table
                        .inline_id
                        .as_ref()
                        .map(|id| match id {
                            InlineId::Literal(lit) => lit.value.clone(),
                            InlineId::Interpolated(_) => "?interpolated?".to_string(),
                        })
                        .unwrap_or_else(|| "__table".to_string());
                    if table_name == name {
                        match self.eval_inline_table(table, scope_id) {
                            Ok(v) => {
                                if let Some((_, entry)) = self.scopes.resolve_mut(scope_id, name) {
                                    entry.value = Some(v);
                                    entry.evaluated = true;
                                }
                            }
                            Err(diag) => self.diagnostics.add(diag),
                        }
                        return;
                    }
                }
                _ => {}
            }
        }
    }

    // ------------------------------------------------------------------
    // Dependency analysis
    // ------------------------------------------------------------------

    /// Find all name references in an expression (for dependency tracking).
    fn find_dependencies(&self, expr: &Expr) -> HashSet<String> {
        let mut deps = HashSet::new();
        self.collect_deps(expr, &mut deps);
        deps
    }

    fn collect_string_lit_deps(&self, s: &StringLit, deps: &mut HashSet<String>) {
        for part in &s.parts {
            if let StringPart::Interpolation(expr) = part {
                self.collect_deps(expr, deps);
            }
        }
    }

    fn collect_deps(&self, expr: &Expr, deps: &mut HashSet<String>) {
        match expr {
            Expr::Ident(id) => {
                deps.insert(id.name.clone());
            }
            Expr::BinaryOp(l, _, r, _) => {
                self.collect_deps(l, deps);
                self.collect_deps(r, deps);
            }
            Expr::UnaryOp(_, e, _) => {
                self.collect_deps(e, deps);
            }
            Expr::Ternary(c, t, f, _) => {
                self.collect_deps(c, deps);
                self.collect_deps(t, deps);
                self.collect_deps(f, deps);
            }
            Expr::MemberAccess(e, _, _) => {
                self.collect_deps(e, deps);
            }
            Expr::IndexAccess(e, i, _) => {
                self.collect_deps(e, deps);
                self.collect_deps(i, deps);
            }
            Expr::FnCall(callee, args, _) => {
                self.collect_deps(callee, deps);
                for arg in args {
                    match arg {
                        CallArg::Positional(e) | CallArg::Named(_, e) => {
                            self.collect_deps(e, deps);
                        }
                    }
                }
            }
            Expr::Lambda(_, body, _) => {
                self.collect_deps(body, deps);
            }
            Expr::BlockExpr(lets, final_expr, _) => {
                for lb in lets {
                    self.collect_deps(&lb.value, deps);
                }
                self.collect_deps(final_expr, deps);
            }
            Expr::List(items, _) => {
                for e in items {
                    self.collect_deps(e, deps);
                }
            }
            Expr::Map(entries, _) => {
                for (_, v) in entries {
                    self.collect_deps(v, deps);
                }
            }
            Expr::StringLit(s) => {
                for part in &s.parts {
                    if let StringPart::Interpolation(e) = part {
                        self.collect_deps(e, deps);
                    }
                }
            }
            Expr::Paren(e, _) => {
                self.collect_deps(e, deps);
            }
            Expr::Query(pipeline, _) => {
                // Track selector dependencies (table/block names)
                match &pipeline.selector {
                    QuerySelector::TableId(id) => {
                        deps.insert(id.value.clone());
                    }
                    QuerySelector::KindId(_, id) => {
                        deps.insert(id.value.clone());
                    }
                    _ => {}
                }
                for filter in &pipeline.filters {
                    if let QueryFilter::AttrComparison(_, _, expr) = filter {
                        self.collect_deps(expr, deps);
                    }
                }
            }
            Expr::Ref(id, _) => {
                deps.insert(id.value.clone());
            }
            _ => {} // literals, etc.
        }
    }

    // ------------------------------------------------------------------
    // Expression evaluation
    // ------------------------------------------------------------------

    /// Evaluate an expression in a given scope, returning a Value.
    pub fn eval_expr(&mut self, expr: &Expr, scope_id: ScopeId) -> Result<Value, Diagnostic> {
        match expr {
            Expr::IntLit(i, _) => Ok(Value::Int(*i)),
            Expr::FloatLit(f, _) => Ok(Value::Float(*f)),
            Expr::BoolLit(b, _) => Ok(Value::Bool(*b)),
            Expr::NullLit(_) => Ok(Value::Null),
            Expr::StringLit(s) => self.eval_string_lit(s, scope_id),
            Expr::Ident(ident) => self.eval_ident(ident, scope_id),
            Expr::IdentifierLit(id) => Ok(Value::Identifier(id.value.clone())),
            Expr::SymbolLit(name, _) => Ok(Value::Symbol(name.clone())),
            Expr::List(items, _) => {
                let mut vals = Vec::with_capacity(items.len());
                for item in items {
                    vals.push(self.eval_expr(item, scope_id)?);
                }
                Ok(Value::List(vals))
            }
            Expr::Map(entries, _) => {
                let mut map = IndexMap::new();
                for (key, val) in entries {
                    let k = match key {
                        MapKey::Ident(id) => id.name.clone(),
                        MapKey::String(s) => self.eval_string_to_string(s, scope_id)?,
                    };
                    let v = self.eval_expr(val, scope_id)?;
                    map.insert(k, v);
                }
                Ok(Value::Map(map))
            }
            Expr::BinaryOp(lhs, op, rhs, span) => self.eval_binary(lhs, *op, rhs, *span, scope_id),
            Expr::UnaryOp(op, inner, span) => self.eval_unary(*op, inner, *span, scope_id),
            Expr::Ternary(cond, then_expr, else_expr, span) => {
                let cond_val = self.eval_expr(cond, scope_id)?;
                match cond_val {
                    Value::Bool(true) => self.eval_expr(then_expr, scope_id),
                    Value::Bool(false) => self.eval_expr(else_expr, scope_id),
                    _ => Err(Diagnostic::error(
                        format!(
                            "ternary condition must be bool, got {}",
                            cond_val.type_name()
                        ),
                        *span,
                    )
                    .with_code("E050")),
                }
            }
            Expr::MemberAccess(inner, field, span) => {
                let val = self.eval_expr(inner, scope_id)?;
                self.access_member(&val, &field.name, *span)
            }
            Expr::IndexAccess(inner, index, span) => {
                let val = self.eval_expr(inner, scope_id)?;
                let idx = self.eval_expr(index, scope_id)?;
                self.access_index(&val, &idx, *span)
            }
            Expr::FnCall(callee, args, span) => self.eval_fn_call(callee, args, *span, scope_id),
            Expr::Lambda(params, body, _span) => Ok(Value::Function(FunctionValue {
                params: params.iter().map(|p| p.name.clone()).collect(),
                body: FunctionBody::UserDefined(body.clone()),
                closure_scope: Some(scope_id),
            })),
            Expr::BlockExpr(lets, final_expr, _) => {
                let block_scope = self.scopes.create_scope(ScopeKind::Lambda, Some(scope_id));
                for lb in lets {
                    let val = self.eval_expr(&lb.value, block_scope)?;
                    self.scopes.add_entry(
                        block_scope,
                        ScopeEntry {
                            name: lb.name.name.clone(),
                            kind: ScopeEntryKind::LetBinding,
                            value: Some(val),
                            span: lb.span,
                            dependencies: Default::default(),
                            evaluated: true,
                            read_count: 0,
                        },
                    );
                }
                self.eval_expr(final_expr, block_scope)
            }
            Expr::Query(pipeline, span) => self.eval_query(pipeline, *span, scope_id),
            Expr::Ref(id, span) => {
                // Resolve block reference by identifier — search for a BlockChild
                // entry whose name matches and return its evaluated Value::BlockRef.
                let id_str = &id.value;
                if let Some((_, entry)) = self.scopes.resolve(scope_id, id_str) {
                    if entry.kind == ScopeEntryKind::BlockChild {
                        if let Some(ref val) = entry.value {
                            return Ok(val.clone());
                        }
                    }
                }
                Err(
                    Diagnostic::error(format!("ref: block with id '{}' not found", id_str), *span)
                        .with_code("E053"),
                )
            }
            Expr::ImportRaw(path, span) => {
                let path_str = self.eval_string_to_string(path, scope_id)?;
                let content = self.read_file_checked(&path_str, *span)?;
                Ok(Value::String(content))
            }
            Expr::ImportTable(args, span) => {
                let path_str = self.eval_string_to_string(&args.path, scope_id)?;
                let content = self.read_file_checked(&path_str, *span)?;
                let separator = if let Some(ref sep_lit) = args.separator {
                    let sep_str = self.eval_string_to_string(sep_lit, scope_id)?;
                    sep_str.chars().next().unwrap_or(',')
                } else {
                    ','
                };
                let headers = args.headers.unwrap_or(true);
                let columns: Option<Vec<String>> = args.columns.as_ref().map(|cols| {
                    cols.iter()
                        .filter_map(|s| self.eval_string_to_string(s, scope_id).ok())
                        .collect()
                });
                Ok(parse_table(
                    &content,
                    separator,
                    headers,
                    columns.as_deref(),
                ))
            }
            Expr::Paren(e, _) => self.eval_expr(e, scope_id),
        }
    }

    // ------------------------------------------------------------------
    // String evaluation
    // ------------------------------------------------------------------

    fn eval_string_lit(&mut self, s: &StringLit, scope_id: ScopeId) -> Result<Value, Diagnostic> {
        let mut result = String::new();
        for part in &s.parts {
            match part {
                StringPart::Literal(text) => result.push_str(text),
                StringPart::Interpolation(expr) => {
                    let val = self.eval_expr(expr, scope_id)?;
                    match val.to_interp_string() {
                        Ok(s) => result.push_str(&s),
                        Err(e) => return Err(Diagnostic::error(e, s.span).with_code("E050")),
                    }
                }
            }
        }
        Ok(Value::String(result))
    }

    fn eval_string_to_string(
        &mut self,
        s: &StringLit,
        scope_id: ScopeId,
    ) -> Result<String, Diagnostic> {
        match self.eval_string_lit(s, scope_id)? {
            Value::String(s) => Ok(s),
            _ => unreachable!(),
        }
    }

    // ------------------------------------------------------------------
    // File import helpers
    // ------------------------------------------------------------------

    fn read_file_checked(&self, path_str: &str, span: Span) -> Result<String, Diagnostic> {
        let fs = self
            .fs
            .as_ref()
            .ok_or_else(|| Diagnostic::error("import_raw not available in this context", span))?;
        let base = self.base_dir.as_ref().unwrap();

        // Resolve path relative to base_dir
        let resolved = base.join(path_str);

        // Normalize to handle .. etc
        let normalized = crate::imports::normalize_path(&resolved);

        // Jail check: normalized path must start with base_dir
        let canonical_base = crate::imports::normalize_path(base);
        if !normalized.starts_with(&canonical_base) {
            return Err(Diagnostic::error(
                format!("path '{}' escapes root directory", path_str),
                span,
            ));
        }

        fs.read_file(&normalized)
            .map_err(|e| Diagnostic::error(format!("cannot read file '{}': {}", path_str, e), span))
    }

    // ------------------------------------------------------------------
    // Identifier resolution
    // ------------------------------------------------------------------

    fn eval_ident(&mut self, ident: &Ident, scope_id: ScopeId) -> Result<Value, Diagnostic> {
        let resolved = self
            .scopes
            .resolve(scope_id, &ident.name)
            .map(|(_, entry)| (entry.value.clone(), entry.evaluated));
        match resolved {
            Some((Some(val), _)) => {
                self.scopes.record_read(scope_id, &ident.name);
                Ok(val)
            }
            Some((None, _)) => Err(Diagnostic::error(
                format!("variable '{}' has not been evaluated yet", ident.name),
                ident.span,
            )
            .with_code("E040")),
            None => Err(Diagnostic::error(
                format!("undefined reference '{}'", ident.name),
                ident.span,
            )
            .with_code("E040")),
        }
    }

    // ------------------------------------------------------------------
    // Binary operations
    // ------------------------------------------------------------------

    fn eval_binary(
        &mut self,
        lhs: &Expr,
        op: BinOp,
        rhs: &Expr,
        span: Span,
        scope_id: ScopeId,
    ) -> Result<Value, Diagnostic> {
        // Short-circuit for && and ||
        if op == BinOp::And {
            let l = self.eval_expr(lhs, scope_id)?;
            if l == Value::Bool(false) {
                return Ok(Value::Bool(false));
            }
            let r = self.eval_expr(rhs, scope_id)?;
            return match (&l, &r) {
                (Value::Bool(_), Value::Bool(b)) => Ok(Value::Bool(*b)),
                _ => Err(Diagnostic::error("&& requires bool operands", span).with_code("E050")),
            };
        }
        if op == BinOp::Or {
            let l = self.eval_expr(lhs, scope_id)?;
            if l == Value::Bool(true) {
                return Ok(Value::Bool(true));
            }
            let r = self.eval_expr(rhs, scope_id)?;
            return match (&l, &r) {
                (Value::Bool(_), Value::Bool(b)) => Ok(Value::Bool(*b)),
                _ => Err(Diagnostic::error("|| requires bool operands", span).with_code("E050")),
            };
        }

        let l = self.eval_expr(lhs, scope_id)?;
        let r = self.eval_expr(rhs, scope_id)?;

        match op {
            BinOp::Add => self.eval_add(&l, &r, span),
            BinOp::Sub => self.eval_arithmetic(&l, &r, span, |a, b| a - b, |a, b| a - b),
            BinOp::Mul => self.eval_arithmetic(&l, &r, span, |a, b| a * b, |a, b| a * b),
            BinOp::Div => self.eval_div(&l, &r, span),
            BinOp::Mod => self.eval_mod(&l, &r, span),
            BinOp::Eq => Ok(Value::Bool(l == r)),
            BinOp::Neq => Ok(Value::Bool(l != r)),
            BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte => {
                self.eval_comparison(&l, op, &r, span)
            }
            BinOp::Match => self.eval_regex_match(&l, &r, span),
            BinOp::And | BinOp::Or => unreachable!(),
        }
    }

    fn eval_add(&self, l: &Value, r: &Value, span: Span) -> Result<Value, Diagnostic> {
        match (l, r) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 + b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a + *b as f64)),
            (Value::String(a), Value::String(b)) => Ok(Value::String(format!("{}{}", a, b))),
            _ => Err(Diagnostic::error(
                format!("cannot add {} and {}", l.type_name(), r.type_name()),
                span,
            )
            .with_code("E050")),
        }
    }

    fn eval_arithmetic(
        &self,
        l: &Value,
        r: &Value,
        span: Span,
        int_op: impl Fn(i64, i64) -> i64,
        float_op: impl Fn(f64, f64) -> f64,
    ) -> Result<Value, Diagnostic> {
        match (l, r) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(int_op(*a, *b))),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(float_op(*a, *b))),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(float_op(*a as f64, *b))),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(float_op(*a, *b as f64))),
            _ => Err(Diagnostic::error(
                format!(
                    "arithmetic requires numeric operands, got {} and {}",
                    l.type_name(),
                    r.type_name()
                ),
                span,
            )
            .with_code("E050")),
        }
    }

    fn eval_div(&self, l: &Value, r: &Value, span: Span) -> Result<Value, Diagnostic> {
        match (l, r) {
            (_, Value::Int(0)) => {
                Err(Diagnostic::error("division by zero", span).with_code("E051"))
            }
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a / b)),
            (Value::Float(a), Value::Float(b)) => {
                if *b == 0.0 {
                    return Err(Diagnostic::error("division by zero", span).with_code("E051"));
                }
                Ok(Value::Float(a / b))
            }
            (Value::Int(a), Value::Float(b)) => {
                if *b == 0.0 {
                    return Err(Diagnostic::error("division by zero", span).with_code("E051"));
                }
                Ok(Value::Float(*a as f64 / b))
            }
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a / *b as f64)),
            _ => {
                Err(Diagnostic::error("division requires numeric operands", span).with_code("E050"))
            }
        }
    }

    fn eval_mod(&self, l: &Value, r: &Value, span: Span) -> Result<Value, Diagnostic> {
        match (l, r) {
            (Value::Int(a), Value::Int(b)) => {
                if *b == 0 {
                    return Err(Diagnostic::error("modulo by zero", span).with_code("E051"));
                }
                Ok(Value::Int(a % b))
            }
            _ => Err(Diagnostic::error("modulo requires int operands", span).with_code("E050")),
        }
    }

    fn eval_comparison(
        &self,
        l: &Value,
        op: BinOp,
        r: &Value,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let result = match (l, r) {
            (Value::Int(a), Value::Int(b)) => compare_ord(a, b, op),
            (Value::Float(a), Value::Float(b)) => compare_partial_ord(a, b, op),
            (Value::Int(a), Value::Float(b)) => compare_partial_ord(&(*a as f64), b, op),
            (Value::Float(a), Value::Int(b)) => compare_partial_ord(a, &(*b as f64), op),
            (Value::String(a), Value::String(b)) => compare_ord(a, b, op),
            _ => {
                return Err(Diagnostic::error(
                    format!("cannot compare {} and {}", l.type_name(), r.type_name()),
                    span,
                )
                .with_code("E050"))
            }
        };
        Ok(Value::Bool(result))
    }

    fn eval_regex_match(&self, l: &Value, r: &Value, span: Span) -> Result<Value, Diagnostic> {
        match (l, r) {
            (Value::String(s), Value::String(pattern)) => match regex::Regex::new(pattern) {
                Ok(re) => Ok(Value::Bool(re.is_match(s))),
                Err(e) => {
                    Err(Diagnostic::error(format!("invalid regex: {}", e), span).with_code("E050"))
                }
            },
            _ => Err(Diagnostic::error("=~ requires string operands", span).with_code("E050")),
        }
    }

    // ------------------------------------------------------------------
    // Unary operations
    // ------------------------------------------------------------------

    fn eval_unary(
        &mut self,
        op: UnaryOp,
        expr: &Expr,
        span: Span,
        scope_id: ScopeId,
    ) -> Result<Value, Diagnostic> {
        let val = self.eval_expr(expr, scope_id)?;
        match op {
            UnaryOp::Not => match val {
                Value::Bool(b) => Ok(Value::Bool(!b)),
                _ => Err(Diagnostic::error("! requires bool operand", span).with_code("E050")),
            },
            UnaryOp::Neg => {
                match val {
                    Value::Int(i) => Ok(Value::Int(-i)),
                    Value::Float(f) => Ok(Value::Float(-f)),
                    _ => Err(Diagnostic::error("unary - requires numeric operand", span)
                        .with_code("E050")),
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Member / index access
    // ------------------------------------------------------------------

    fn access_member(&self, val: &Value, field: &str, span: Span) -> Result<Value, Diagnostic> {
        match val {
            Value::Map(m) => m.get(field).cloned().ok_or_else(|| {
                Diagnostic::error(format!("key '{}' not found in map", field), span)
                    .with_code("E054")
            }),
            Value::BlockRef(br) => br.attributes.get(field).cloned().ok_or_else(|| {
                Diagnostic::error(format!("attribute '{}' not found in block", field), span)
                    .with_code("E054")
            }),
            _ => Err(Diagnostic::error(
                format!("cannot access member on {}", val.type_name()),
                span,
            )
            .with_code("E050")),
        }
    }

    fn access_index(&self, val: &Value, idx: &Value, span: Span) -> Result<Value, Diagnostic> {
        match (val, idx) {
            (Value::List(items), Value::Int(i)) => {
                let i = *i as usize;
                items.get(i).cloned().ok_or_else(|| {
                    Diagnostic::error(
                        format!("index {} out of bounds (length {})", i, items.len()),
                        span,
                    )
                    .with_code("E054")
                })
            }
            (Value::Map(m), Value::String(key)) => m.get(key).cloned().ok_or_else(|| {
                Diagnostic::error(format!("key '{}' not found in map", key), span).with_code("E054")
            }),
            _ => Err(Diagnostic::error(
                format!("cannot index {} with {}", val.type_name(), idx.type_name()),
                span,
            )
            .with_code("E050")),
        }
    }

    // ------------------------------------------------------------------
    // Function calls
    // ------------------------------------------------------------------

    fn eval_fn_call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        span: Span,
        scope_id: ScopeId,
    ) -> Result<Value, Diagnostic> {
        // Determine function
        match callee {
            Expr::Ident(ident) => {
                let name = &ident.name;

                // Check higher-order builtins first
                if matches!(
                    name.as_str(),
                    "map" | "filter" | "every" | "some" | "reduce" | "count"
                ) {
                    return self.eval_higher_order(name, args, span, scope_id);
                }

                // Introspection functions (special-cased, need evaluator state)
                if name == "is_imported" {
                    let eval_args = self.eval_call_args(args, scope_id)?;
                    if eval_args.len() != 1 {
                        return Err(Diagnostic::error(
                            "is_imported() expects exactly 1 argument",
                            span,
                        )
                        .with_code("E052"));
                    }
                    let path_str = match &eval_args[0] {
                        Value::String(s) => s.clone(),
                        _ => {
                            return Err(Diagnostic::error(
                                "is_imported() argument must be a string",
                                span,
                            )
                            .with_code("E052"))
                        }
                    };
                    // Resolve relative to base_dir
                    let result = if let Some(base) = &self.base_dir {
                        let resolved = crate::imports::normalize_path(&base.join(&path_str));
                        self.imported_paths.contains(&resolved)
                    } else {
                        false
                    };
                    return Ok(Value::Bool(result));
                }

                if name == "has_schema" {
                    let eval_args = self.eval_call_args(args, scope_id)?;
                    if eval_args.len() != 1 {
                        return Err(Diagnostic::error(
                            "has_schema() expects exactly 1 argument",
                            span,
                        )
                        .with_code("E052"));
                    }
                    let schema_name = match &eval_args[0] {
                        Value::String(s) => s.clone(),
                        _ => {
                            return Err(Diagnostic::error(
                                "has_schema() argument must be a string",
                                span,
                            )
                            .with_code("E052"))
                        }
                    };
                    return Ok(Value::Bool(self.schema_names.contains(&schema_name)));
                }

                // Evaluate arguments eagerly for normal builtins and user fns
                let eval_args = self.eval_call_args(args, scope_id)?;

                // Check builtin functions
                if let Some(builtin) = self.builtins.get(name.as_str()).cloned() {
                    return builtin(&eval_args).map_err(|e| {
                        Diagnostic::error(format!("in {}(): {}", name, e), span).with_code("E052")
                    });
                }

                // Check user-defined functions in scope
                let maybe_func = self.scopes.resolve(scope_id, name).and_then(|(_, entry)| {
                    if let Some(Value::Function(func)) = &entry.value {
                        Some(func.clone())
                    } else {
                        None
                    }
                });
                if let Some(func) = maybe_func {
                    self.scopes.record_read(scope_id, name);
                    return self.call_user_fn(&func, &eval_args, span);
                }

                // Check if function is declared (from library import) but not registered
                if self.declared_functions.contains(name.as_str()) {
                    return Err(
                        Diagnostic::error(
                            format!("function '{}' is declared in library but not registered by host application", name),
                            span,
                        )
                        .with_code("E053"),
                    );
                }

                Err(
                    Diagnostic::error(format!("unknown function '{}'", name), span)
                        .with_code("E052"),
                )
            }
            _ => {
                let callee_val = self.eval_expr(callee, scope_id)?;
                let eval_args = self.eval_call_args(args, scope_id)?;
                match callee_val {
                    Value::Function(func) => self.call_user_fn(&func, &eval_args, span),
                    _ => Err(Diagnostic::error("not a callable value", span).with_code("E050")),
                }
            }
        }
    }

    fn eval_call_args(
        &mut self,
        args: &[CallArg],
        scope_id: ScopeId,
    ) -> Result<Vec<Value>, Diagnostic> {
        let mut eval_args = Vec::with_capacity(args.len());
        for arg in args {
            match arg {
                CallArg::Positional(e) | CallArg::Named(_, e) => {
                    eval_args.push(self.eval_expr(e, scope_id)?);
                }
            }
        }
        Ok(eval_args)
    }

    fn call_user_fn(
        &mut self,
        func: &FunctionValue,
        args: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        if args.len() != func.params.len() {
            return Err(Diagnostic::error(
                format!(
                    "expected {} arguments, got {}",
                    func.params.len(),
                    args.len()
                ),
                span,
            )
            .with_code("E052"));
        }

        let parent_scope = func.closure_scope.unwrap_or(ScopeId(0));
        let call_scope = self
            .scopes
            .create_scope(ScopeKind::Lambda, Some(parent_scope));

        for (param, arg) in func.params.iter().zip(args.iter()) {
            self.scopes.add_entry(
                call_scope,
                ScopeEntry {
                    name: param.clone(),
                    kind: ScopeEntryKind::LetBinding,
                    value: Some(arg.clone()),
                    span,
                    dependencies: Default::default(),
                    evaluated: true,
                    read_count: 0,
                },
            );
        }

        match &func.body {
            FunctionBody::UserDefined(expr) => self.eval_expr(expr, call_scope),
            FunctionBody::BlockExpr(lets, final_expr) => {
                for (name, expr) in lets {
                    let val = self.eval_expr(expr, call_scope)?;
                    self.scopes.add_entry(
                        call_scope,
                        ScopeEntry {
                            name: name.clone(),
                            kind: ScopeEntryKind::LetBinding,
                            value: Some(val),
                            span,
                            dependencies: Default::default(),
                            evaluated: true,
                            read_count: 0,
                        },
                    );
                }
                self.eval_expr(final_expr, call_scope)
            }
            FunctionBody::Builtin(name) => {
                // Builtins are handled in eval_fn_call, not here
                if let Some(builtin) = self.builtins.get(name.as_str()) {
                    builtin(args).map_err(|e| {
                        Diagnostic::error(format!("in {}(): {}", name, e), span).with_code("E052")
                    })
                } else {
                    Err(Diagnostic::error(
                        format!("unknown builtin '{}'", name),
                        span,
                    ))
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Higher-order function evaluation
    // ------------------------------------------------------------------

    fn eval_higher_order(
        &mut self,
        name: &str,
        args: &[CallArg],
        span: Span,
        scope_id: ScopeId,
    ) -> Result<Value, Diagnostic> {
        match name {
            "map" => {
                self.expect_ho_args(2, args.len(), "map", span)?;
                let list = self.eval_call_arg(&args[0], scope_id)?;
                let func = self.eval_call_arg_as_fn(&args[1], scope_id, "map", span)?;
                let items = self.expect_list(list, "map", span)?;
                let mut results = Vec::with_capacity(items.len());
                for item in &items {
                    results.push(self.call_user_fn(&func, std::slice::from_ref(item), span)?);
                }
                Ok(Value::List(results))
            }
            "filter" => {
                self.expect_ho_args(2, args.len(), "filter", span)?;
                let list = self.eval_call_arg(&args[0], scope_id)?;
                let func = self.eval_call_arg_as_fn(&args[1], scope_id, "filter", span)?;
                let items = self.expect_list(list, "filter", span)?;
                let mut results = Vec::new();
                for item in &items {
                    let keep = self.call_user_fn(&func, std::slice::from_ref(item), span)?;
                    if keep == Value::Bool(true) {
                        results.push(item.clone());
                    }
                }
                Ok(Value::List(results))
            }
            "every" => {
                self.expect_ho_args(2, args.len(), "every", span)?;
                let list = self.eval_call_arg(&args[0], scope_id)?;
                let func = self.eval_call_arg_as_fn(&args[1], scope_id, "every", span)?;
                let items = self.expect_list(list, "every", span)?;
                for item in &items {
                    let result = self.call_user_fn(&func, std::slice::from_ref(item), span)?;
                    if result != Value::Bool(true) {
                        return Ok(Value::Bool(false));
                    }
                }
                Ok(Value::Bool(true))
            }
            "some" => {
                self.expect_ho_args(2, args.len(), "some", span)?;
                let list = self.eval_call_arg(&args[0], scope_id)?;
                let func = self.eval_call_arg_as_fn(&args[1], scope_id, "some", span)?;
                let items = self.expect_list(list, "some", span)?;
                for item in &items {
                    let result = self.call_user_fn(&func, std::slice::from_ref(item), span)?;
                    if result == Value::Bool(true) {
                        return Ok(Value::Bool(true));
                    }
                }
                Ok(Value::Bool(false))
            }
            "reduce" => {
                self.expect_ho_args(3, args.len(), "reduce", span)?;
                let list = self.eval_call_arg(&args[0], scope_id)?;
                let init = self.eval_call_arg(&args[1], scope_id)?;
                let func = self.eval_call_arg_as_fn(&args[2], scope_id, "reduce", span)?;
                let items = self.expect_list(list, "reduce", span)?;
                let mut acc = init;
                for item in &items {
                    acc = self.call_user_fn(&func, &[acc, item.clone()], span)?;
                }
                Ok(acc)
            }
            "count" => {
                self.expect_ho_args(2, args.len(), "count", span)?;
                let list = self.eval_call_arg(&args[0], scope_id)?;
                let func = self.eval_call_arg_as_fn(&args[1], scope_id, "count", span)?;
                let items = self.expect_list(list, "count", span)?;
                let mut n = 0i64;
                for item in &items {
                    let result = self.call_user_fn(&func, std::slice::from_ref(item), span)?;
                    if result == Value::Bool(true) {
                        n += 1;
                    }
                }
                Ok(Value::Int(n))
            }
            _ => unreachable!(),
        }
    }

    fn expect_ho_args(
        &self,
        expected: usize,
        got: usize,
        name: &str,
        span: Span,
    ) -> Result<(), Diagnostic> {
        if got != expected {
            Err(Diagnostic::error(
                format!("{}() takes {} arguments, got {}", name, expected, got),
                span,
            ))
        } else {
            Ok(())
        }
    }

    fn eval_call_arg(&mut self, arg: &CallArg, scope_id: ScopeId) -> Result<Value, Diagnostic> {
        match arg {
            CallArg::Positional(e) | CallArg::Named(_, e) => self.eval_expr(e, scope_id),
        }
    }

    fn eval_call_arg_as_fn(
        &mut self,
        arg: &CallArg,
        scope_id: ScopeId,
        fn_name: &str,
        span: Span,
    ) -> Result<FunctionValue, Diagnostic> {
        let val = self.eval_call_arg(arg, scope_id)?;
        match val {
            Value::Function(f) => Ok(f),
            _ => Err(Diagnostic::error(
                format!(
                    "{}() callback argument must be a function, got {}",
                    fn_name,
                    val.type_name()
                ),
                span,
            )),
        }
    }

    fn expect_list(&self, val: Value, fn_name: &str, span: Span) -> Result<Vec<Value>, Diagnostic> {
        match val {
            Value::List(l) => Ok(l),
            _ => Err(Diagnostic::error(
                format!(
                    "{}() first argument must be a list, got {}",
                    fn_name,
                    val.type_name()
                ),
                span,
            )),
        }
    }

    // ------------------------------------------------------------------
    // Block evaluation — build Value::BlockRef from AST Block nodes
    // ------------------------------------------------------------------

    /// Build a `BlockRef` for a block AST node.  Evaluates the block's child
    /// scope (attributes, let-bindings, nested blocks) and collects the results
    /// into a `BlockRef` value.
    fn build_block_ref(&mut self, block: &Block, parent_scope: ScopeId) -> BlockRef {
        let name = block
            .inline_id
            .as_ref()
            .map(|id| match id {
                InlineId::Literal(lit) => lit.value.clone(),
                InlineId::Interpolated(_) => "?interpolated?".to_string(),
            })
            .unwrap_or_else(|| format!("__block_{}", block.kind.name));

        let child_scope = self.block_scope_map.get(&(parent_scope, name)).copied();

        if let Some(child_scope) = child_scope {
            // Evaluate entries inside the child scope (topo-sorted)
            self.evaluate_block_scope(&block.body, child_scope);
        }

        // Collect attributes from the child scope
        let mut attributes = IndexMap::new();
        let mut children = Vec::new();

        if let Some(child_scope) = child_scope {
            let scope = self.scopes.get(child_scope);
            let entry_names: Vec<(String, ScopeEntryKind)> = scope
                .entries
                .values()
                .map(|e| (e.name.clone(), e.kind))
                .collect();

            for (ename, ekind) in &entry_names {
                match ekind {
                    ScopeEntryKind::Attribute
                    | ScopeEntryKind::ExportLet
                    | ScopeEntryKind::TableEntry => {
                        if let Some((_, entry)) = self.scopes.resolve(child_scope, ename) {
                            if let Some(ref val) = entry.value {
                                attributes.insert(ename.clone(), val.clone());
                            }
                        }
                    }
                    ScopeEntryKind::BlockChild => {
                        if let Some((_, entry)) = self.scopes.resolve(child_scope, ename) {
                            if let Some(Value::BlockRef(br)) = &entry.value {
                                children.push(br.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Inject text content as "content" attribute
        if let Some(ref tc) = block.text_content {
            if let Ok(val) = self.eval_string_lit(tc, child_scope.unwrap_or(parent_scope)) {
                attributes.insert("content".to_string(), val);
            }
        }

        // Build decorator values
        let decorators = block
            .decorators
            .iter()
            .map(|d| {
                let mut args = IndexMap::new();
                for (i, arg) in d.args.iter().enumerate() {
                    match arg {
                        DecoratorArg::Positional(expr) => {
                            if let Ok(val) =
                                self.eval_expr(expr, child_scope.unwrap_or(parent_scope))
                            {
                                args.insert(format!("_{}", i), val);
                            }
                        }
                        DecoratorArg::Named(name, expr) => {
                            if let Ok(val) =
                                self.eval_expr(expr, child_scope.unwrap_or(parent_scope))
                            {
                                args.insert(name.name.clone(), val);
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

        let evaluated_args: Vec<Value> = block
            .inline_args
            .iter()
            .filter_map(|e| self.eval_expr(e, child_scope.unwrap_or(parent_scope)).ok())
            .collect();

        let inline_id = block.inline_id.as_ref().map(|id| match id {
            InlineId::Literal(lit) => lit.value.clone(),
            InlineId::Interpolated(_) => "?interpolated?".to_string(),
        });

        if !evaluated_args.is_empty() {
            attributes.insert("_args".to_string(), Value::List(evaluated_args));
        }

        BlockRef {
            kind: block.kind.name.clone(),
            id: inline_id,
            attributes,
            children,
            decorators,
            span: block.span,
        }
    }

    /// Evaluate all entries in a block scope (attributes, let-bindings, child blocks).
    /// Uses topo sort within the scope, then evaluates each entry by walking the
    /// block body items.
    /// Evaluate an inline table into a `Value::List(Vec<Value::Map>)`.
    ///
    /// If the table has an `import_expr`, evaluate that expression directly.
    /// Otherwise, build rows from the column declarations and row cells.
    fn eval_inline_table(&mut self, table: &Table, scope_id: ScopeId) -> Result<Value, Diagnostic> {
        // If the table uses import_table(...), evaluate the import expression
        if let Some(ref expr) = table.import_expr {
            return self.eval_expr(expr, scope_id);
        }

        // Build from inline columns + rows
        let col_names: Vec<String> = table.columns.iter().map(|c| c.name.name.clone()).collect();

        let mut rows = Vec::with_capacity(table.rows.len());
        for row in &table.rows {
            let mut map = IndexMap::new();
            for (i, col_name) in col_names.iter().enumerate() {
                if i < row.cells.len() {
                    let val = self.eval_expr(&row.cells[i], scope_id)?;
                    map.insert(col_name.clone(), val);
                } else {
                    map.insert(col_name.clone(), Value::Null);
                }
            }
            rows.push(Value::Map(map));
        }

        Ok(Value::List(rows))
    }

    fn evaluate_block_scope(&mut self, body: &[BodyItem], scope_id: ScopeId) {
        match self.scopes.topo_sort(scope_id) {
            Ok(order) => {
                for name in &order {
                    self.evaluate_body_entry(body, scope_id, name);
                }
            }
            Err(cycle) => {
                self.diagnostics.error_with_code(
                    format!("cyclic dependency in block: {}", cycle.join(" -> ")),
                    Span::dummy(),
                    "E041",
                );
            }
        }
    }

    /// Evaluate a single named entry from a block body (analogous to
    /// `evaluate_doc_entry` but for `BodyItem` slices).
    fn evaluate_body_entry(&mut self, body: &[BodyItem], scope_id: ScopeId, name: &str) {
        for item in body {
            match item {
                BodyItem::Attribute(attr) if attr.name.name == name => {
                    let val = self.eval_expr(&attr.value, scope_id);
                    match val {
                        Ok(v) => {
                            if let Some((_, entry)) = self.scopes.resolve_mut(scope_id, name) {
                                entry.value = Some(v);
                                entry.evaluated = true;
                            }
                        }
                        Err(diag) => self.diagnostics.add(diag),
                    }
                    return;
                }
                BodyItem::LetBinding(lb) if lb.name.name == name => {
                    let val = self.eval_expr(&lb.value, scope_id);
                    match val {
                        Ok(v) => {
                            if let Some((_, entry)) = self.scopes.resolve_mut(scope_id, name) {
                                entry.value = Some(v);
                                entry.evaluated = true;
                            }
                        }
                        Err(diag) => self.diagnostics.add(diag),
                    }
                    return;
                }
                BodyItem::Block(block) => {
                    let block_name = block
                        .inline_id
                        .as_ref()
                        .map(|id| match id {
                            InlineId::Literal(lit) => lit.value.clone(),
                            InlineId::Interpolated(_) => "?interpolated?".to_string(),
                        })
                        .unwrap_or_else(|| format!("__block_{}", block.kind.name));
                    if block_name == name {
                        let block_ref = self.build_block_ref(block, scope_id);
                        if let Some((_, entry)) = self.scopes.resolve_mut(scope_id, name) {
                            entry.value = Some(Value::BlockRef(block_ref));
                            entry.evaluated = true;
                        }
                        return;
                    }
                }
                BodyItem::Table(table) => {
                    let table_name = table
                        .inline_id
                        .as_ref()
                        .map(|id| match id {
                            InlineId::Literal(lit) => lit.value.clone(),
                            InlineId::Interpolated(_) => "?interpolated?".to_string(),
                        })
                        .unwrap_or_else(|| "__table".to_string());
                    if table_name == name {
                        match self.eval_inline_table(table, scope_id) {
                            Ok(v) => {
                                if let Some((_, entry)) = self.scopes.resolve_mut(scope_id, name) {
                                    entry.value = Some(v);
                                    entry.evaluated = true;
                                }
                            }
                            Err(diag) => self.diagnostics.add(diag),
                        }
                        return;
                    }
                }
                _ => {}
            }
        }
    }

    // ------------------------------------------------------------------
    // Query evaluation
    // ------------------------------------------------------------------

    pub(crate) fn eval_query(
        &mut self,
        pipeline: &QueryPipeline,
        span: Span,
        scope_id: ScopeId,
    ) -> Result<Value, Diagnostic> {
        let blocks = self.collect_blocks(scope_id);
        let engine = super::query::QueryEngine::new();
        engine
            .execute(pipeline, &blocks, self, scope_id)
            .map_err(|e| Diagnostic::error(e, span).with_code("E050"))
    }

    fn collect_blocks(&self, scope_id: ScopeId) -> Vec<BlockRef> {
        let scope = self.scopes.get(scope_id);
        let mut blocks = Vec::new();
        for entry in scope.entries.values() {
            match &entry.value {
                Some(Value::BlockRef(br)) => {
                    blocks.push(br.clone());
                }
                Some(Value::List(rows)) if entry.kind == ScopeEntryKind::TableEntry => {
                    // Convert table entries into pseudo-BlockRefs so queries work
                    let children: Vec<BlockRef> = rows
                        .iter()
                        .filter_map(|row| {
                            if let Value::Map(m) = row {
                                Some(BlockRef {
                                    kind: "__row".to_string(),
                                    id: None,
                                    attributes: m.clone(),
                                    children: Vec::new(),
                                    decorators: Vec::new(),
                                    span: Span::dummy(),
                                })
                            } else {
                                None
                            }
                        })
                        .collect();
                    blocks.push(BlockRef {
                        kind: "table".to_string(),
                        id: Some(entry.name.clone()),
                        attributes: indexmap::IndexMap::new(),
                        children,
                        decorators: Vec::new(),
                        span: entry.span,
                    });
                }
                _ => {}
            }
        }
        // Walk to parent scope to collect blocks there too
        if let Some(parent) = scope.parent {
            blocks.extend(self.collect_blocks(parent));
        }
        blocks
    }

    // ------------------------------------------------------------------
    // Output collection
    // ------------------------------------------------------------------

    fn collect_output(&self, scope_id: ScopeId) -> IndexMap<String, Value> {
        let scope = self.scopes.get(scope_id);
        let mut result = IndexMap::new();
        for (name, entry) in &scope.entries {
            if entry.kind == ScopeEntryKind::Attribute
                || entry.kind == ScopeEntryKind::BlockChild
                || entry.kind == ScopeEntryKind::ExportLet
                || entry.kind == ScopeEntryKind::TableEntry
            {
                if let Some(ref val) = entry.value {
                    result.insert(name.clone(), val.clone());
                }
            }
        }
        result
    }

    // ------------------------------------------------------------------
    // Accessors
    // ------------------------------------------------------------------

    pub fn into_diagnostics(self) -> DiagnosticBag {
        self.diagnostics
    }

    pub fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    /// Scan all scopes for `LetBinding` entries with zero reads and emit W002.
    fn check_unused_variables(&mut self) {
        let unused: Vec<(String, Span)> = self
            .scopes
            .all_entries()
            .filter(|(_, entry)| entry.kind == ScopeEntryKind::LetBinding && entry.read_count == 0)
            .map(|(_, entry)| (entry.name.clone(), entry.span))
            .collect();

        for (name, span) in unused {
            // Skip names starting with `_` (conventional unused marker)
            if name.starts_with('_') {
                continue;
            }
            self.diagnostics
                .warning_with_code(format!("unused variable '{}'", name), span, "W002");
        }
    }

    /// Provide read access to the scope arena.
    pub fn scopes(&self) -> &ScopeArena {
        &self.scopes
    }

    /// Provide mutable access to the scope arena (used by the facade crate and query engine).
    pub fn scopes_mut(&mut self) -> &mut ScopeArena {
        &mut self.scopes
    }

    /// Consume the evaluator, returning the scope arena and diagnostics separately.
    /// Used by the LSP to retain scope information for hover/go-to-definition.
    pub fn into_parts(self) -> (ScopeArena, DiagnosticBag) {
        (self.scopes, self.diagnostics)
    }
}

impl Default for Evaluator {
    fn default() -> Self {
        Self::new()
    }
}

// =====================================================================
// Free-standing helpers
// =====================================================================

/// Check whether a list of decorators contains `@allow(arg_name)`.
fn has_allow_decorator(decorators: &[Decorator], arg_name: &str) -> bool {
    decorators.iter().any(|d| {
        d.name.name == "allow"
            && d.args.iter().any(|a| match a {
                DecoratorArg::Positional(Expr::Ident(id)) => id.name == arg_name,
                DecoratorArg::Positional(Expr::IdentifierLit(id)) => id.value == arg_name,
                _ => false,
            })
    })
}

fn compare_ord<T: Ord>(a: &T, b: &T, op: BinOp) -> bool {
    match op {
        BinOp::Lt => a < b,
        BinOp::Gt => a > b,
        BinOp::Lte => a <= b,
        BinOp::Gte => a >= b,
        _ => unreachable!(),
    }
}

fn compare_partial_ord<T: PartialOrd>(a: &T, b: &T, op: BinOp) -> bool {
    match op {
        BinOp::Lt => a < b,
        BinOp::Gt => a > b,
        BinOp::Lte => a <= b,
        BinOp::Gte => a >= b,
        _ => unreachable!(),
    }
}

/// Parse CSV/TSV content into a `Value::List(Vec<Value::Map>)`.
///
/// This is a pure function extracted from `Evaluator` so it can be reused
/// during early import-table resolution (Phase 3a) without a full evaluator.
pub fn parse_table(
    content: &str,
    separator: char,
    has_headers: bool,
    explicit_columns: Option<&[String]>,
) -> Value {
    let mut lines = content.lines().peekable();

    // Determine column names
    let col_names: Vec<String> = if let Some(cols) = explicit_columns {
        // Explicit columns provided — skip header line if headers=true
        if has_headers {
            lines.next();
        }
        cols.to_vec()
    } else if has_headers {
        // First line is headers
        match lines.next() {
            Some(line) => line
                .split(separator)
                .map(|s| s.trim().to_string())
                .collect(),
            None => return Value::List(vec![]),
        }
    } else {
        // No headers, no explicit columns — use numeric indices.
        // Peek at first data line to determine column count.
        match lines.peek() {
            Some(line) => (0..line.split(separator).count())
                .map(|i| i.to_string())
                .collect(),
            None => return Value::List(vec![]),
        }
    };

    let rows: Vec<Value> = lines
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let fields: Vec<&str> = line.split(separator).collect();
            let mut map = IndexMap::new();
            for (i, header) in col_names.iter().enumerate() {
                let val = fields.get(i).map(|f| f.trim()).unwrap_or("");
                map.insert(header.clone(), Value::String(val.to_string()));
            }
            Value::Map(map)
        })
        .collect();

    Value::List(rows)
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use wcl_core::span::{FileId, Span};

    fn ds() -> Span {
        Span::new(FileId(0), 0, 0)
    }

    fn mk_ident(name: &str) -> Ident {
        Ident {
            name: name.to_string(),
            span: ds(),
        }
    }

    // ── Integer arithmetic ───────────────────────────────────────────

    #[test]
    fn eval_int_add() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::BinaryOp(
            Box::new(Expr::IntLit(3, ds())),
            BinOp::Add,
            Box::new(Expr::IntLit(4, ds())),
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Int(7));
    }

    #[test]
    fn eval_int_sub() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::BinaryOp(
            Box::new(Expr::IntLit(10, ds())),
            BinOp::Sub,
            Box::new(Expr::IntLit(3, ds())),
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Int(7));
    }

    #[test]
    fn eval_int_mul() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::BinaryOp(
            Box::new(Expr::IntLit(6, ds())),
            BinOp::Mul,
            Box::new(Expr::IntLit(7, ds())),
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Int(42));
    }

    #[test]
    fn eval_int_div() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::BinaryOp(
            Box::new(Expr::IntLit(10, ds())),
            BinOp::Div,
            Box::new(Expr::IntLit(3, ds())),
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Int(3));
    }

    #[test]
    fn eval_int_mod() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::BinaryOp(
            Box::new(Expr::IntLit(10, ds())),
            BinOp::Mod,
            Box::new(Expr::IntLit(3, ds())),
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Int(1));
    }

    #[test]
    fn eval_div_by_zero() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::BinaryOp(
            Box::new(Expr::IntLit(10, ds())),
            BinOp::Div,
            Box::new(Expr::IntLit(0, ds())),
            ds(),
        );
        let err = ev.eval_expr(&expr, scope).unwrap_err();
        assert!(err.message.contains("division by zero"));
    }

    #[test]
    fn eval_unary_neg() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::UnaryOp(UnaryOp::Neg, Box::new(Expr::IntLit(5, ds())), ds());
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Int(-5));
    }

    // ── Float arithmetic ─────────────────────────────────────────────

    #[test]
    fn eval_float_add() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::BinaryOp(
            Box::new(Expr::FloatLit(1.5, ds())),
            BinOp::Add,
            Box::new(Expr::FloatLit(2.5, ds())),
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Float(4.0));
    }

    #[test]
    fn eval_int_float_mixed() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::BinaryOp(
            Box::new(Expr::IntLit(2, ds())),
            BinOp::Add,
            Box::new(Expr::FloatLit(1.5, ds())),
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Float(3.5));
    }

    // ── String interpolation ─────────────────────────────────────────

    #[test]
    fn eval_string_literal() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::StringLit(StringLit {
            parts: vec![StringPart::Literal("hello".to_string())],
            span: ds(),
        });
        assert_eq!(
            ev.eval_expr(&expr, scope).unwrap(),
            Value::String("hello".to_string())
        );
    }

    #[test]
    fn eval_string_interpolation() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        // Set up a variable in scope
        ev.scopes.add_entry(
            scope,
            ScopeEntry {
                name: "name".to_string(),
                kind: ScopeEntryKind::LetBinding,
                value: Some(Value::String("world".to_string())),
                span: ds(),
                dependencies: Default::default(),
                evaluated: true,
                read_count: 0,
            },
        );

        let expr = Expr::StringLit(StringLit {
            parts: vec![
                StringPart::Literal("hello ".to_string()),
                StringPart::Interpolation(Box::new(Expr::Ident(mk_ident("name")))),
                StringPart::Literal("!".to_string()),
            ],
            span: ds(),
        });
        assert_eq!(
            ev.eval_expr(&expr, scope).unwrap(),
            Value::String("hello world!".to_string())
        );
    }

    #[test]
    fn eval_string_concat() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::BinaryOp(
            Box::new(Expr::StringLit(StringLit {
                parts: vec![StringPart::Literal("foo".to_string())],
                span: ds(),
            })),
            BinOp::Add,
            Box::new(Expr::StringLit(StringLit {
                parts: vec![StringPart::Literal("bar".to_string())],
                span: ds(),
            })),
            ds(),
        );
        assert_eq!(
            ev.eval_expr(&expr, scope).unwrap(),
            Value::String("foobar".to_string())
        );
    }

    // ── Boolean logic ────────────────────────────────────────────────

    #[test]
    fn eval_bool_and() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::BinaryOp(
            Box::new(Expr::BoolLit(true, ds())),
            BinOp::And,
            Box::new(Expr::BoolLit(false, ds())),
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Bool(false));
    }

    #[test]
    fn eval_bool_or() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::BinaryOp(
            Box::new(Expr::BoolLit(false, ds())),
            BinOp::Or,
            Box::new(Expr::BoolLit(true, ds())),
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Bool(true));
    }

    #[test]
    fn eval_bool_not() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::UnaryOp(UnaryOp::Not, Box::new(Expr::BoolLit(true, ds())), ds());
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Bool(false));
    }

    #[test]
    fn eval_short_circuit_and() {
        // false && <anything> should short-circuit to false without evaluating rhs
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        // rhs references undefined variable — should never be evaluated
        let expr = Expr::BinaryOp(
            Box::new(Expr::BoolLit(false, ds())),
            BinOp::And,
            Box::new(Expr::Ident(mk_ident("undefined_var"))),
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Bool(false));
    }

    #[test]
    fn eval_short_circuit_or() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::BinaryOp(
            Box::new(Expr::BoolLit(true, ds())),
            BinOp::Or,
            Box::new(Expr::Ident(mk_ident("undefined_var"))),
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Bool(true));
    }

    // ── Comparisons ──────────────────────────────────────────────────

    #[test]
    fn eval_eq() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::BinaryOp(
            Box::new(Expr::IntLit(42, ds())),
            BinOp::Eq,
            Box::new(Expr::IntLit(42, ds())),
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Bool(true));
    }

    #[test]
    fn eval_neq() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::BinaryOp(
            Box::new(Expr::IntLit(1, ds())),
            BinOp::Neq,
            Box::new(Expr::IntLit(2, ds())),
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Bool(true));
    }

    #[test]
    fn eval_lt() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::BinaryOp(
            Box::new(Expr::IntLit(1, ds())),
            BinOp::Lt,
            Box::new(Expr::IntLit(2, ds())),
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Bool(true));
    }

    #[test]
    fn eval_gte() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::BinaryOp(
            Box::new(Expr::IntLit(3, ds())),
            BinOp::Gte,
            Box::new(Expr::IntLit(3, ds())),
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Bool(true));
    }

    // ── Ternary ──────────────────────────────────────────────────────

    #[test]
    fn eval_ternary_true() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::Ternary(
            Box::new(Expr::BoolLit(true, ds())),
            Box::new(Expr::IntLit(1, ds())),
            Box::new(Expr::IntLit(2, ds())),
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Int(1));
    }

    #[test]
    fn eval_ternary_false() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::Ternary(
            Box::new(Expr::BoolLit(false, ds())),
            Box::new(Expr::IntLit(1, ds())),
            Box::new(Expr::IntLit(2, ds())),
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Int(2));
    }

    // ── Lists and Maps ───────────────────────────────────────────────

    #[test]
    fn eval_list() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::List(
            vec![
                Expr::IntLit(1, ds()),
                Expr::IntLit(2, ds()),
                Expr::IntLit(3, ds()),
            ],
            ds(),
        );
        assert_eq!(
            ev.eval_expr(&expr, scope).unwrap(),
            Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
        );
    }

    #[test]
    fn eval_map() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::Map(
            vec![(MapKey::Ident(mk_ident("x")), Expr::IntLit(1, ds()))],
            ds(),
        );
        let mut expected = IndexMap::new();
        expected.insert("x".to_string(), Value::Int(1));
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Map(expected));
    }

    // ── Member / index access ────────────────────────────────────────

    #[test]
    fn eval_member_access() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let mut m = IndexMap::new();
        m.insert("key".to_string(), Value::Int(42));
        ev.scopes.add_entry(
            scope,
            ScopeEntry {
                name: "obj".to_string(),
                kind: ScopeEntryKind::LetBinding,
                value: Some(Value::Map(m)),
                span: ds(),
                dependencies: Default::default(),
                evaluated: true,
                read_count: 0,
            },
        );

        let expr = Expr::MemberAccess(
            Box::new(Expr::Ident(mk_ident("obj"))),
            mk_ident("key"),
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Int(42));
    }

    #[test]
    fn eval_index_access_list() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        ev.scopes.add_entry(
            scope,
            ScopeEntry {
                name: "arr".to_string(),
                kind: ScopeEntryKind::LetBinding,
                value: Some(Value::List(vec![
                    Value::Int(10),
                    Value::Int(20),
                    Value::Int(30),
                ])),
                span: ds(),
                dependencies: Default::default(),
                evaluated: true,
                read_count: 0,
            },
        );

        let expr = Expr::IndexAccess(
            Box::new(Expr::Ident(mk_ident("arr"))),
            Box::new(Expr::IntLit(1, ds())),
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Int(20));
    }

    // ── Built-in function calls ──────────────────────────────────────

    #[test]
    fn eval_builtin_upper() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::FnCall(
            Box::new(Expr::Ident(mk_ident("upper"))),
            vec![CallArg::Positional(Expr::StringLit(StringLit {
                parts: vec![StringPart::Literal("hello".to_string())],
                span: ds(),
            }))],
            ds(),
        );
        assert_eq!(
            ev.eval_expr(&expr, scope).unwrap(),
            Value::String("HELLO".to_string())
        );
    }

    #[test]
    fn eval_builtin_len() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::FnCall(
            Box::new(Expr::Ident(mk_ident("len"))),
            vec![CallArg::Positional(Expr::List(
                vec![
                    Expr::IntLit(1, ds()),
                    Expr::IntLit(2, ds()),
                    Expr::IntLit(3, ds()),
                ],
                ds(),
            ))],
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Int(3));
    }

    #[test]
    fn eval_builtin_abs() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::FnCall(
            Box::new(Expr::Ident(mk_ident("abs"))),
            vec![CallArg::Positional(Expr::IntLit(-42, ds()))],
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Int(42));
    }

    #[test]
    fn eval_unknown_function() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::FnCall(Box::new(Expr::Ident(mk_ident("nonexistent"))), vec![], ds());
        let err = ev.eval_expr(&expr, scope).unwrap_err();
        assert!(err.message.contains("unknown function"));
    }

    // ── Higher-order functions ───────────────────────────────────────

    fn mk_lambda_add1() -> Expr {
        Expr::Lambda(
            vec![mk_ident("x")],
            Box::new(Expr::BinaryOp(
                Box::new(Expr::Ident(mk_ident("x"))),
                BinOp::Add,
                Box::new(Expr::IntLit(1, ds())),
                ds(),
            )),
            ds(),
        )
    }

    fn mk_lambda_is_positive() -> Expr {
        Expr::Lambda(
            vec![mk_ident("x")],
            Box::new(Expr::BinaryOp(
                Box::new(Expr::Ident(mk_ident("x"))),
                BinOp::Gt,
                Box::new(Expr::IntLit(0, ds())),
                ds(),
            )),
            ds(),
        )
    }

    #[test]
    fn eval_map_ho() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::FnCall(
            Box::new(Expr::Ident(mk_ident("map"))),
            vec![
                CallArg::Positional(Expr::List(
                    vec![
                        Expr::IntLit(1, ds()),
                        Expr::IntLit(2, ds()),
                        Expr::IntLit(3, ds()),
                    ],
                    ds(),
                )),
                CallArg::Positional(mk_lambda_add1()),
            ],
            ds(),
        );
        assert_eq!(
            ev.eval_expr(&expr, scope).unwrap(),
            Value::List(vec![Value::Int(2), Value::Int(3), Value::Int(4)])
        );
    }

    #[test]
    fn eval_filter_ho() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::FnCall(
            Box::new(Expr::Ident(mk_ident("filter"))),
            vec![
                CallArg::Positional(Expr::List(
                    vec![
                        Expr::IntLit(-1, ds()),
                        Expr::IntLit(2, ds()),
                        Expr::IntLit(-3, ds()),
                        Expr::IntLit(4, ds()),
                    ],
                    ds(),
                )),
                CallArg::Positional(mk_lambda_is_positive()),
            ],
            ds(),
        );
        assert_eq!(
            ev.eval_expr(&expr, scope).unwrap(),
            Value::List(vec![Value::Int(2), Value::Int(4)])
        );
    }

    #[test]
    fn eval_reduce_ho() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        // reduce([1, 2, 3], 0, (acc, x) => acc + x) == 6
        let expr = Expr::FnCall(
            Box::new(Expr::Ident(mk_ident("reduce"))),
            vec![
                CallArg::Positional(Expr::List(
                    vec![
                        Expr::IntLit(1, ds()),
                        Expr::IntLit(2, ds()),
                        Expr::IntLit(3, ds()),
                    ],
                    ds(),
                )),
                CallArg::Positional(Expr::IntLit(0, ds())),
                CallArg::Positional(Expr::Lambda(
                    vec![mk_ident("acc"), mk_ident("x")],
                    Box::new(Expr::BinaryOp(
                        Box::new(Expr::Ident(mk_ident("acc"))),
                        BinOp::Add,
                        Box::new(Expr::Ident(mk_ident("x"))),
                        ds(),
                    )),
                    ds(),
                )),
            ],
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Int(6));
    }

    #[test]
    fn eval_every_ho() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::FnCall(
            Box::new(Expr::Ident(mk_ident("every"))),
            vec![
                CallArg::Positional(Expr::List(
                    vec![
                        Expr::IntLit(1, ds()),
                        Expr::IntLit(2, ds()),
                        Expr::IntLit(3, ds()),
                    ],
                    ds(),
                )),
                CallArg::Positional(mk_lambda_is_positive()),
            ],
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Bool(true));
    }

    #[test]
    fn eval_some_ho() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::FnCall(
            Box::new(Expr::Ident(mk_ident("some"))),
            vec![
                CallArg::Positional(Expr::List(
                    vec![
                        Expr::IntLit(-1, ds()),
                        Expr::IntLit(-2, ds()),
                        Expr::IntLit(3, ds()),
                    ],
                    ds(),
                )),
                CallArg::Positional(mk_lambda_is_positive()),
            ],
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Bool(true));
    }

    #[test]
    fn eval_count_ho() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::FnCall(
            Box::new(Expr::Ident(mk_ident("count"))),
            vec![
                CallArg::Positional(Expr::List(
                    vec![
                        Expr::IntLit(-1, ds()),
                        Expr::IntLit(2, ds()),
                        Expr::IntLit(-3, ds()),
                        Expr::IntLit(4, ds()),
                    ],
                    ds(),
                )),
                CallArg::Positional(mk_lambda_is_positive()),
            ],
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Int(2));
    }

    // ── Block expressions ────────────────────────────────────────────

    #[test]
    fn eval_block_expr() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::BlockExpr(
            vec![LetBinding {
                decorators: vec![],
                partial: false,
                name: mk_ident("x"),
                value: Expr::IntLit(10, ds()),
                trivia: wcl_core::trivia::Trivia::empty(),
                span: ds(),
            }],
            Box::new(Expr::BinaryOp(
                Box::new(Expr::Ident(mk_ident("x"))),
                BinOp::Mul,
                Box::new(Expr::IntLit(2, ds())),
                ds(),
            )),
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Int(20));
    }

    // ── Regex match operator ─────────────────────────────────────────

    #[test]
    fn eval_regex_match_op() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::BinaryOp(
            Box::new(Expr::StringLit(StringLit {
                parts: vec![StringPart::Literal("hello123".to_string())],
                span: ds(),
            })),
            BinOp::Match,
            Box::new(Expr::StringLit(StringLit {
                parts: vec![StringPart::Literal(r"\d+".to_string())],
                span: ds(),
            })),
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Bool(true));
    }

    // ── Lambda as value ──────────────────────────────────────────────

    #[test]
    fn eval_lambda_call() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        // let f = x => x + 1; then call f(5)
        ev.scopes.add_entry(
            scope,
            ScopeEntry {
                name: "f".to_string(),
                kind: ScopeEntryKind::LetBinding,
                value: Some(Value::Function(FunctionValue {
                    params: vec!["x".to_string()],
                    body: FunctionBody::UserDefined(Box::new(Expr::BinaryOp(
                        Box::new(Expr::Ident(mk_ident("x"))),
                        BinOp::Add,
                        Box::new(Expr::IntLit(1, ds())),
                        ds(),
                    ))),
                    closure_scope: Some(scope),
                })),
                span: ds(),
                dependencies: Default::default(),
                evaluated: true,
                read_count: 0,
            },
        );

        let expr = Expr::FnCall(
            Box::new(Expr::Ident(mk_ident("f"))),
            vec![CallArg::Positional(Expr::IntLit(5, ds()))],
            ds(),
        );
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Int(6));
    }

    // ── Null literal ─────────────────────────────────────────────────

    #[test]
    fn eval_null() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::NullLit(ds());
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Null);
    }

    // ── Paren pass-through ───────────────────────────────────────────

    #[test]
    fn eval_paren() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::Paren(Box::new(Expr::IntLit(42, ds())), ds());
        assert_eq!(ev.eval_expr(&expr, scope).unwrap(), Value::Int(42));
    }

    // ── ref() resolution ────────────────────────────────────────────

    fn mk_string_lit(s: &str) -> StringLit {
        StringLit {
            parts: vec![StringPart::Literal(s.to_string())],
            span: ds(),
        }
    }

    fn mk_attr(name: &str, value: Expr) -> BodyItem {
        BodyItem::Attribute(Attribute {
            decorators: vec![],
            name: mk_ident(name),
            value,
            trivia: wcl_core::trivia::Trivia::empty(),
            span: ds(),
        })
    }

    fn mk_block(kind: &str, id: &str, body: Vec<BodyItem>) -> BodyItem {
        BodyItem::Block(Block {
            decorators: vec![],
            partial: false,
            kind: mk_ident(kind),
            inline_id: Some(InlineId::Literal(IdentifierLit {
                value: id.to_string(),
                span: ds(),
            })),
            inline_args: vec![],
            body,
            text_content: None,
            trivia: wcl_core::trivia::Trivia::empty(),
            span: ds(),
        })
    }

    fn mk_doc(items: Vec<BodyItem>) -> Document {
        Document {
            items: items.into_iter().map(DocItem::Body).collect(),
            trivia: wcl_core::trivia::Trivia::empty(),
            span: ds(),
        }
    }

    #[test]
    fn test_ref_resolves_block() {
        // service "svc-auth" { port = 8080 }
        // target = ref(svc-auth)
        let doc = mk_doc(vec![
            mk_block(
                "service",
                "svc-auth",
                vec![mk_attr("port", Expr::IntLit(8080, ds()))],
            ),
            mk_attr(
                "target",
                Expr::Ref(
                    IdentifierLit {
                        value: "svc-auth".to_string(),
                        span: ds(),
                    },
                    ds(),
                ),
            ),
        ]);

        let mut ev = Evaluator::new();
        let result = ev.evaluate(&doc);
        assert!(
            !ev.diagnostics.has_errors(),
            "unexpected errors: {:?}",
            ev.diagnostics.diagnostics()
        );

        let target = result.get("target").expect("target should exist");
        match target {
            Value::BlockRef(br) => {
                assert_eq!(br.kind, "service");
                assert_eq!(br.id, Some("svc-auth".to_string()));
                assert_eq!(br.attributes.get("port"), Some(&Value::Int(8080)));
            }
            other => panic!("expected BlockRef, got {:?}", other),
        }
    }

    #[test]
    fn test_ref_member_access() {
        // service "svc-auth" { port = 8080 }
        // auth_port = ref(svc-auth).port
        let doc = mk_doc(vec![
            mk_block(
                "service",
                "svc-auth",
                vec![mk_attr("port", Expr::IntLit(8080, ds()))],
            ),
            mk_attr(
                "auth_port",
                Expr::MemberAccess(
                    Box::new(Expr::Ref(
                        IdentifierLit {
                            value: "svc-auth".to_string(),
                            span: ds(),
                        },
                        ds(),
                    )),
                    mk_ident("port"),
                    ds(),
                ),
            ),
        ]);

        let mut ev = Evaluator::new();
        let result = ev.evaluate(&doc);
        assert!(
            !ev.diagnostics.has_errors(),
            "unexpected errors: {:?}",
            ev.diagnostics.diagnostics()
        );
        assert_eq!(result.get("auth_port"), Some(&Value::Int(8080)));
    }

    #[test]
    fn test_ref_undefined_id_errors() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);
        let expr = Expr::Ref(
            IdentifierLit {
                value: "nonexistent".to_string(),
                span: ds(),
            },
            ds(),
        );
        let err = ev.eval_expr(&expr, scope).unwrap_err();
        assert!(
            err.message.contains("not found"),
            "expected 'not found' error, got: {}",
            err.message
        );
    }

    // ── import_raw() ────────────────────────────────────────────────

    #[test]
    fn test_import_raw_reads_file() {
        use crate::imports::InMemoryFs;
        use std::path::PathBuf;

        let mut fs = InMemoryFs::new();
        fs.add_file(PathBuf::from("/project/data.txt"), "hello world");
        let mut ev = Evaluator::with_fs(Box::new(fs), PathBuf::from("/project"));
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);

        let expr = Expr::ImportRaw(mk_string_lit("data.txt"), ds());
        let result = ev.eval_expr(&expr, scope).unwrap();
        assert_eq!(result, Value::String("hello world".to_string()));
    }

    #[test]
    fn test_import_raw_missing_file_errors() {
        use crate::imports::InMemoryFs;
        use std::path::PathBuf;

        let fs = InMemoryFs::new();
        let mut ev = Evaluator::with_fs(Box::new(fs), PathBuf::from("/project"));
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);

        let expr = Expr::ImportRaw(mk_string_lit("missing.txt"), ds());
        let err = ev.eval_expr(&expr, scope).unwrap_err();
        assert!(
            err.message.contains("cannot read file"),
            "expected read error, got: {}",
            err.message
        );
    }

    #[test]
    fn test_import_raw_jail_escape_rejected() {
        use crate::imports::InMemoryFs;
        use std::path::PathBuf;

        let mut fs = InMemoryFs::new();
        fs.add_file(PathBuf::from("/etc/passwd"), "root:x:0:0");
        let mut ev = Evaluator::with_fs(Box::new(fs), PathBuf::from("/project"));
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);

        let expr = Expr::ImportRaw(mk_string_lit("../../etc/passwd"), ds());
        let err = ev.eval_expr(&expr, scope).unwrap_err();
        assert!(
            err.message.contains("escapes root"),
            "expected jail escape error, got: {}",
            err.message
        );
    }

    // ── import_table() ──────────────────────────────────────────────

    #[test]
    fn test_import_table_csv() {
        use crate::imports::InMemoryFs;
        use std::path::PathBuf;

        let mut fs = InMemoryFs::new();
        fs.add_file(
            PathBuf::from("/project/services.csv"),
            "name,port\nauth,8080\napi,9090",
        );
        let mut ev = Evaluator::with_fs(Box::new(fs), PathBuf::from("/project"));
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);

        let expr = Expr::ImportTable(
            ImportTableArgs {
                path: mk_string_lit("services.csv"),
                separator: None,
                headers: None,
                columns: None,
            },
            ds(),
        );
        let result = ev.eval_expr(&expr, scope).unwrap();

        let mut row1 = IndexMap::new();
        row1.insert("name".to_string(), Value::String("auth".to_string()));
        row1.insert("port".to_string(), Value::String("8080".to_string()));
        let mut row2 = IndexMap::new();
        row2.insert("name".to_string(), Value::String("api".to_string()));
        row2.insert("port".to_string(), Value::String("9090".to_string()));

        assert_eq!(
            result,
            Value::List(vec![Value::Map(row1), Value::Map(row2)])
        );
    }

    #[test]
    fn test_import_table_tsv_explicit_separator() {
        use crate::imports::InMemoryFs;
        use std::path::PathBuf;

        let mut fs = InMemoryFs::new();
        fs.add_file(
            PathBuf::from("/project/services.tsv"),
            "name\tport\nauth\t8080\napi\t9090",
        );
        let mut ev = Evaluator::with_fs(Box::new(fs), PathBuf::from("/project"));
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);

        let expr = Expr::ImportTable(
            ImportTableArgs {
                path: mk_string_lit("services.tsv"),
                separator: Some(mk_string_lit("\t")),
                headers: None,
                columns: None,
            },
            ds(),
        );
        let result = ev.eval_expr(&expr, scope).unwrap();

        let mut row1 = IndexMap::new();
        row1.insert("name".to_string(), Value::String("auth".to_string()));
        row1.insert("port".to_string(), Value::String("8080".to_string()));
        let mut row2 = IndexMap::new();
        row2.insert("name".to_string(), Value::String("api".to_string()));
        row2.insert("port".to_string(), Value::String("9090".to_string()));

        assert_eq!(
            result,
            Value::List(vec![Value::Map(row1), Value::Map(row2)])
        );
    }

    #[test]
    fn test_import_table_empty_file() {
        use crate::imports::InMemoryFs;
        use std::path::PathBuf;

        let mut fs = InMemoryFs::new();
        fs.add_file(PathBuf::from("/project/empty.csv"), "");
        let mut ev = Evaluator::with_fs(Box::new(fs), PathBuf::from("/project"));
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);

        let expr = Expr::ImportTable(
            ImportTableArgs {
                path: mk_string_lit("empty.csv"),
                separator: None,
                headers: None,
                columns: None,
            },
            ds(),
        );
        let result = ev.eval_expr(&expr, scope).unwrap();
        assert_eq!(result, Value::List(vec![]));
    }

    #[test]
    fn test_import_table_headers_false() {
        use crate::imports::InMemoryFs;
        use std::path::PathBuf;

        let mut fs = InMemoryFs::new();
        fs.add_file(PathBuf::from("/project/data.csv"), "auth,8080\napi,9090");
        let mut ev = Evaluator::with_fs(Box::new(fs), PathBuf::from("/project"));
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);

        let expr = Expr::ImportTable(
            ImportTableArgs {
                path: mk_string_lit("data.csv"),
                separator: None,
                headers: Some(false),
                columns: None,
            },
            ds(),
        );
        let result = ev.eval_expr(&expr, scope).unwrap();

        // With headers=false and no columns, keys should be "0", "1"
        let mut row1 = IndexMap::new();
        row1.insert("0".to_string(), Value::String("auth".to_string()));
        row1.insert("1".to_string(), Value::String("8080".to_string()));
        let mut row2 = IndexMap::new();
        row2.insert("0".to_string(), Value::String("api".to_string()));
        row2.insert("1".to_string(), Value::String("9090".to_string()));

        assert_eq!(
            result,
            Value::List(vec![Value::Map(row1), Value::Map(row2)])
        );
    }

    #[test]
    fn test_import_table_explicit_columns() {
        use crate::imports::InMemoryFs;
        use std::path::PathBuf;

        let mut fs = InMemoryFs::new();
        fs.add_file(PathBuf::from("/project/data.csv"), "auth,8080\napi,9090");
        let mut ev = Evaluator::with_fs(Box::new(fs), PathBuf::from("/project"));
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);

        let expr = Expr::ImportTable(
            ImportTableArgs {
                path: mk_string_lit("data.csv"),
                separator: None,
                headers: Some(false),
                columns: Some(vec![mk_string_lit("name"), mk_string_lit("port")]),
            },
            ds(),
        );
        let result = ev.eval_expr(&expr, scope).unwrap();

        let mut row1 = IndexMap::new();
        row1.insert("name".to_string(), Value::String("auth".to_string()));
        row1.insert("port".to_string(), Value::String("8080".to_string()));
        let mut row2 = IndexMap::new();
        row2.insert("name".to_string(), Value::String("api".to_string()));
        row2.insert("port".to_string(), Value::String("9090".to_string()));

        assert_eq!(
            result,
            Value::List(vec![Value::Map(row1), Value::Map(row2)])
        );
    }

    #[test]
    fn test_import_table_explicit_columns_skip_header() {
        use crate::imports::InMemoryFs;
        use std::path::PathBuf;

        let mut fs = InMemoryFs::new();
        fs.add_file(
            PathBuf::from("/project/data.csv"),
            "old_name,old_port\nauth,8080\napi,9090",
        );
        let mut ev = Evaluator::with_fs(Box::new(fs), PathBuf::from("/project"));
        let scope = ev.scopes.create_scope(ScopeKind::Module, None);

        // headers=true (default) + explicit columns: skip header, use explicit names
        let expr = Expr::ImportTable(
            ImportTableArgs {
                path: mk_string_lit("data.csv"),
                separator: None,
                headers: None, // defaults to true
                columns: Some(vec![mk_string_lit("name"), mk_string_lit("port")]),
            },
            ds(),
        );
        let result = ev.eval_expr(&expr, scope).unwrap();

        let mut row1 = IndexMap::new();
        row1.insert("name".to_string(), Value::String("auth".to_string()));
        row1.insert("port".to_string(), Value::String("8080".to_string()));
        let mut row2 = IndexMap::new();
        row2.insert("name".to_string(), Value::String("api".to_string()));
        row2.insert("port".to_string(), Value::String("9090".to_string()));

        assert_eq!(
            result,
            Value::List(vec![Value::Map(row1), Value::Map(row2)])
        );
    }

    // ── Variable warning helpers ────────────────────────────────────

    fn mk_let(name: &str, value: Expr) -> BodyItem {
        BodyItem::LetBinding(LetBinding {
            decorators: vec![],
            partial: false,
            name: mk_ident(name),
            value,
            trivia: wcl_core::trivia::Trivia::empty(),
            span: ds(),
        })
    }

    fn mk_let_with_decorators(name: &str, value: Expr, decorators: Vec<Decorator>) -> BodyItem {
        BodyItem::LetBinding(LetBinding {
            decorators,
            partial: false,
            name: mk_ident(name),
            value,
            trivia: wcl_core::trivia::Trivia::empty(),
            span: ds(),
        })
    }

    fn count_warnings_with_code(ev: &Evaluator, code: &str) -> usize {
        ev.diagnostics()
            .diagnostics()
            .iter()
            .filter(|d| {
                d.severity == wcl_core::diagnostic::Severity::Warning
                    && d.code.as_deref() == Some(code)
            })
            .count()
    }

    // ── W001: Shadowing warnings ────────────────────────────────────

    #[test]
    fn shadowing_let_produces_w001() {
        // let x = 1
        // service "s" { let x = 2; port = x }
        let doc = mk_doc(vec![
            mk_let("x", Expr::IntLit(1, ds())),
            mk_block(
                "service",
                "s",
                vec![
                    mk_let("x", Expr::IntLit(2, ds())),
                    mk_attr("port", Expr::Ident(mk_ident("x"))),
                ],
            ),
        ]);

        let mut ev = Evaluator::new();
        let _result = ev.evaluate(&doc);
        assert_eq!(count_warnings_with_code(&ev, "W001"), 1);
    }

    #[test]
    fn no_shadowing_no_w001() {
        // let x = 1
        // let y = 2
        let doc = mk_doc(vec![
            mk_let("x", Expr::IntLit(1, ds())),
            mk_let("y", Expr::IntLit(2, ds())),
        ]);

        let mut ev = Evaluator::new();
        let _result = ev.evaluate(&doc);
        assert_eq!(count_warnings_with_code(&ev, "W001"), 0);
    }

    // ── W002: Unused variable warnings ──────────────────────────────

    #[test]
    fn unused_let_produces_w002() {
        // let x = 1
        // port = 42
        let doc = mk_doc(vec![
            mk_let("x", Expr::IntLit(1, ds())),
            mk_attr("port", Expr::IntLit(42, ds())),
        ]);

        let mut ev = Evaluator::new();
        let _result = ev.evaluate(&doc);
        assert_eq!(count_warnings_with_code(&ev, "W002"), 1);
    }

    #[test]
    fn used_let_no_w002() {
        // let x = 1
        // port = x
        let doc = mk_doc(vec![
            mk_let("x", Expr::IntLit(1, ds())),
            mk_attr("port", Expr::Ident(mk_ident("x"))),
        ]);

        let mut ev = Evaluator::new();
        let _result = ev.evaluate(&doc);
        assert_eq!(count_warnings_with_code(&ev, "W002"), 0);
    }

    #[test]
    fn underscore_prefix_suppresses_w002() {
        // let _x = 1
        // port = 42
        let doc = mk_doc(vec![
            mk_let("_x", Expr::IntLit(1, ds())),
            mk_attr("port", Expr::IntLit(42, ds())),
        ]);

        let mut ev = Evaluator::new();
        let _result = ev.evaluate(&doc);
        assert_eq!(count_warnings_with_code(&ev, "W002"), 0);
    }

    // ── @allow(shadowing) suppression ───────────────────────────────

    #[test]
    fn allow_shadowing_suppresses_w001() {
        // let x = 1
        // service "s" { @allow(shadowing) let x = 2; port = x }
        let allow_decorator = Decorator {
            name: mk_ident("allow"),
            args: vec![DecoratorArg::Positional(Expr::Ident(mk_ident("shadowing")))],
            span: ds(),
        };

        let doc = mk_doc(vec![
            mk_let("x", Expr::IntLit(1, ds())),
            mk_block(
                "service",
                "s",
                vec![
                    mk_let_with_decorators("x", Expr::IntLit(2, ds()), vec![allow_decorator]),
                    mk_attr("port", Expr::Ident(mk_ident("x"))),
                ],
            ),
        ]);

        let mut ev = Evaluator::new();
        let _result = ev.evaluate(&doc);
        assert_eq!(count_warnings_with_code(&ev, "W001"), 0);
    }

    #[test]
    fn allow_other_does_not_suppress_w001() {
        // let x = 1
        // service "s" { @allow(unused) let x = 2; port = x }
        let allow_decorator = Decorator {
            name: mk_ident("allow"),
            args: vec![DecoratorArg::Positional(Expr::Ident(mk_ident("unused")))],
            span: ds(),
        };

        let doc = mk_doc(vec![
            mk_let("x", Expr::IntLit(1, ds())),
            mk_block(
                "service",
                "s",
                vec![
                    mk_let_with_decorators("x", Expr::IntLit(2, ds()), vec![allow_decorator]),
                    mk_attr("port", Expr::Ident(mk_ident("x"))),
                ],
            ),
        ]);

        let mut ev = Evaluator::new();
        let _result = ev.evaluate(&doc);
        assert_eq!(count_warnings_with_code(&ev, "W001"), 1);
    }

    // ── Text block evaluator tests ──────────────────────────────────────

    #[test]
    fn text_block_evaluates_to_blockref_with_content() {
        let doc = mk_doc(vec![BodyItem::Block(Block {
            decorators: vec![],
            partial: false,
            kind: mk_ident("readme"),
            inline_id: Some(InlineId::Literal(IdentifierLit {
                value: "my-doc".to_string(),
                span: ds(),
            })),
            inline_args: vec![],
            body: vec![],
            text_content: Some(StringLit {
                parts: vec![StringPart::Literal("Hello world".to_string())],
                span: ds(),
            }),
            trivia: wcl_core::trivia::Trivia::empty(),
            span: ds(),
        })]);

        let mut ev = Evaluator::new();
        let result = ev.evaluate(&doc);
        assert!(
            !ev.diagnostics.has_errors(),
            "unexpected errors: {:?}",
            ev.diagnostics.diagnostics()
        );

        let block_val = result.get("my-doc").expect("block should exist");
        match block_val {
            Value::BlockRef(br) => {
                assert_eq!(br.kind, "readme");
                assert_eq!(br.id, Some("my-doc".to_string()));
                assert_eq!(
                    br.attributes.get("content"),
                    Some(&Value::String("Hello world".to_string()))
                );
            }
            other => panic!("expected BlockRef, got {:?}", other),
        }
    }

    #[test]
    fn text_block_with_interpolation() {
        // let name = "World"
        // readme my-doc "Hello ${name}!"
        let doc = mk_doc(vec![
            BodyItem::LetBinding(LetBinding {
                decorators: vec![],
                partial: false,
                name: mk_ident("name"),
                value: Expr::StringLit(StringLit {
                    parts: vec![StringPart::Literal("World".to_string())],
                    span: ds(),
                }),
                trivia: wcl_core::trivia::Trivia::empty(),
                span: ds(),
            }),
            BodyItem::Block(Block {
                decorators: vec![],
                partial: false,
                kind: mk_ident("readme"),
                inline_id: Some(InlineId::Literal(IdentifierLit {
                    value: "my-doc".to_string(),
                    span: ds(),
                })),
                inline_args: vec![],
                body: vec![],
                text_content: Some(StringLit {
                    parts: vec![
                        StringPart::Literal("Hello ".to_string()),
                        StringPart::Interpolation(Box::new(Expr::Ident(mk_ident("name")))),
                        StringPart::Literal("!".to_string()),
                    ],
                    span: ds(),
                }),
                trivia: wcl_core::trivia::Trivia::empty(),
                span: ds(),
            }),
        ]);

        let mut ev = Evaluator::new();
        let result = ev.evaluate(&doc);
        assert!(
            !ev.diagnostics.has_errors(),
            "unexpected errors: {:?}",
            ev.diagnostics.diagnostics()
        );

        let block_val = result.get("my-doc").expect("block should exist");
        match block_val {
            Value::BlockRef(br) => {
                assert_eq!(
                    br.attributes.get("content"),
                    Some(&Value::String("Hello World!".to_string()))
                );
            }
            other => panic!("expected BlockRef, got {:?}", other),
        }
    }
}
