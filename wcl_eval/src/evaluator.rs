use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;
use wcl_core::ast::*;
use wcl_core::diagnostic::{Diagnostic, DiagnosticBag};
use wcl_core::span::Span;

use crate::functions::{builtin_registry, BuiltinFn};
use crate::scope::*;
use crate::value::*;

pub struct Evaluator {
    scopes: ScopeArena,
    builtins: HashMap<&'static str, BuiltinFn>,
    diagnostics: DiagnosticBag,
}

impl Evaluator {
    pub fn new() -> Self {
        Evaluator {
            scopes: ScopeArena::new(),
            builtins: builtin_registry(),
            diagnostics: DiagnosticBag::new(),
        }
    }

    /// Evaluate a full document. Returns the evaluated document as a list of
    /// (key, Value) pairs representing the resolved content.
    pub fn evaluate(&mut self, doc: &Document) -> IndexMap<String, Value> {
        let module_scope = self.scopes.create_scope(ScopeKind::Module, None);

        // Phase 1: Register all names in scope (let bindings, attributes, blocks)
        self.register_doc_items(&doc.items, module_scope);

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
                self.diagnostics.error(
                    format!("cyclic dependency detected: {}", cycle.join(" -> ")),
                    Span::dummy(),
                );
            }
        }

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
                        },
                    );
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
                    },
                );
            }
            BodyItem::LetBinding(lb) => {
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
                self.scopes.add_entry(
                    scope_id,
                    ScopeEntry {
                        name,
                        kind: ScopeEntryKind::BlockChild,
                        value: None,
                        span: block.span,
                        dependencies: Default::default(),
                        evaluated: false,
                    },
                );
                self.register_block_body(&block.body, child_scope);
            }
            _ => {}
        }
    }

    fn register_block_body(&mut self, body: &[BodyItem], scope_id: ScopeId) {
        for item in body {
            self.register_body_item(item, scope_id);
        }
    }

    // ------------------------------------------------------------------
    // Evaluate a named entry from the document items
    // ------------------------------------------------------------------

    fn evaluate_doc_entry(&mut self, items: &[DocItem], scope_id: ScopeId, name: &str) {
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
                        // Mark as evaluated (block evaluation is handled separately)
                        if let Some((_, entry)) = self.scopes.resolve_mut(scope_id, name) {
                            entry.evaluated = true;
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
                for filter in &pipeline.filters {
                    if let QueryFilter::AttrComparison(_, _, expr) = filter {
                        self.collect_deps(expr, deps);
                    }
                }
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
            Expr::BinaryOp(lhs, op, rhs, span) => {
                self.eval_binary(lhs, *op, rhs, *span, scope_id)
            }
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
            Expr::FnCall(callee, args, span) => {
                self.eval_fn_call(callee, args, *span, scope_id)
            }
            Expr::Lambda(params, body, _span) => Ok(Value::Function(FunctionValue {
                params: params.iter().map(|p| p.name.clone()).collect(),
                body: FunctionBody::UserDefined(body.clone()),
                closure_scope: Some(scope_id),
            })),
            Expr::BlockExpr(lets, final_expr, _) => {
                let block_scope =
                    self.scopes
                        .create_scope(ScopeKind::Lambda, Some(scope_id));
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
                        },
                    );
                }
                self.eval_expr(final_expr, block_scope)
            }
            Expr::Query(pipeline, span) => self.eval_query(pipeline, *span, scope_id),
            Expr::Ref(id, _span) => {
                // Resolve block reference by identifier
                Ok(Value::Identifier(id.value.clone()))
            }
            Expr::ImportRaw(_path, span) => {
                Err(Diagnostic::error("import_raw not available in this context", *span))
            }
            Expr::ImportTable(_path, _sep, span) => Err(Diagnostic::error(
                "import_table not available in this context",
                *span,
            )),
            Expr::Paren(e, _) => self.eval_expr(e, scope_id),
        }
    }

    // ------------------------------------------------------------------
    // String evaluation
    // ------------------------------------------------------------------

    fn eval_string_lit(
        &mut self,
        s: &StringLit,
        scope_id: ScopeId,
    ) -> Result<Value, Diagnostic> {
        let mut result = String::new();
        for part in &s.parts {
            match part {
                StringPart::Literal(text) => result.push_str(text),
                StringPart::Interpolation(expr) => {
                    let val = self.eval_expr(expr, scope_id)?;
                    match val.to_interp_string() {
                        Ok(s) => result.push_str(&s),
                        Err(e) => {
                            return Err(
                                Diagnostic::error(e, s.span).with_code("E050")
                            )
                        }
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
    // Identifier resolution
    // ------------------------------------------------------------------

    fn eval_ident(
        &mut self,
        ident: &Ident,
        scope_id: ScopeId,
    ) -> Result<Value, Diagnostic> {
        if let Some((_, entry)) = self.scopes.resolve(scope_id, &ident.name) {
            if let Some(ref val) = entry.value {
                Ok(val.clone())
            } else {
                Err(Diagnostic::error(
                    format!("variable '{}' has not been evaluated yet", ident.name),
                    ident.span,
                )
                .with_code("E040"))
            }
        } else {
            Err(Diagnostic::error(
                format!("undefined reference '{}'", ident.name),
                ident.span,
            )
            .with_code("E040"))
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
                _ => Err(
                    Diagnostic::error("&& requires bool operands", span).with_code("E050")
                ),
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
                _ => Err(
                    Diagnostic::error("|| requires bool operands", span).with_code("E050")
                ),
            };
        }

        let l = self.eval_expr(lhs, scope_id)?;
        let r = self.eval_expr(rhs, scope_id)?;

        match op {
            BinOp::Add => self.eval_add(&l, &r, span),
            BinOp::Sub => {
                self.eval_arithmetic(&l, &r, span, |a, b| a - b, |a, b| a - b)
            }
            BinOp::Mul => {
                self.eval_arithmetic(&l, &r, span, |a, b| a * b, |a, b| a * b)
            }
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
            (Value::String(a), Value::String(b)) => {
                Ok(Value::String(format!("{}{}", a, b)))
            }
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
            (Value::Int(a), Value::Float(b)) => {
                Ok(Value::Float(float_op(*a as f64, *b)))
            }
            (Value::Float(a), Value::Int(b)) => {
                Ok(Value::Float(float_op(*a, *b as f64)))
            }
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
                    return Err(
                        Diagnostic::error("division by zero", span).with_code("E051")
                    );
                }
                Ok(Value::Float(a / b))
            }
            (Value::Int(a), Value::Float(b)) => {
                if *b == 0.0 {
                    return Err(
                        Diagnostic::error("division by zero", span).with_code("E051")
                    );
                }
                Ok(Value::Float(*a as f64 / b))
            }
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a / *b as f64)),
            _ => Err(
                Diagnostic::error("division requires numeric operands", span)
                    .with_code("E050"),
            ),
        }
    }

    fn eval_mod(&self, l: &Value, r: &Value, span: Span) -> Result<Value, Diagnostic> {
        match (l, r) {
            (Value::Int(a), Value::Int(b)) => {
                if *b == 0 {
                    return Err(
                        Diagnostic::error("modulo by zero", span).with_code("E051")
                    );
                }
                Ok(Value::Int(a % b))
            }
            _ => Err(
                Diagnostic::error("modulo requires int operands", span).with_code("E050")
            ),
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
            (Value::Int(a), Value::Float(b)) => {
                compare_partial_ord(&(*a as f64), b, op)
            }
            (Value::Float(a), Value::Int(b)) => {
                compare_partial_ord(a, &(*b as f64), op)
            }
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

    fn eval_regex_match(
        &self,
        l: &Value,
        r: &Value,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match (l, r) {
            (Value::String(s), Value::String(pattern)) => {
                match regex::Regex::new(pattern) {
                    Ok(re) => Ok(Value::Bool(re.is_match(s))),
                    Err(e) => Err(
                        Diagnostic::error(format!("invalid regex: {}", e), span)
                            .with_code("E050"),
                    ),
                }
            }
            _ => Err(
                Diagnostic::error("=~ requires string operands", span).with_code("E050")
            ),
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
                _ => Err(
                    Diagnostic::error("! requires bool operand", span).with_code("E050")
                ),
            },
            UnaryOp::Neg => match val {
                Value::Int(i) => Ok(Value::Int(-i)),
                Value::Float(f) => Ok(Value::Float(-f)),
                _ => Err(
                    Diagnostic::error("unary - requires numeric operand", span)
                        .with_code("E050"),
                ),
            },
        }
    }

    // ------------------------------------------------------------------
    // Member / index access
    // ------------------------------------------------------------------

    fn access_member(
        &self,
        val: &Value,
        field: &str,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match val {
            Value::Map(m) => m.get(field).cloned().ok_or_else(|| {
                Diagnostic::error(format!("key '{}' not found in map", field), span)
                    .with_code("E054")
            }),
            Value::BlockRef(br) => br.attributes.get(field).cloned().ok_or_else(|| {
                Diagnostic::error(
                    format!("attribute '{}' not found in block", field),
                    span,
                )
                .with_code("E054")
            }),
            _ => Err(Diagnostic::error(
                format!("cannot access member on {}", val.type_name()),
                span,
            )
            .with_code("E050")),
        }
    }

    fn access_index(
        &self,
        val: &Value,
        idx: &Value,
        span: Span,
    ) -> Result<Value, Diagnostic> {
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
            (Value::Map(m), Value::String(key)) => {
                m.get(key).cloned().ok_or_else(|| {
                    Diagnostic::error(
                        format!("key '{}' not found in map", key),
                        span,
                    )
                    .with_code("E054")
                })
            }
            _ => Err(Diagnostic::error(
                format!(
                    "cannot index {} with {}",
                    val.type_name(),
                    idx.type_name()
                ),
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

                // Evaluate arguments eagerly for normal builtins and user fns
                let eval_args = self.eval_call_args(args, scope_id)?;

                // Check builtin functions
                if let Some(builtin) = self.builtins.get(name.as_str()) {
                    return builtin(&eval_args).map_err(|e| {
                        Diagnostic::error(format!("in {}(): {}", name, e), span)
                            .with_code("E052")
                    });
                }

                // Check user-defined functions in scope
                if let Some((_, entry)) = self.scopes.resolve(scope_id, name) {
                    if let Some(Value::Function(func)) = &entry.value {
                        let func = func.clone();
                        return self.call_user_fn(&func, &eval_args, span);
                    }
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
                    _ => Err(
                        Diagnostic::error("not a callable value", span).with_code("E050")
                    ),
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
        let call_scope =
            self.scopes
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
                        },
                    );
                }
                self.eval_expr(final_expr, call_scope)
            }
            FunctionBody::Builtin(name) => {
                // Builtins are handled in eval_fn_call, not here
                if let Some(builtin) = self.builtins.get(name.as_str()) {
                    builtin(args).map_err(|e| {
                        Diagnostic::error(format!("in {}(): {}", name, e), span)
                            .with_code("E052")
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
                    results.push(self.call_user_fn(&func, &[item.clone()], span)?);
                }
                Ok(Value::List(results))
            }
            "filter" => {
                self.expect_ho_args(2, args.len(), "filter", span)?;
                let list = self.eval_call_arg(&args[0], scope_id)?;
                let func =
                    self.eval_call_arg_as_fn(&args[1], scope_id, "filter", span)?;
                let items = self.expect_list(list, "filter", span)?;
                let mut results = Vec::new();
                for item in &items {
                    let keep = self.call_user_fn(&func, &[item.clone()], span)?;
                    if keep == Value::Bool(true) {
                        results.push(item.clone());
                    }
                }
                Ok(Value::List(results))
            }
            "every" => {
                self.expect_ho_args(2, args.len(), "every", span)?;
                let list = self.eval_call_arg(&args[0], scope_id)?;
                let func =
                    self.eval_call_arg_as_fn(&args[1], scope_id, "every", span)?;
                let items = self.expect_list(list, "every", span)?;
                for item in &items {
                    let result = self.call_user_fn(&func, &[item.clone()], span)?;
                    if result != Value::Bool(true) {
                        return Ok(Value::Bool(false));
                    }
                }
                Ok(Value::Bool(true))
            }
            "some" => {
                self.expect_ho_args(2, args.len(), "some", span)?;
                let list = self.eval_call_arg(&args[0], scope_id)?;
                let func =
                    self.eval_call_arg_as_fn(&args[1], scope_id, "some", span)?;
                let items = self.expect_list(list, "some", span)?;
                for item in &items {
                    let result = self.call_user_fn(&func, &[item.clone()], span)?;
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
                let func =
                    self.eval_call_arg_as_fn(&args[2], scope_id, "reduce", span)?;
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
                let func =
                    self.eval_call_arg_as_fn(&args[1], scope_id, "count", span)?;
                let items = self.expect_list(list, "count", span)?;
                let mut n = 0i64;
                for item in &items {
                    let result = self.call_user_fn(&func, &[item.clone()], span)?;
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

    fn eval_call_arg(
        &mut self,
        arg: &CallArg,
        scope_id: ScopeId,
    ) -> Result<Value, Diagnostic> {
        match arg {
            CallArg::Positional(e) | CallArg::Named(_, e) => {
                self.eval_expr(e, scope_id)
            }
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

    fn expect_list(
        &self,
        val: Value,
        fn_name: &str,
        span: Span,
    ) -> Result<Vec<Value>, Diagnostic> {
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
        engine.execute(pipeline, &blocks, self, scope_id).map_err(|e| {
            Diagnostic::error(e, span).with_code("E050")
        })
    }

    fn collect_blocks(&self, scope_id: ScopeId) -> Vec<BlockRef> {
        let scope = self.scopes.get(scope_id);
        let mut blocks = Vec::new();
        for entry in scope.entries.values() {
            if let Some(Value::BlockRef(br)) = &entry.value {
                blocks.push(br.clone());
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

    /// Provide read access to the scope arena.
    pub fn scopes(&self) -> &ScopeArena {
        &self.scopes
    }

    /// Provide mutable access to the scope arena (used by the facade crate and query engine).
    pub fn scopes_mut(&mut self) -> &mut ScopeArena {
        &mut self.scopes
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
        let expr = Expr::FnCall(
            Box::new(Expr::Ident(mk_ident("nonexistent"))),
            vec![],
            ds(),
        );
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
}
