//! The declared type of a value, for diagnostics that name it.
//!
//! A record literal carries no type of its own: `{ name: "a" }` is the
//! same value in a `Row` slot as anywhere else, so its display, its JSON,
//! its equality and the slots it fills do not depend on where it was
//! written. A diagnostic about it still wants the type the author
//! declared — `record of type 'Row' has no field 'nope'` says more than
//! `record has no field 'nope'`.
//!
//! This module recovers that type from the expression that produced the
//! value instead of from the value: a field's schema declaration, a
//! function's parameter or return type, the element type of the list
//! `at` / `head` / `find` read from, the field type of the record a
//! member was read out of, a union variant's payload type. Nothing here
//! changes a value or decides whether a slot accepts one; it is consulted
//! only to word an error, and to carry the type on a local binding so a
//! later error can.

use crate::ast::{self, Pattern, TypeRef, VariantPatArgs};
use crate::value::{FnValue, Value};

use super::eval::{EvalCtx, lookup_function};
use super::signature::{BuiltinCallback, is_value_class};
use super::{DataKind, DataRef, Document};

/// A type as declared, with the namespace it was written in so a named
/// type resolves the way its declaration does.
#[derive(Debug, Clone)]
pub(crate) struct Declared {
    /// The type as written.
    ty: TypeRef,
    /// The namespace the type was written in.
    ns: Vec<String>,
}

impl Declared {
    /// `ty`, written in namespace `ns`.
    pub(crate) fn new(ty: TypeRef, ns: &[String]) -> Self {
        Self {
            ty,
            ns: ns.to_vec(),
        }
    }
}

/// Builtins whose result is an element of their first argument.
const ELEMENT_OF_FIRST: &[&str] = &["at", "head", "find", "max_by", "min_by"];

/// Builtins whose result is a list of the same elements as their first
/// argument: a reordering, a filter or a sub-range.
const SAME_LIST_AS_FIRST: &[&str] = &[
    "drop",
    "filter",
    "reverse",
    "slice",
    "sort",
    "sort_by",
    "sort_connected",
    "tail",
    "take",
    "unique",
];

/// Whether a declared type could name something in `value` that has
/// none of its own: a record literal, directly or as a list element.
/// Binding sites check this before working out a declared type, so a
/// local holding a number or a block costs nothing. A list is judged by
/// its first element that is not `none`, so a long list costs no more
/// than a short one.
pub(crate) fn wants_declared(value: &Value) -> bool {
    match value {
        Value::Record { ty, .. } => ty.is_empty(),
        Value::List(items) => items
            .iter()
            .find(|v| !matches!(v, Value::None))
            .is_some_and(wants_declared),
        _ => false,
    }
}

/// Whether `value` is a record literal: a record that carries no type of
/// its own, which only a declaration can name.
pub(crate) fn is_record_literal(value: &Value) -> bool {
    matches!(value, Value::Record { ty, .. } if ty.is_empty())
}

/// `got` — a refusal's description of a value, as it follows "got" or
/// "value is" — with a record literal's leading `record` naming the type
/// `name` works out (`record of type 'Row' without field 'width'`).
/// `literal` says whether the value was a record literal
/// ([`is_record_literal`]); for any other value, a tagged record among
/// them (whose description already names its type), and when `name`
/// finds none, `got` is left as it is. `name` runs only for a record
/// literal, so refusing anything else costs nothing more.
pub(crate) fn name_declared_record(
    got: String,
    literal: bool,
    name: impl FnOnce() -> Option<String>,
) -> String {
    if !literal {
        return got;
    }
    let Some(rest) = got
        .strip_prefix("record")
        .filter(|rest| rest.is_empty() || rest.starts_with([' ', ',']))
    else {
        return got;
    };
    match name() {
        Some(ty) => format!("record of type '{ty}'{rest}"),
        None => got,
    }
}

/// Builtins that hand their callback each element of their first
/// argument as the callback's only argument.
const CALLBACK_GETS_ELEMENT: &[&str] = &[
    "all", "any", "filter", "find", "group_by", "map", "max_by", "min_by", "sort_by",
];

/// How a refusal introduces the element of a list it describes next
/// (`a list whose element [0] is record`): the text before the index,
/// and the text between the index and the element's description.
const LIST_ELEMENT: (&str, &str) = ("a list whose element [", "] is ");

impl Document {
    /// `got` — a refusal's description of `value`, as it follows "got" —
    /// with the record literal it describes named by its declared type:
    /// the value itself, or the element a refused list names
    /// (`a list whose element [0] is record of type 'Row'`), however
    /// deeply lists nest. `declared` gives the declared type of the
    /// value at a path of list indices into `value` (empty for `value`
    /// itself), and runs only when the described value is a record
    /// literal; otherwise, and when it finds no name, `got` is left as
    /// it is.
    pub(crate) fn name_declared_in(
        &self,
        got: String,
        value: &Value,
        declared: impl FnOnce(&[usize]) -> Option<Declared>,
    ) -> String {
        let mut path = Vec::new();
        let mut at = value;
        let mut rest = got.as_str();
        while let Some(after) = rest.strip_prefix(LIST_ELEMENT.0) {
            let Some((index, tail)) = after.split_once(LIST_ELEMENT.1) else {
                return got;
            };
            let (Ok(i), Value::List(items)) = (index.parse::<usize>(), at) else {
                return got;
            };
            let Some(item) = items.get(i) else {
                return got;
            };
            path.push(i);
            at = item;
            rest = tail;
        }
        if !is_record_literal(at) {
            return got;
        }
        let prefix = &got[..got.len() - rest.len()];
        let named = name_declared_record(rest.to_string(), true, || {
            Self::declared_record_name(&declared(&path)?)
        });
        format!("{prefix}{named}")
    }

    /// The declared type of the value at `path` — list indices, empty
    /// for the value itself — inside the value `expr` produces: an
    /// element of a list literal is its own expression's, anything else
    /// the element type of what `expr` was declared to produce. See
    /// [`Self::declared_type_of`].
    pub(crate) fn declared_at<'a>(
        &'a self,
        expr: &ast::Expr,
        path: &[usize],
        ctx: &EvalCtx<'a>,
    ) -> Option<Declared> {
        let Some((&i, rest)) = path.split_first() else {
            return self.declared_type_of(expr, ctx);
        };
        match expr {
            ast::Expr::Paren { inner, .. } => self.declared_at(inner, path, ctx),
            ast::Expr::ListLit { elements, .. } => self.declared_at(elements.get(i)?, rest, ctx),
            _ => self.declared_elements(self.declared_type_of(expr, ctx)?, path.len()),
        }
    }

    /// The type `depth` levels of list element inside `declared`.
    fn declared_elements(&self, declared: Declared, depth: usize) -> Option<Declared> {
        (0..depth).try_fold(declared, |d, _| self.declared_element(&d))
    }

    /// The declared type of the value at `path` (as for
    /// [`Self::declared_at`]) inside argument `index` (from 0) that a
    /// builtin hands its callback `f`. For the builtins that pass each
    /// element of their first argument, that element's declared type:
    /// the element type the argument declares, or for a list literal the
    /// element's own expression's. For `fold`, its element likewise as
    /// the second argument; the accumulator is the initial value on the
    /// first call and what `f` declared it returns after that. `None`
    /// for anything else.
    pub(crate) fn callback_arg_declared<'a>(
        &'a self,
        callback: &BuiltinCallback<'_>,
        f: &FnValue,
        index: usize,
        path: &[usize],
        ctx: &EvalCtx<'a>,
    ) -> Option<Declared> {
        // Each of these builtins calls its callback once per element, in
        // order, so the call's number is the element's index: a list
        // literal's element is read from its own expression.
        let element = |list: &ast::Expr| {
            let mut at = Vec::with_capacity(path.len() + 1);
            at.push(callback.call);
            at.extend_from_slice(path);
            self.declared_at(list, &at, ctx)
        };
        match (callback.builtin, index) {
            ("fold", 0) if callback.call == 0 => self.declared_at(callback.args.get(1)?, path, ctx),
            ("fold", 0) => {
                let returned = Declared::new(f.return_ty().clone(), self.ctx_ns(ctx));
                self.declared_elements(returned, path.len())
            }
            ("fold", 1) => element(callback.args.first()?),
            (builtin, 0) if CALLBACK_GETS_ELEMENT.contains(&builtin) => {
                element(callback.args.first()?)
            }
            _ => None,
        }
    }

    /// The namespace expressions in `ctx` are written in.
    pub(super) fn ctx_ns<'a>(&'a self, ctx: &EvalCtx<'a>) -> &'a [String] {
        ctx.scope
            .frames()
            .last()
            .map(|f| f.file_ns)
            .unwrap_or(&self.file_ns)
    }

    /// The name a diagnostic gives a record literal declared `declared`:
    /// the named type as written (`Row`), or `None` when the declaration
    /// is not a named type, or is one of a signature's value classes
    /// (`any`, `record`, …), which name no type of their own.
    pub(crate) fn declared_record_name(declared: &Declared) -> Option<String> {
        match &declared.ty {
            TypeRef::Named { path, args } if !is_value_class(path, args) => Some(path.join(".")),
            _ => None,
        }
    }

    /// The name of the type `expr` was declared to produce, for a
    /// diagnostic about the record literal it evaluated to: see
    /// [`Self::declared_type_of`] and [`Self::declared_record_name`].
    pub(crate) fn declared_record_name_of<'a>(
        &'a self,
        expr: &ast::Expr,
        ctx: &EvalCtx<'a>,
    ) -> Option<String> {
        Self::declared_record_name(&self.declared_type_of(expr, ctx)?)
    }

    /// The type a navigator's target was declared with: a field's schema
    /// declaration. `None` for anything else.
    fn dataref_declared(dr: &DataRef<'_>) -> Option<Declared> {
        let DataKind::Field(f) = dr.inner() else {
            return None;
        };
        let field = f.declared_field()?;
        Some(Declared::new(field.type_ref().clone(), field.file_ns))
    }

    /// The element type of a list declared `declared`.
    fn declared_element(&self, declared: &Declared) -> Option<Declared> {
        match self.resolve_alias_in(&declared.ty, &declared.ns) {
            TypeRef::List(inner) => Some(Declared::new(*inner, &declared.ns)),
            _ => None,
        }
    }

    /// The type field `name` is declared with in the record type
    /// `declared`, inherited fields included.
    fn declared_field_of(&self, declared: &Declared, name: &str) -> Option<Declared> {
        let TypeRef::Named { path, .. } = self.resolve_alias_in(&declared.ty, &declared.ns) else {
            return None;
        };
        let fqn = self.resolve_path_in(&path, &declared.ns).unwrap_or(path);
        let decl = self.type_decl(&fqn.join("."))?;
        let field = decl.effective_field(name)?;
        Some(Declared::new(field.type_ref().clone(), field.file_ns))
    }

    /// The type `expr` was declared to produce, when its shape says so
    /// without evaluating anything new: a path to a declared field (or a
    /// field of a declared record), a local carrying a declared type, a
    /// call whose callee declares its return type or a list builtin that
    /// hands back an element of a declared list, and a bracketed, block
    /// or `if` expression whose result is one of those.
    ///
    /// Used to word diagnostics; `None` whenever the type is not known,
    /// and every failure along the way is `None` too.
    pub(crate) fn declared_type_of<'a>(
        &'a self,
        expr: &ast::Expr,
        ctx: &EvalCtx<'a>,
    ) -> Option<Declared> {
        use ast::Expr as E;
        match expr {
            E::Paren { inner, .. } => self.declared_type_of(inner, ctx),
            E::Identifier(name, _) if ctx.is_local(name) => ctx.lookup_declared(name).cloned(),
            E::Identifier(..) | E::SelfKw(_) | E::ParentKw(_) => {
                Self::dataref_declared(&self.eval_to_dataref(expr, ctx).ok()?)
            }
            E::Member { recv, name, .. } => {
                if let Some(d) = self
                    .eval_to_dataref(expr, ctx)
                    .ok()
                    .and_then(|dr| Self::dataref_declared(&dr))
                {
                    return Some(d);
                }
                self.declared_field_of(&self.declared_type_of(recv, ctx)?, name)
            }
            E::Call { callee, args, .. } => self.declared_call_result(callee, args, ctx),
            E::Block { lets, tail, .. } => match tail.as_ref() {
                // The block hands back one of its own lets: that let's
                // value. A let reading an earlier let is not followed.
                E::Identifier(name, _) if lets.iter().any(|l| &l.name == name) => {
                    let binding = lets.iter().rev().find(|l| &l.name == name)?;
                    self.declared_type_of(&binding.value, ctx)
                }
                tail if lets.is_empty() => self.declared_type_of(tail, ctx),
                _ => None,
            },
            // Both branches must agree; an else-less `if` may be `none`,
            // which a record diagnostic never meets.
            E::If {
                then_block,
                else_block,
                ..
            } => {
                let then = self.declared_type_of(then_block, ctx)?;
                match else_block {
                    Some(other) => {
                        let other = self.declared_type_of(other, ctx)?;
                        (other.ty == then.ty).then_some(then)
                    }
                    None => Some(then),
                }
            }
            _ => None,
        }
    }

    /// The declared type of a call's result: the return type a named or
    /// literal function declares, or the element (or list) of the first
    /// argument for the list builtins that hand one back.
    fn declared_call_result<'a>(
        &'a self,
        callee: &ast::Expr,
        args: &[ast::Expr],
        ctx: &EvalCtx<'a>,
    ) -> Option<Declared> {
        let ns = self.ctx_ns(ctx);
        match callee {
            ast::Expr::Function(f) => Some(Declared::new(f.return_ty.clone(), ns)),
            ast::Expr::Paren { inner, .. } => self.declared_call_result(inner, args, ctx),
            ast::Expr::Identifier(name, _) => {
                if let Some(f) = lookup_function(self, ctx, name) {
                    return Some(Declared::new(f.return_ty().clone(), ns));
                }
                let first = args.first()?;
                if ELEMENT_OF_FIRST.contains(&name.as_str()) {
                    self.declared_element(&self.declared_type_of(first, ctx)?)
                } else if SAME_LIST_AS_FIRST.contains(&name.as_str()) {
                    self.declared_type_of(first, ctx)
                } else if name == "flatten" {
                    let outer = self.declared_type_of(first, ctx)?;
                    self.declared_element(&outer)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// The declared type of the value `pat` binds to `name` when matched
    /// against `value`, whose own declared type `scrutinee` gives (it is
    /// asked for only when the name binds the whole value). A name bound
    /// inside a variant takes the variant's declared payload or field
    /// type.
    pub(crate) fn pattern_binding_declared(
        &self,
        pat: &Pattern,
        value: &Value,
        name: &str,
        scrutinee: &mut dyn FnMut() -> Option<Declared>,
    ) -> Option<Declared> {
        match pat {
            Pattern::Binding { name: n, .. } if n == name => scrutinee(),
            Pattern::At { name: n, .. } if n == name => scrutinee(),
            Pattern::At { inner, .. } => {
                self.pattern_binding_declared(inner, value, name, scrutinee)
            }
            Pattern::Variant { args, .. } => {
                let Value::Variant {
                    union,
                    variant,
                    payload,
                } = value
                else {
                    return None;
                };
                let decl = self.union_decl(&union.join("."))?;
                let var = decl.ast.variants.iter().find(|v| &v.name == variant)?;
                match (args, &var.body, payload) {
                    (
                        VariantPatArgs::Positional(inner),
                        ast::VariantBody::TypeRef { ty, .. },
                        crate::value::VariantPayload::Positional(v),
                    ) => {
                        let declared = Declared::new(ty.clone(), decl.file_ns);
                        self.pattern_binding_declared(inner, v, name, &mut || {
                            Some(declared.clone())
                        })
                    }
                    (
                        VariantPatArgs::Record { fields, .. },
                        ast::VariantBody::Record {
                            fields: declared, ..
                        },
                        crate::value::VariantPayload::Record(map),
                    ) => fields.iter().find_map(|(field, sub)| {
                        let v = map.get(field)?;
                        let ty = declared.iter().find(|f| &f.name == field)?.ty.clone();
                        let d = Declared::new(ty, decl.file_ns);
                        self.pattern_binding_declared(sub, v, name, &mut || Some(d.clone()))
                    }),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}
