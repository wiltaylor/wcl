//! Document evaluator: Pratt-tree walker, function-call dispatch,
//! variant construction, scope-chain lookups, and the small
//! per-evaluation state (EvalCtx, LocalsFrame, EvalCaller).
//! Extracted from `doc.rs` so the parent file can stay focused
//! on the public Document API.

use crate::ast::{self, Span};
use crate::error::EvalError;
use crate::value::{FnParam, FnValue, Value};

use super::eval_ops::{apply_binary, apply_unary, as_bool, describe_expr, format_member_path};
use super::match_pat;
use super::scope::Scope;
use super::{
    Block, Document, expr_to_path_segments, materialise_dataref, materialise_dataref_or_path,
    span_of, value_matches_type_ref,
};

/// Hard cap on nested user-`fn` invocations during a single evaluation.
/// Prevents accidental recursion in a `Value::Function` body from blowing
/// the Rust stack; surfaces as [`EvalError::CallDepthExceeded`].
const MAX_CALL_DEPTH: usize = 256;

pub(crate) struct EvalCtx<'a> {
    /// Stack of name → value bindings introduced by `Block` let-bindings.
    /// Searched right-to-left so the most recent binding shadows older ones.
    locals: Vec<(String, Value)>,
    /// Lexical scope of the expression's evaluation site. Used to
    /// resolve bare identifiers and `self`/`parent`.
    scope: Scope<'a>,
    /// Current nested `Value::Function` invocation depth.
    call_depth: usize,
}

impl<'a> EvalCtx<'a> {
    pub(super) fn new(scope: Scope<'a>) -> Self {
        Self {
            locals: Vec::new(),
            scope,
            call_depth: 0,
        }
    }

    fn lookup(&self, name: &str) -> Option<&Value> {
        self.locals
            .iter()
            .rev()
            .find_map(|(n, v)| if n == name { Some(v) } else { None })
    }

    /// Push a fresh locals frame and return a guard whose `Drop` impl
    /// pops everything pushed during its lifetime. Used by `Block`,
    /// `IfLet`, `Match`-arm bindings, and function-call frames so an
    /// early `return Err(...)` can't leak bindings into the parent
    /// frame.
    fn push_frame(&mut self) -> LocalsFrame<'_, 'a> {
        let base = self.locals.len();
        LocalsFrame { ctx: self, base }
    }
}

/// RAII guard: on drop, truncates the wrapped `EvalCtx`'s `locals`
/// back to the length it had when the guard was created. Deref to the
/// underlying `EvalCtx` so calls like `frame.locals.push(...)` work
/// directly; pass `&mut *frame` where an `&mut EvalCtx<'a>` is
/// expected.
pub(crate) struct LocalsFrame<'c, 'a> {
    ctx: &'c mut EvalCtx<'a>,
    base: usize,
}

impl<'a> std::ops::Deref for LocalsFrame<'_, 'a> {
    type Target = EvalCtx<'a>;
    fn deref(&self) -> &Self::Target {
        self.ctx
    }
}

impl<'a> std::ops::DerefMut for LocalsFrame<'_, 'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.ctx
    }
}

impl Drop for LocalsFrame<'_, '_> {
    fn drop(&mut self) {
        self.ctx.locals.truncate(self.base);
    }
}

/// Resolve `name` to a `Value::Function`, if it has one bound in the
/// caller's locals or document scope. Returns `None` for anything else
/// (non-function values, missing names) so the caller can decide what
/// fallback to take (e.g. dispatch to a builtin by the same name).
fn lookup_function(doc: &Document, ctx: &EvalCtx<'_>, name: &str) -> Option<FnValue> {
    if let Some(Value::Function(fv)) = ctx.lookup(name) {
        return Some(fv.clone());
    }
    // A let-bound or scope-resolved value that fails to evaluate isn't a
    // function — fall through to None so the caller can try a builtin.
    // Use `DataRef::value()` (not `materialise_dataref`) so a `let f =
    // fn(...)` — which resolves to a pre-materialised `VariantValue` —
    // is unwrapped, not just a top-level `f = fn(...)` field.
    let dr = doc.scope_lookup(&ctx.scope, name)?.ok()?;
    match dr.value() {
        Ok(Value::Function(fv)) => Some(fv),
        _ => None,
    }
}

/// Extract a string payload from a builtin argument, or build a
/// `BuiltinTypeMismatch` error explaining what was expected. Used by
/// the `error`/`panic`/`assert` control-flow primitives in
/// `eval_call_builtin`.
fn string_arg_or_err(
    name: &str,
    v: &Value,
    span: Span,
    expected: &str,
) -> Result<String, EvalError> {
    match v {
        Value::Utf8(s) | Value::Ascii(s) => Ok(s.clone()),
        other => Err(EvalError::builtin_type(
            name.to_string(),
            format!("{name}: {expected}, got {}", other.type_name()),
            span,
        )),
    }
}

/// Attach the call site name to a generic call-arity error, so error
/// reporting for `myFn(1, 2)` mentions `myFn`. Other variants pass
/// through unchanged.
fn call_err_at(err: EvalError, name: String, span: Span) -> EvalError {
    match err {
        EvalError::CallArity { expected, got, .. } => {
            EvalError::builtin_arity(name, expected, got, span)
        }
        other => other,
    }
}

/// `Caller` impl used by the evaluator to invoke `Value::Function`
/// callbacks from inside HOF builtins. Holds a back-reference to the
/// document and the live `EvalCtx`, so the call observes (and reuses)
/// the surrounding evaluation's locals/scope/call_depth.
struct EvalCaller<'a, 'c> {
    doc: &'a Document,
    ctx: &'c mut EvalCtx<'a>,
    span: Span,
    /// If a user-function invocation surfaces an `EvalError`, we stash
    /// it here and return a string from `call_fn` so the builtin can
    /// short-circuit. The dispatch site re-raises the structured error.
    err: Option<EvalError>,
}

impl<'a> crate::builtins::Caller for EvalCaller<'a, '_> {
    fn call_fn(&mut self, f: &FnValue, args: &[Value]) -> Result<Value, String> {
        let _profile_guard = self.doc.profile_enter(crate::profile::ProfileKey::UserFn {
            name: String::new(),
        });
        match self.doc.invoke_fn_value(f, args, self.ctx, self.span) {
            Ok(v) => Ok(v),
            Err(e) => {
                let msg = e.to_string();
                self.err = Some(e);
                Err(msg)
            }
        }
    }

    fn resolve<'r>(&'r self, path: &[String]) -> Option<crate::data::DataRef<'r>> {
        let (first, rest) = path.split_first()?;
        let mut cur = self.doc.resolve_root(first)?;
        for seg in rest {
            cur = cur.child(seg)?;
        }
        Some(cur)
    }

    fn builtin_info(&self, name: &str) -> Option<crate::builtins::BuiltinSignature> {
        self.doc
            .environment()
            .builtin(name)
            .map(|b| b.signature_info())
    }

    fn builtin_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .doc
            .environment()
            .builtins()
            .map(|(name, _)| name.to_string())
            .collect();
        names.sort();
        names
    }
}

impl Document {
    /// Scope-aware expression evaluator. Bare identifiers and path
    /// expressions resolve via the supplied [`Scope`] chain, falling
    /// through to the document root. Unresolved names error with
    /// [`EvalError::UnresolvedReference`].
    pub(crate) fn eval_in_scope(
        &self,
        expr: &ast::Expr,
        scope: &Scope<'_>,
    ) -> Result<Value, EvalError> {
        let mut ctx = EvalCtx::new(scope.clone());
        self.eval_in(expr, &mut ctx)
    }

    /// Literal-mode evaluator for contexts that intentionally treat
    /// bare identifiers as opaque names (block labels). Any
    /// non-identifier expression is evaluated through the root scope.
    pub(crate) fn eval_literal(&self, expr: &ast::Expr) -> Result<Value, EvalError> {
        if let ast::Expr::Identifier(s, _) = expr {
            return Ok(Value::Identifier(s.clone()));
        }
        self.eval_in_scope(expr, &Scope::root())
    }

    /// Like [`eval_literal`] but evaluates non-identifier expressions in a
    /// given scope rather than root. Block labels use this so an
    /// interpolated `$"…${slot}…"` label resolves component/repeater
    /// bindings (and any enclosing names) while a bare identifier still
    /// stays an opaque literal name. Behaviour-identical to `eval_literal`
    /// for plain literal labels (their value is scope-independent).
    pub(crate) fn eval_literal_in_scope(
        &self,
        expr: &ast::Expr,
        scope: &Scope<'_>,
    ) -> Result<Value, EvalError> {
        if let ast::Expr::Identifier(s, _) = expr {
            return Ok(Value::Identifier(s.clone()));
        }
        self.eval_in_scope(expr, scope)
    }

    /// Back-compat shim — same as `eval_literal`. Used by call sites
    /// that pre-date the scope distinction (decorator args). Bare
    /// identifiers fall through; everything else evaluates at the
    /// document root.
    pub(crate) fn eval(&self, expr: &ast::Expr) -> Result<Value, EvalError> {
        self.eval_literal(expr)
    }

    /// Evaluate a standalone expression against this document's root
    /// scope. Bare identifiers resolve against the document's
    /// top-level fields; unknown names error with
    /// [`EvalError::UnresolvedReference`]. Designed for hosts that
    /// drive ad-hoc evaluation — `wcl repl` and embedded REPLs.
    pub fn eval_expr(&self, expr: &ast::Expr) -> Result<Value, EvalError> {
        self.eval_in_scope(expr, &Scope::root())
    }

    /// Trivial value-literal expressions: scalar number/bool/string
    /// variants, symbols, `none`. Pulled out of `eval_in` to keep the
    /// big match focused on expressions that actually involve scope or
    /// recursion.
    fn eval_value_literal(expr: &ast::Expr) -> Option<Value> {
        use ast::Expr as E;
        Some(match expr {
            E::Bool(b) => Value::Bool(*b),
            E::I8(v) => Value::I8(*v),
            E::I16(v) => Value::I16(*v),
            E::I32(v) => Value::I32(*v),
            E::I64(v) => Value::I64(*v),
            E::I128(v) => Value::I128(*v),
            E::Isize(v) => Value::Isize(*v),
            E::U8(v) => Value::U8(*v),
            E::U16(v) => Value::U16(*v),
            E::U32(v) => Value::U32(*v),
            E::U64(v) => Value::U64(*v),
            E::U128(v) => Value::U128(*v),
            E::Usize(v) => Value::Usize(*v),
            E::F32(v) => Value::F32(*v),
            E::F64(v) => Value::F64(*v),
            E::Utf8(s) => Value::Utf8(s.clone()),
            E::Ascii(s) => Value::Ascii(s.clone()),
            E::Utf16(v) => Value::Utf16(v.clone()),
            E::Utf32(v) => Value::Utf32(v.clone()),
            E::Symbol(s) => Value::Symbol(s.clone()),
            E::None => Value::None,
            _ => return None,
        })
    }

    fn eval_in<'a>(&'a self, expr: &ast::Expr, ctx: &mut EvalCtx<'a>) -> Result<Value, EvalError> {
        use ast::Expr as E;
        if let Some(v) = Self::eval_value_literal(expr) {
            return Ok(v);
        }
        Ok(match expr {
            E::InterpolatedString {
                encoding,
                parts,
                span,
            } => {
                use crate::lexer::StringEncoding as Enc;
                let mut joined = String::new();
                for part in parts {
                    match part {
                        ast::TemplatePart::Literal(s) => joined.push_str(s),
                        ast::TemplatePart::Expr(e) => {
                            let v = self.eval_in(e, ctx)?;
                            joined.push_str(&crate::collections::format_value(&v));
                        }
                    }
                }
                return match encoding {
                    Enc::Utf8 => Ok(Value::Utf8(joined)),
                    Enc::Ascii => {
                        if joined.chars().any(|c| (c as u32) >= 0x80) {
                            Err(EvalError::schema_violation(
                                crate::error::SchemaViolationKind::FieldTypeMismatch,
                                "interpolated ascii string contains a non-ASCII character",
                                *span,
                            ))
                        } else {
                            Ok(Value::Ascii(joined))
                        }
                    }
                    Enc::Utf16 => Ok(Value::Utf16(joined.encode_utf16().collect())),
                    Enc::Utf32 => Ok(Value::Utf32(joined.chars().collect())),
                };
            }
            E::Function(f) => {
                let params: Vec<FnParam> = f
                    .params
                    .iter()
                    .map(|p| FnParam::new(p.name.clone(), p.ty.clone()))
                    .collect();
                // Snapshot surrounding locals as the function value's
                // lexical capture. Document-scope identifiers (fields,
                // blocks, …) resolve at call time, so they don't need
                // snapshotting.
                let captured = ctx.locals.clone();
                Value::Function(
                    FnValue::new(params, f.return_ty.clone(), f.body.clone())
                        .with_captures(captured),
                )
            }
            E::Identifier(name, _) => {
                // Locals (let-binding scope) shadow scope-walked names.
                if let Some(v) = ctx.lookup(name) {
                    return Ok(v.clone());
                }
                let dr = self
                    .scope_lookup(&ctx.scope, name)
                    .ok_or_else(|| EvalError::unresolved_reference(name, span_of(expr)))??;
                return materialise_dataref_or_path(dr, vec![name.clone()], span_of(expr));
            }
            E::SelfKw(span) => {
                let dr = self.self_dataref(&ctx.scope);
                return materialise_dataref(dr, *span);
            }
            E::ParentKw(span) => {
                let dr = self.parent_dataref(&ctx.scope).ok_or_else(|| {
                    EvalError::unresolved_reference("parent at document root", *span)
                })?;
                return materialise_dataref(dr, *span);
            }
            E::Member {
                recv: _,
                name: _,
                span,
            } => {
                let dr = self.eval_to_dataref(expr, ctx)?;
                let segments = expr_to_path_segments(expr).unwrap_or_default();
                return materialise_dataref_or_path(dr, segments, *span);
            }
            E::Paren { inner, .. } => return self.eval_in(inner, ctx),
            E::Unary { op, operand, span } => {
                let v = self.eval_in(operand, ctx)?;
                return apply_unary(*op, v, *span);
            }
            E::Binary { op, lhs, rhs, span } => {
                // Short-circuit logical ops.
                if matches!(op, ast::BinOp::And | ast::BinOp::Or) {
                    let l = self.eval_in(lhs, ctx)?;
                    let lb = as_bool(&l, *op, *span)?;
                    let short =
                        matches!(op, ast::BinOp::And) && !lb || matches!(op, ast::BinOp::Or) && lb;
                    if short {
                        return Ok(Value::Bool(lb));
                    }
                    let r = self.eval_in(rhs, ctx)?;
                    let rb = as_bool(&r, *op, *span)?;
                    return Ok(Value::Bool(rb));
                }
                let l = self.eval_in(lhs, ctx)?;
                let r = self.eval_in(rhs, ctx)?;
                return apply_binary(*op, l, r, *span);
            }
            E::Call { callee, args, span } => {
                return self.eval_call(callee, args, *span, ctx);
            }
            E::Block { lets, tail, .. } => {
                return self.eval_block(lets, tail, ctx);
            }
            E::ListLit { elements, .. } => {
                let mut out = Vec::with_capacity(elements.len());
                for e in elements {
                    out.push(self.eval_in(e, ctx)?);
                }
                Value::List(out)
            }
            E::Record { fields, .. } => {
                // A bare record literal evaluates to an anonymous
                // `Value::Record`. When the surrounding context declares
                // a union type, the consumer (field materialisation,
                // variant args, fn-call args) coerces it to the matching
                // `Value::Variant` by shape via `coerce_value_to_type`.
                let mut map = std::collections::BTreeMap::new();
                for f in fields {
                    map.insert(f.name.clone(), self.eval_in(&f.value, ctx)?);
                }
                Value::Record {
                    ty: Vec::new(),
                    fields: map,
                }
            }
            E::If {
                cond,
                then_block,
                else_block,
                span,
            } => {
                let c = self.eval_in(cond, ctx)?;
                let b = as_bool(&c, ast::BinOp::And, *span)?;
                return self.eval_in(if b { then_block } else { else_block }, ctx);
            }
            E::IfLet {
                pattern,
                scrut,
                then_block,
                else_block,
                ..
            } => {
                return self.eval_if_let(pattern, scrut, then_block, else_block, ctx);
            }
            E::Match { scrut, arms, span } => {
                return self.eval_match(scrut, arms, *span, ctx);
            }
            E::Variant {
                type_path,
                variant,
                args,
                span,
            } => {
                return self.build_variant(type_path, variant, args, *span, ctx);
            }
            // Trivial value literals were handled by `eval_value_literal`
            // at the top of this function.
            E::Bool(_)
            | E::I8(_)
            | E::I16(_)
            | E::I32(_)
            | E::I64(_)
            | E::I128(_)
            | E::Isize(_)
            | E::U8(_)
            | E::U16(_)
            | E::U32(_)
            | E::U64(_)
            | E::U128(_)
            | E::Usize(_)
            | E::F32(_)
            | E::F64(_)
            | E::Utf8(_)
            | E::Ascii(_)
            | E::Utf16(_)
            | E::Utf32(_)
            | E::Symbol(_)
            | E::None => unreachable!("handled by eval_value_literal"),
        })
    }

    /// Evaluate every argument expression left-to-right. Pulled out
    /// because `eval_call` evaluates args in three different sub-paths
    /// (user fn, builtin, dynamic callee) — same loop body each time.
    fn eval_args<'a>(
        &'a self,
        args: &[ast::Expr],
        ctx: &mut EvalCtx<'a>,
    ) -> Result<Vec<Value>, EvalError> {
        let mut out = Vec::with_capacity(args.len());
        for arg in args {
            out.push(self.eval_in(arg, ctx)?);
        }
        Ok(out)
    }

    /// `E::Call` arm. Resolution order: when `callee` is a bare
    /// identifier, prefer a `Value::Function` in locals/scope; fall
    /// back to the builtin registry by name. Any other callee
    /// expression is evaluated and must resolve to a function value.
    fn eval_call<'a>(
        &'a self,
        callee: &ast::Expr,
        args: &[ast::Expr],
        span: Span,
        ctx: &mut EvalCtx<'a>,
    ) -> Result<Value, EvalError> {
        use ast::Expr as E;
        if let E::Identifier(name, _) = callee {
            if let Some(fv) = lookup_function(self, ctx, name) {
                let evald = self.eval_args(args, ctx)?;
                let _profile_guard =
                    self.profile_enter(crate::profile::ProfileKey::UserFn { name: name.clone() });
                return self
                    .invoke_fn_value(&fv, &evald, ctx, span)
                    .map_err(|e| call_err_at(e, name.clone(), span));
            }
            return self.eval_call_builtin(name, args, span, ctx);
        }
        let callee_val = self.eval_in(callee, ctx)?;
        let Value::Function(fv) = callee_val else {
            return Err(EvalError::non_callable(span));
        };
        let evald = self.eval_args(args, ctx)?;
        let _profile_guard = self.profile_enter(crate::profile::ProfileKey::UserFn {
            name: String::new(),
        });
        self.invoke_fn_value(&fv, &evald, ctx, span)
    }

    /// Builtin-call subpath of `eval_call`. Held distinct because it
    /// handles three control-flow primitives (`format`, `error`/`panic`,
    /// `assert`) that don't fit the generic Pure/Hof dispatch.
    fn eval_call_builtin<'a>(
        &'a self,
        name: &str,
        args: &[ast::Expr],
        span: Span,
        ctx: &mut EvalCtx<'a>,
    ) -> Result<Value, EvalError> {
        let Some(builtin) = self.env.builtin(name).cloned() else {
            return Err(EvalError::unknown_builtin(name.to_string(), span));
        };
        // `format(template, ...args)` is variadic — its registered
        // arity (0) is a sentinel that means "skip the arity check".
        if name != "format" && args.len() != builtin.arity {
            return Err(EvalError::builtin_arity(
                name.to_string(),
                builtin.arity,
                args.len(),
                span,
            ));
        }
        let evald = self.eval_args(args, ctx)?;
        let _profile_guard = self.profile_enter(crate::profile::ProfileKey::Builtin {
            name: name.to_string(),
        });
        // Special-case `error(msg)` / `panic(msg)`: raise a structured
        // UserError rather than the generic BuiltinTypeMismatch path.
        // Keeps these primitives first-class without bending the
        // builtin trait machinery.
        if (name == "error" || name == "panic") && evald.len() == 1 {
            let msg = string_arg_or_err(name, &evald[0], span, "expected utf8 string")?;
            return Err(EvalError::user_error(msg, span));
        }
        // `assert(cond, msg)` — when cond is false raise UserError;
        // when true return None.
        if name == "assert" && evald.len() == 2 {
            if matches!(&evald[0], Value::Bool(true)) {
                return Ok(Value::None);
            }
            let msg = string_arg_or_err(name, &evald[1], span, "message must be utf8")?;
            return Err(EvalError::user_error(msg, span));
        }
        // `eval(src)` — parse `src` as a WCL expression and evaluate it in
        // the *current* scope (it sees surrounding let-bindings + document
        // names). Needs `self` + `ctx`, so it's special-cased here rather
        // than going through the registered (stub) builtin body.
        if name == "eval" && evald.len() == 1 {
            let code = string_arg_or_err(name, &evald[0], span, "expected utf8 string")?;
            let expr = crate::parse_expr(&code, "<eval>").map_err(|e| {
                EvalError::builtin_type(name.to_string(), format!("eval: {e}"), span)
            })?;
            return self.eval_in(&expr, ctx);
        }
        match &builtin.kind {
            crate::builtins::BuiltinKind::Pure(body) => {
                (body)(&evald).map_err(|msg| EvalError::builtin_type(name.to_string(), msg, span))
            }
            crate::builtins::BuiltinKind::Hof(body) => {
                let mut caller = EvalCaller {
                    doc: self,
                    ctx,
                    span,
                    err: None,
                };
                let res = (body)(&mut caller, &evald);
                if let Some(e) = caller.err.take() {
                    return Err(e);
                }
                res.map_err(|msg| EvalError::builtin_type(name.to_string(), msg, span))
            }
        }
    }

    /// `E::Block { lets, tail, .. }` arm. Pushes a fresh locals frame,
    /// binds each let in order, evaluates the tail, and pops the frame.
    fn eval_block<'a>(
        &'a self,
        lets: &[ast::LetBinding],
        tail: &ast::Expr,
        ctx: &mut EvalCtx<'a>,
    ) -> Result<Value, EvalError> {
        let mut frame = ctx.push_frame();
        for binding in lets {
            let v = self.eval_in(&binding.value, &mut frame)?;
            frame.locals.push((binding.name.clone(), v));
        }
        self.eval_in(tail, &mut frame)
    }

    /// `E::IfLet` arm. On pattern match, push the bindings as a locals
    /// frame and evaluate `then_block`; otherwise evaluate `else_block`
    /// with no new bindings.
    fn eval_if_let<'a>(
        &'a self,
        pattern: &ast::Pattern,
        scrut: &ast::Expr,
        then_block: &ast::Expr,
        else_block: &ast::Expr,
        ctx: &mut EvalCtx<'a>,
    ) -> Result<Value, EvalError> {
        let v = self.eval_in(scrut, ctx)?;
        if let Some(bindings) = match_pat::match_pattern(pattern, &v) {
            let mut frame = ctx.push_frame();
            for (name, val) in bindings {
                frame.locals.push((name, val));
            }
            return self.eval_in(then_block, &mut frame);
        }
        self.eval_in(else_block, ctx)
    }

    /// `E::Match` arm. Tries each `(arm, pattern)` pair in source
    /// order; on a successful match runs the optional guard, then
    /// evaluates the arm body in a locals frame holding the bindings.
    /// The `LocalsFrame` guard makes truncate-on-early-return
    /// automatic, so unlike the previous open-coded version this path
    /// doesn't need repeated manual `truncate()` calls.
    fn eval_match<'a>(
        &'a self,
        scrut: &ast::Expr,
        arms: &[ast::MatchArm],
        span: Span,
        ctx: &mut EvalCtx<'a>,
    ) -> Result<Value, EvalError> {
        let v = self.eval_in(scrut, ctx)?;
        for arm in arms {
            for pat in &arm.patterns {
                let Some(bindings) = match_pat::match_pattern(pat, &v) else {
                    continue;
                };
                let mut frame = ctx.push_frame();
                for (name, val) in bindings {
                    frame.locals.push((name, val));
                }
                if let Some(guard) = &arm.guard {
                    match self.eval_in(guard, &mut frame)? {
                        Value::Bool(true) => {}
                        Value::Bool(false) => continue,
                        other => {
                            return Err(EvalError::guard_not_bool(
                                other.type_name(),
                                span_of(guard),
                            ));
                        }
                    }
                }
                return self.eval_in(&arm.body, &mut frame);
            }
        }
        Err(EvalError::match_no_arm(span))
    }

    /// Apply a `Value::Function` to the supplied argument values. Pushes
    /// the function's parameters onto `ctx.locals`, evaluates the body in
    /// the caller's context, and pops the frame regardless of outcome.
    ///
    /// Closure semantics: the body sees its own parameters plus whatever
    /// the caller's `ctx` has on its `locals` stack and `scope` chain.
    /// There is no capture of the *definition-site* scope in this pass,
    /// so a function value passed across blocks observes the *call*
    /// site's lexical environment, not its origin.
    pub(crate) fn invoke_fn_value<'a>(
        &'a self,
        f: &FnValue,
        args: &[Value],
        ctx: &mut EvalCtx<'a>,
        span: Span,
    ) -> Result<Value, EvalError> {
        if args.len() != f.params().len() {
            return Err(EvalError::call_arity(f.params().len(), args.len(), span));
        }
        if ctx.call_depth >= MAX_CALL_DEPTH {
            return Err(EvalError::call_depth_exceeded(MAX_CALL_DEPTH, span));
        }
        let mut frame = ctx.push_frame();
        // Lexical captures first — later pushes (params, nested let
        // bindings) shadow them on right-to-left lookup.
        for (name, value) in &f.captured {
            frame.locals.push((name.clone(), value.clone()));
        }
        for (param, value) in f.params().iter().zip(args.iter()) {
            // Coerce a bare-record argument to the parameter's declared
            // union variant by shape; all other args pass through.
            let value = super::variant_dispatch::coerce_value_to_type(
                self,
                value.clone(),
                param.ty(),
                span,
            )?;
            frame.locals.push((param.name().to_string(), value));
        }
        frame.call_depth += 1;
        let result = self.eval_in(&f.body, &mut frame);
        frame.call_depth -= 1;
        result
    }

    /// Construct a [`Value::Variant`] from a parsed `Type::Variant`
    /// expression. Resolves the union by `type_path` (with the
    /// document's `file_ns` as a candidate prefix), validates the args
    /// shape against the declared variant body, evaluates each arg,
    /// and stashes them into the appropriate `VariantPayload`. Field
    /// *type* checking is left to schema validation.
    pub(crate) fn build_variant<'a>(
        &'a self,
        type_path: &[String],
        variant: &str,
        args: &ast::VariantArgs,
        span: Span,
        ctx: &mut EvalCtx<'a>,
    ) -> Result<Value, EvalError> {
        // Resolve the union — try the path as-is, then with the
        // document's namespace prefixed (same dance as `field`/`union_decl`).
        let candidates: Vec<String> = if self.file_ns.is_empty() {
            vec![type_path.join(".")]
        } else {
            let bare = type_path.join(".");
            let qualified = format!("{}.{}", self.file_ns.join("."), bare);
            vec![qualified, bare]
        };
        let mut found_union: Option<&ast::UnionDecl> = None;
        for fqn in &candidates {
            if let Some(u) = self.union_decl(fqn) {
                found_union = Some(u.ast);
                break;
            }
        }
        let Some(union_ast) = found_union else {
            return Err(EvalError::unknown_union(type_path.join("."), span));
        };
        let union_fqn = union_ast.name.clone();
        let effective = self.effective_variants_of(union_ast)?;
        let Some(variant_decl) = effective.iter().copied().find(|v| v.name == variant) else {
            return Err(EvalError::unknown_variant(
                union_fqn.join("."),
                variant.to_string(),
                span,
            ));
        };
        let payload = match (&variant_decl.body, args) {
            (ast::VariantBody::Unit, ast::VariantArgs::Unit) => crate::value::VariantPayload::Unit,
            (ast::VariantBody::TypeRef { .. }, ast::VariantArgs::Positional(e)) => {
                let v = self.eval_in(e, ctx)?;
                crate::value::VariantPayload::Positional(Box::new(v))
            }
            (ast::VariantBody::InterfaceRef { iface, .. }, ast::VariantArgs::Positional(e)) => {
                let v = self.eval_in(e, ctx)?;
                self.check_value_implements_iface(&v, iface, span)?;
                crate::value::VariantPayload::Positional(Box::new(v))
            }
            (ast::VariantBody::Record(decl_fields), ast::VariantArgs::Record(named_args)) => {
                let mut map = std::collections::BTreeMap::new();
                // Each declared field must be supplied exactly once.
                for decl_field in decl_fields {
                    let Some(arg) = named_args.iter().find(|na| na.name == decl_field.name) else {
                        return Err(EvalError::variant_shape_mismatch(
                            format!("field '{}'", decl_field.name),
                            "missing",
                            span,
                        ));
                    };
                    let v = self.eval_in(&arg.value, ctx)?;
                    // Coerce a bare record nested in a union-typed
                    // variant field to its matching variant by shape.
                    let v = super::variant_dispatch::coerce_value_to_type(
                        self,
                        v,
                        &decl_field.ty,
                        span,
                    )?;
                    map.insert(decl_field.name.clone(), v);
                }
                // Reject extras — keeps the runtime value strictly
                // shaped to the declared variant body.
                for arg in named_args {
                    if !decl_fields.iter().any(|f| f.name == arg.name) {
                        return Err(EvalError::variant_shape_mismatch(
                            format!("declared fields of {}::{}", union_fqn.join("."), variant),
                            format!("unexpected field '{}'", arg.name),
                            span,
                        ));
                    }
                }
                crate::value::VariantPayload::Record(map)
            }
            (expected_body, given) => {
                let expected = match expected_body {
                    ast::VariantBody::Unit => "no arguments",
                    ast::VariantBody::TypeRef { .. } => "positional argument",
                    ast::VariantBody::InterfaceRef { .. } => "positional argument (interface ref)",
                    ast::VariantBody::Record(_) => "record arguments",
                };
                let got = match given {
                    ast::VariantArgs::Unit => "no arguments",
                    ast::VariantArgs::Positional(_) => "positional argument",
                    ast::VariantArgs::Record(_) => "record arguments",
                };
                return Err(EvalError::variant_shape_mismatch(expected, got, span));
            }
        };
        Ok(Value::Variant {
            union: union_fqn,
            variant: variant.to_string(),
            payload,
        })
    }

    /// Effective variants of a union: parent unions' variants first
    /// (depth-first across the `extends` chain), then the union's own
    /// variants, deduplicating by name (parent first wins; collisions
    /// are caught separately by validation). Detects cycles and
    /// returns `EvalError::UnionCycle`.
    pub(crate) fn effective_variants_of<'a>(
        &'a self,
        union_ast: &'a ast::UnionDecl,
    ) -> Result<Vec<&'a ast::UnionVariant>, EvalError> {
        let mut out: Vec<&ast::UnionVariant> = Vec::new();
        let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut visiting: std::collections::HashSet<String> = std::collections::HashSet::new();
        self.collect_effective_variants(union_ast, &mut out, &mut seen_names, &mut visiting)?;
        Ok(out)
    }

    /// Resolve a `union extends` parent reference. Prefers the
    /// file-namespace-qualified form (matching how the parent was
    /// indexed) and falls back to the bare path. Returns `None` if
    /// neither form is a registered union.
    fn resolve_parent_union(&self, parent_path: &[String]) -> Option<&ast::UnionDecl> {
        let bare = parent_path.join(".");
        if !self.file_ns.is_empty() {
            let qualified = format!("{}.{bare}", self.file_ns.join("."));
            if let Some(p) = self.union_decl(&qualified) {
                return Some(p.ast);
            }
        }
        self.union_decl(&bare).map(|p| p.ast)
    }

    fn collect_effective_variants<'a>(
        &'a self,
        u: &'a ast::UnionDecl,
        out: &mut Vec<&'a ast::UnionVariant>,
        seen: &mut std::collections::HashSet<String>,
        visiting: &mut std::collections::HashSet<String>,
    ) -> Result<(), EvalError> {
        let key = u.name.join(".");
        if visiting.contains(&key) {
            return Err(EvalError::union_cycle(key, u.span));
        }
        visiting.insert(key.clone());
        // Parents first (depth-first), then own variants.
        for parent_path in &u.extends {
            let p = self
                .resolve_parent_union(parent_path)
                .ok_or_else(|| EvalError::unknown_union(parent_path.join("."), u.span))?;
            self.collect_effective_variants(p, out, seen, visiting)?;
        }
        for v in &u.variants {
            if seen.insert(v.name.clone()) {
                out.push(v);
            }
            // Collisions silently drop the later definition — declaration
            // validation reports them as DuplicateVariant errors.
        }
        visiting.remove(&key);
        Ok(())
    }

    /// Check that `value`'s effective fields cover the interface's
    /// declared fields with matching types — for `VariantBody::InterfaceRef`.
    ///
    /// Scope: values that carry a named field map — a variant with a
    /// record payload, or a bare `Value::Record` — are structurally
    /// introspected. Other value shapes (closures, lists, tensors,
    /// scalars) pass through permissively, since value→type
    /// introspection for them would require runtime type tags the
    /// language doesn't currently carry.
    pub(crate) fn check_value_implements_iface(
        &self,
        value: &Value,
        iface_path: &[String],
        span: Span,
    ) -> Result<(), EvalError> {
        // Resolve the interface declaration. Try with namespace prefix
        // first (matching `union_decl`/`field` lookup conventions).
        let candidates: Vec<String> = if self.file_ns.is_empty() {
            vec![iface_path.join(".")]
        } else {
            vec![
                format!("{}.{}", self.file_ns.join("."), iface_path.join(".")),
                iface_path.join("."),
            ]
        };
        let mut iface_decl: Option<&ast::InterfaceDecl> = None;
        for fqn in &candidates {
            if let Some(i) = self.interface(fqn) {
                iface_decl = Some(i.ast);
                break;
            }
        }
        let Some(iface) = iface_decl else {
            return Err(EvalError::unknown_union(iface_path.join("."), span));
        };
        // Structurally introspect values that carry a named field map:
        // a variant with a record payload, or a bare record value. Other
        // shapes (closures, lists, tensors, scalars, and non-record
        // variant payloads) carry no field map, so they pass through
        // permissively until the language tags them at runtime.
        let fields = match value {
            Value::Variant {
                payload: crate::value::VariantPayload::Record(map),
                ..
            } => map,
            Value::Record { fields, .. } => fields,
            _ => return Ok(()),
        };
        for f in &iface.fields {
            let Some(v) = fields.get(&f.name) else {
                return Err(EvalError::variant_shape_mismatch(
                    format!("interface field '{}'", f.name),
                    "missing on value",
                    span,
                ));
            };
            let expected = &f.ty;
            if !value_matches_type_ref(v, expected) {
                return Err(EvalError::variant_shape_mismatch(
                    format!("interface field '{}': {expected:?}", f.name),
                    format!("value field is {}", v.type_name()),
                    span,
                ));
            }
        }
        Ok(())
    }

    /// Walk a path expression and return the navigator for its
    /// resolved target.
    pub(crate) fn eval_to_dataref<'a>(
        &'a self,
        expr: &ast::Expr,
        ctx: &EvalCtx<'a>,
    ) -> Result<crate::data::DataRef<'a>, EvalError> {
        use ast::Expr as E;
        match expr {
            E::Identifier(name, _) => {
                // Locals (let-bindings, function parameters) shadow
                // scope-walked names. Wrap the value as a DataRef so
                // downstream member access can project record/variant
                // fields without re-evaluating the receiver.
                if let Some(v) = ctx.lookup(name) {
                    return Ok(crate::data::DataRef::from_variant_value(v.clone()));
                }
                // Outer `?` turns a missing name into "unresolved
                // reference"; the inner `Result` (a let's eval/cycle
                // error) propagates as-is.
                self.scope_lookup(&ctx.scope, name)
                    .ok_or_else(|| EvalError::unresolved_reference(name, span_of(expr)))?
            }
            E::SelfKw(_) => Ok(self.self_dataref(&ctx.scope)),
            E::ParentKw(span) => self
                .parent_dataref(&ctx.scope)
                .ok_or_else(|| EvalError::unresolved_reference("parent at document root", *span)),
            E::Member { recv, name, span } => {
                let recv_dr = self.eval_to_dataref(recv, ctx)?;
                recv_dr.child(name).ok_or_else(|| {
                    let full_path = format_member_path(expr);
                    EvalError::unresolved_reference(full_path, *span)
                })
            }
            E::Paren { inner, .. } => self.eval_to_dataref(inner, ctx),
            other => Err(EvalError::not_a_reference(
                describe_expr(other),
                span_of(other),
            )),
        }
    }

    /// Rebuild a [`Block`] view for the frame at index `i` of `scope`'s
    /// frame chain. The new block's own scope is the slice of frames
    /// strictly *before* `i`, which is what every scope-walking caller
    /// needs (looking up a name from frame `i` should see ancestors but
    /// not siblings).
    fn frame_as_block<'a>(&'a self, scope: &Scope<'a>, i: usize) -> Block<'a> {
        let frames = scope.frames();
        Block {
            ast: frames[i].ast,
            cells: frames[i].cells,
            doc: self,
            kind_override: frames[i].kind_override,
            scope: Scope::from_frames(&frames[..i]),
        }
    }

    /// Resolve a single name against the scope chain (innermost
    /// frame first) and fall through to the document root.
    ///
    /// At each frame a `let` binding is tried before the frame's
    /// fields/blocks, so an inner `let` shadows a same-named field and
    /// inner scopes win over outer ones. Returns `None` when the name
    /// is unresolved; `Some(Err(..))` when a matched `let`'s value
    /// fails to evaluate (e.g. a cycle), so callers surface the real
    /// error instead of "unresolved reference".
    pub(crate) fn scope_lookup<'a>(
        &'a self,
        scope: &Scope<'a>,
        name: &str,
    ) -> Option<Result<crate::data::DataRef<'a>, EvalError>> {
        for i in (0..scope.frames().len()).rev() {
            // Renderer-injected bindings (a `wdoc_component` slot or a
            // `wdoc_repeater` loop variable) resolve first at this frame,
            // shadowing the frame's own fields/blocks like an inner `let`.
            if let Some(bindings) = &scope.frames()[i].bindings
                && let Some((_, v)) = bindings.iter().find(|(n, _)| n == name)
            {
                return Some(Ok(crate::data::DataRef::from_variant_value(v.clone())));
            }
            let block = self.frame_as_block(scope, i);
            if let Some(letv) = block.find_let(name) {
                return Some(letv.value().map(crate::data::DataRef::from_variant_value));
            }
            if let Some(child) = crate::data::DataRef::from_block(block).child(name) {
                return Some(Ok(child));
            }
        }
        if let Some(letv) = self.root_let(name) {
            return Some(letv.value().map(crate::data::DataRef::from_variant_value));
        }
        self.resolve_root(name).map(Ok)
    }

    fn self_dataref<'a>(&'a self, scope: &Scope<'a>) -> crate::data::DataRef<'a> {
        match scope.frames().len().checked_sub(1) {
            Some(last_idx) => {
                crate::data::DataRef::from_block(self.frame_as_block(scope, last_idx))
            }
            None => crate::data::DataRef::from_document(self),
        }
    }

    fn parent_dataref<'a>(&'a self, scope: &Scope<'a>) -> Option<crate::data::DataRef<'a>> {
        match scope.frames().len() {
            0 => None,
            1 => Some(crate::data::DataRef::from_document(self)),
            n => Some(crate::data::DataRef::from_block(
                self.frame_as_block(scope, n - 2),
            )),
        }
    }
}
