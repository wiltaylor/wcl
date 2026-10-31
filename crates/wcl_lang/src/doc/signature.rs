//! Function signatures, enforced at call time.
//!
//! A parameter accepts an argument exactly when a field declared with the
//! parameter's type would accept it, and converts it exactly as that
//! field would: the rule lives in [`types::slot`](super::types) and is
//! shared, not restated. The return value is held to the return type the
//! same way.
//!
//! A signature may name three things a field slot cannot, and those are
//! settled here, without conversion:
//!
//! - the builtin notation's value classes — `any` (every value, `none`
//!   included), `number`, `integer` and `record` (a record, or a variant
//!   whose payload is one);
//! - an interface, written bare (`fn(s: Shape)`), which the value must
//!   satisfy structurally;
//! - a function type, which checks that the argument is a function of
//!   the same arity — exactly as far as a `fn`-typed field checks it,
//!   plus the parameter count.
//!
//! Resolving what a named type is costs a namespace walk, and closure-
//! heavy documents call functions in tight loops, so the verdict for each
//! written path is memoised on the document ([`NamedSlot`]).

use crate::ast::{self, Span, TypeRef};
use crate::diagnostics::EvalError;
use crate::value::{FnValue, Value};

use super::Document;
use super::eval::{IfaceMisfit, iface_misfit};
use super::types::{
    Refusal, admit_to_slot, path_matches_suffix, value_kind, value_matches_type_ref,
};

/// What a named type in a signature turns out to be, as far as checking
/// a value against it goes. Memoised per written path.
#[derive(Debug, Clone)]
pub(super) enum NamedSlot {
    /// An interface, by fully-qualified name: the value must satisfy it.
    Interface(String),
    /// A union. `name` is the declaration the field rule's union check
    /// finds under the path as written, whose variants alone it accepts;
    /// `None` when that lookup misses and the rule falls back to matching
    /// a variant's union by path suffix.
    Union {
        /// The union's declared name, when found under the written path.
        name: Option<Vec<String>>,
    },
    /// A type the field rule judges by identity alone — no alias to
    /// peel, no union to infer, no symbol set, no constraints on an
    /// alias chain. The check reduces to [`value_matches_type_ref`].
    Plain,
    /// Anything else: run the whole field rule.
    Full,
}

/// One of the builtin signature notation's value classes.
#[derive(Debug, Clone, Copy)]
enum ValueClass {
    /// `any` — every value, `none` included.
    Any,
    /// `number` — any numeric value.
    Number,
    /// `integer` — any integer value.
    Integer,
    /// `record` — a record, or a variant whose payload is one.
    Record,
}

impl ValueClass {
    /// The class a signature's named type spells, if it spells one.
    fn of(path: &[String], args: &[TypeRef]) -> Option<Self> {
        if !args.is_empty() {
            return None;
        }
        match path {
            [word] => match word.as_str() {
                "any" => Some(Self::Any),
                "number" => Some(Self::Number),
                "integer" => Some(Self::Integer),
                "record" => Some(Self::Record),
                _ => None,
            },
            _ => None,
        }
    }

    /// Whether `value` belongs to the class.
    fn admits(self, value: &Value) -> bool {
        match self {
            Self::Any => true,
            Self::Number => value.is_numeric(),
            Self::Integer => matches!(
                value,
                Value::I8(_)
                    | Value::I16(_)
                    | Value::I32(_)
                    | Value::I64(_)
                    | Value::I128(_)
                    | Value::Isize(_)
                    | Value::U8(_)
                    | Value::U16(_)
                    | Value::U32(_)
                    | Value::U64(_)
                    | Value::U128(_)
                    | Value::Usize(_)
            ),
            Self::Record => matches!(
                value,
                Value::Record { .. }
                    | Value::Variant {
                        payload: crate::value::VariantPayload::Record(_),
                        ..
                    }
            ),
        }
    }
}

/// A function value's own type, as a signature would write it.
fn fn_type_of(f: &FnValue) -> String {
    TypeRef::Function {
        params: f.params().iter().map(|p| p.ty().clone()).collect(),
        return_ty: Box::new(f.return_ty().clone()),
    }
    .to_string()
}

impl Document {
    /// What a signature slot declared `ty` makes of `value`:
    /// `Ok(Ok(converted))` when it accepts the value, `Ok(Err(got))` with
    /// a description of what it was handed when it refuses, and `Err`
    /// when converting the value fails outright (a unit the type does not
    /// declare). `span` is where a conversion error points.
    pub(super) fn admit_signature(
        &self,
        value: Value,
        ty: &TypeRef,
        span: Span,
    ) -> Result<Result<Value, String>, EvalError> {
        if self.is_signature_only(ty) {
            return Ok(match self.signature_misfit(&value, ty) {
                None => Ok(value),
                Some(got) => Err(got),
            });
        }
        if self.accepts_unconverted(&value, ty) {
            return Ok(Ok(value));
        }
        if let TypeRef::Named { path, .. } = ty
            && !matches!(value, Value::PendingUnit { .. })
            && matches!(self.named_slot(path), NamedSlot::Plain)
        {
            return Ok(if value_matches_type_ref(self, &value, ty) {
                Ok(value)
            } else {
                Err(Refusal::Type.describe_got(self, &value, ty))
            });
        }
        Ok(admit_to_slot(self, value, ty, &self.file_ns, span)?
            .map_err(|(value, resolved, refusal)| refusal.describe_got(self, &value, &resolved)))
    }

    /// Whether the field rule would accept `value` for `ty` and convert
    /// nothing, decided without running it. A shortcut for the shapes
    /// closure-heavy documents pass most — a union variant, a list of
    /// variants or block records — which the full rule would rebuild
    /// element by element only to hand back unchanged. `false` means
    /// "run the full rule", not "refused".
    fn accepts_unconverted(&self, value: &Value, ty: &TypeRef) -> bool {
        match (value, ty) {
            // A variant of the declared union. Coercion leaves a variant
            // alone, and nothing past the union check applies to a union.
            (Value::Variant { union, .. }, TypeRef::Named { path, .. }) => {
                match self.named_slot(path) {
                    NamedSlot::Union { name: Some(name) } => union == &name,
                    NamedSlot::Union { name: None } => path_matches_suffix(path, union),
                    _ => false,
                }
            }
            // A list of a union or a plain named type, none of whose
            // elements coercion would touch: a field holding it judges
            // each element by `value_matches_type_ref` and nothing more.
            (Value::List(items), TypeRef::List(inner)) => {
                let TypeRef::Named { path, .. } = inner.as_ref() else {
                    return false;
                };
                // Coercion infers a bare record's variant for a union
                // element type, and resolves a unit for any; a record
                // against a plain type it leaves alone.
                let union = match self.named_slot(path) {
                    NamedSlot::Union { .. } => true,
                    NamedSlot::Plain => false,
                    _ => return false,
                };
                items.iter().all(|item| match item {
                    Value::None => true,
                    Value::PendingUnit { .. } => false,
                    Value::Record { .. } if union => false,
                    item => value_matches_type_ref(self, item, inner),
                })
            }
            _ => false,
        }
    }

    /// Whether `ty` is made only of what a signature alone may say — a
    /// value class, a bare interface, a function type, or a list of
    /// those — so that no conversion applies anywhere inside it.
    fn is_signature_only(&self, ty: &TypeRef) -> bool {
        match ty {
            TypeRef::Named { path, args } => {
                ValueClass::of(path, args).is_some()
                    || matches!(self.named_slot(path), NamedSlot::Interface(_))
            }
            TypeRef::Function { .. } => true,
            TypeRef::List(inner) => self.is_signature_only(inner),
            _ => false,
        }
    }

    /// Why `value` misses a signature-only type ([`Self::is_signature_only`]),
    /// phrased to follow "got", or `None` when it fits.
    fn signature_misfit(&self, value: &Value, ty: &TypeRef) -> Option<String> {
        match ty {
            TypeRef::Named { path, args } => {
                if let Some(class) = ValueClass::of(path, args) {
                    return (!class.admits(value)).then(|| value_kind(value));
                }
                let NamedSlot::Interface(fqn) = self.named_slot(path) else {
                    return None;
                };
                let iface = self.interface(&fqn)?;
                iface_misfit(self, value, iface.ast).map(|misfit| match misfit {
                    IfaceMisfit::Missing(f) => {
                        format!("{} without field '{}'", value_kind(value), f.name)
                    }
                    IfaceMisfit::Mistyped(f, got) => format!(
                        "{} whose field '{}' is {got}, not {}",
                        value_kind(value),
                        f.name,
                        f.ty
                    ),
                })
            }
            TypeRef::Function { params, .. } => match value {
                Value::Function(f) if f.params().len() == params.len() => None,
                Value::Function(f) => Some(fn_type_of(f)),
                other => Some(value_kind(other)),
            },
            TypeRef::List(inner) => {
                let Value::List(items) = value else {
                    return Some(value_kind(value));
                };
                // A `none` element is always legal, as it is in a field.
                items.iter().enumerate().find_map(|(i, item)| {
                    if matches!(item, Value::None) {
                        return None;
                    }
                    self.signature_misfit(item, inner)
                        .map(|got| format!("a list whose element [{i}] is {got}"))
                })
            }
            _ => None,
        }
    }

    /// What the written path `path` names, as far as checking a value
    /// against it goes. Resolved from the document's namespace, as the
    /// argument coercion it replaces always was, and memoised.
    pub(super) fn named_slot(&self, path: &[String]) -> NamedSlot {
        if let Ok(memo) = self.named_slot_memo.read()
            && let Some(hit) = memo.get(path)
        {
            return hit.clone();
        }
        let resolved = self
            .resolve_path_in(path, &self.file_ns)
            .map(|p| p.join("."))
            .unwrap_or_else(|| path.join("."));
        let raw = path.join(".");
        let interface = [&resolved, &raw]
            .into_iter()
            .find(|fqn| self.interface(fqn).is_some());
        let slot = if let Some(fqn) = interface {
            NamedSlot::Interface(fqn.clone())
        } else if self.union_fqn_for_path(path).is_some() {
            NamedSlot::Union {
                name: self.union_decl(&raw).map(|decl| decl.ast.name.clone()),
            }
        } else if self.symbol_set(&resolved).is_some()
            || self
                .type_decl(&resolved)
                .is_some_and(|decl| decl.ast.alias.is_some())
        {
            NamedSlot::Full
        } else {
            NamedSlot::Plain
        };
        if let Ok(mut memo) = self.named_slot_memo.write() {
            memo.insert(path.to_vec(), slot.clone());
        }
        slot
    }

    /// Hold each argument to its parameter's type, converting it as a
    /// field declared with that type would. The error names argument
    /// `index` (from 1), the parameter, and the function, and points at
    /// the argument when the call site wrote one. A host call
    /// (`Document::call_value`) has no call site in any file, so its error
    /// points at the function it reached instead.
    pub(super) fn admit_arguments(
        &self,
        f: &FnValue,
        args: &[Value],
        site: &CallSite<'_>,
    ) -> Result<Vec<Value>, EvalError> {
        let mut out = Vec::with_capacity(args.len());
        for (i, (param, value)) in f.params().iter().zip(args).enumerate() {
            let span = site.arg_span(i);
            match self.admit_signature(value.clone(), param.ty(), span)? {
                Ok(value) => out.push(value),
                Err(got) => {
                    let at = |span| {
                        EvalError::argument_type(
                            site.name.or(f.name()),
                            i + 1,
                            param.name(),
                            param.ty().to_string(),
                            got,
                            span,
                        )
                    };
                    return Err(match (span.is_empty(), fn_span(f)) {
                        (true, Some(own)) => self.with_body_origin(at(own), &f.body),
                        _ => at(span),
                    });
                }
            }
        }
        Ok(out)
    }

    /// Hold a body's result to the declared return type, converting it as
    /// a field declared with that type would. The error points at the
    /// body, in the file the body was written in; a literal body carries
    /// no span, so then at the whole function literal.
    pub(super) fn admit_return(
        &self,
        f: &FnValue,
        value: Value,
        site: &CallSite<'_>,
    ) -> Result<Value, EvalError> {
        let own = fn_span(f);
        let span = own.unwrap_or(site.span);
        let error = match self.admit_signature(value, f.return_ty(), span) {
            Ok(Ok(value)) => return Ok(value),
            Ok(Err(got)) => {
                EvalError::return_type(site.name.or(f.name()), f.return_ty().to_string(), got, span)
            }
            Err(error) => error,
        };
        Err(match own {
            Some(_) => self.with_body_origin(error, &f.body),
            None => error,
        })
    }
}

/// Where a signature error about `f` itself points: its body, or the
/// whole literal when the body is a literal value (which carries no span
/// of its own). `None` when neither is known.
fn fn_span(f: &FnValue) -> Option<Span> {
    [super::span_of(&f.body), f.span]
        .into_iter()
        .find(|span| !span.is_empty())
}

/// Where a function is being called from: the name the call knows it
/// by, the argument expressions (for spans), and the call's own span.
pub(crate) struct CallSite<'s> {
    /// The name the callee was written as (`f` in `f(x)`, `apply` in
    /// `s.apply(x)`); `None` for a callback a builtin invokes or a host
    /// call, where the function value's own binding name is used.
    pub(crate) name: Option<&'s str>,
    /// The argument expressions as written, when the call is in source.
    pub(crate) args: Option<&'s [ast::Expr]>,
    /// The call's span.
    pub(crate) span: Span,
}

impl CallSite<'_> {
    /// A call with no source of its own: a builtin's callback, a host.
    pub(crate) fn bare(span: Span) -> CallSite<'static> {
        CallSite {
            name: None,
            args: None,
            span,
        }
    }

    /// Span of argument `i` as written, else the call's. A literal
    /// argument carries no span of its own, so it gets the call's too.
    fn arg_span(&self, i: usize) -> Span {
        self.args
            .and_then(|args| args.get(i))
            .map(super::span_of)
            .filter(|span| !span.is_empty())
            .unwrap_or(self.span)
    }
}
