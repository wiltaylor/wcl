//! Interface / type compatibility helpers.
//!
//! Used by the reference-acceptance check (a `&T` field whose target's
//! concrete type implements interface `T`) and by `ResolvedType` equality
//! in the structural-subtyping path. Each lookup is pointer-equality on the
//! underlying AST node, so name-based comparison isn't needed.

use crate::ast::Span;
use crate::data::{DataKind, DataRef};
use crate::error::EvalError;
use crate::value::TypeRef;

use super::{DeclName, Document, InterfaceDecl, ResolvedType, TypeDecl};

/// Find the declared `TypeDecl` corresponding to the concrete value at a
/// `DataRef`, when one is known. `None` for refs whose target type isn't a
/// named `TypeDecl` (builtins, unions, etc.).
pub(super) fn dataref_concrete_type<'a>(
    dr: &DataRef<'a>,
    doc: &'a Document,
) -> Option<TypeDecl<'a>> {
    match dr.inner() {
        DataKind::Block(b) => b.schema().or_else(|| doc.block_schema(b.kind())),
        DataKind::Field(f) => {
            let ty = f.declared_type_ref()?;
            match ty {
                TypeRef::Named(path) => doc.type_decl(&path.join(".")),
                _ => None,
            }
        }
        _ => None,
    }
}

pub(super) fn same_type_decl(a: &TypeDecl<'_>, b: &TypeDecl<'_>) -> bool {
    std::ptr::eq(a.ast, b.ast)
}

pub(super) fn check_interface_conformance(
    doc: &Document,
    iface: &InterfaceDecl<'_>,
    target_decl: &TypeDecl<'_>,
    span: Span,
) -> Result<(), EvalError> {
    use crate::error::SchemaViolationKind as Kind;
    let iface_fqn = iface.full_name();
    let target_fqn = target_decl.full_name();
    for if_field in iface.effective_fields() {
        let Some(tg_field) = target_decl.effective_field(if_field.name()) else {
            return Err(EvalError::schema_violation(
                Kind::InterfaceNotImplemented,
                format!(
                    "type '{}' does not implement interface '{}': missing field '{}'",
                    target_fqn,
                    iface_fqn,
                    if_field.name(),
                ),
                span,
            ));
        };
        let iface_ty = doc.resolve(if_field.type_ref());
        let tg_ty = doc.resolve(tg_field.type_ref());
        if !iface_field_type_compatible(&iface_ty, &tg_ty) {
            return Err(EvalError::schema_violation(
                Kind::InterfaceNotImplemented,
                format!(
                    "type '{}' does not implement interface '{}': field '{}' has incompatible type",
                    target_fqn,
                    iface_fqn,
                    if_field.name(),
                ),
                span,
            ));
        }
    }
    Ok(())
}

/// Interface-conformance type check. Strict equality for everything
/// except function fields, which are treated as purely structural:
/// the impl need only declare a function under the same name — the
/// parameter list and return type are unconstrained. That lets a
/// concrete impl narrow `fn(&Iface) -> R` (declared on the
/// interface) to `fn(Concrete) -> Whatever`, and lets recursive
/// lowerings return intermediate union types that the host post-
/// processes. The presence check (function vs non-function) is
/// still enforced via the catch-all fallback.
fn iface_field_type_compatible(iface: &ResolvedType<'_>, tg: &ResolvedType<'_>) -> bool {
    if matches!(
        (iface, tg),
        (ResolvedType::Function { .. }, ResolvedType::Function { .. })
    ) {
        return true;
    }
    resolved_types_equal(iface, tg)
}

pub(super) fn resolved_types_equal(a: &ResolvedType<'_>, b: &ResolvedType<'_>) -> bool {
    match (a, b) {
        (ResolvedType::Builtin(x), ResolvedType::Builtin(y)) => x == y,
        (ResolvedType::Named(x), ResolvedType::Named(y)) => std::ptr::eq(x.ast, y.ast),
        (ResolvedType::Interface(x), ResolvedType::Interface(y)) => std::ptr::eq(x.ast, y.ast),
        (ResolvedType::Union(x), ResolvedType::Union(y)) => std::ptr::eq(x.ast, y.ast),
        (ResolvedType::SymbolSet(x), ResolvedType::SymbolSet(y)) => std::ptr::eq(x.ast, y.ast),
        (ResolvedType::Reference(x), ResolvedType::Reference(y)) => resolved_types_equal(x, y),
        (ResolvedType::List(x), ResolvedType::List(y)) => resolved_types_equal(x, y),
        (
            ResolvedType::Tensor {
                element: ax,
                dims: adims,
            },
            ResolvedType::Tensor {
                element: bx,
                dims: bdims,
            },
        ) => resolved_types_equal(ax, bx) && adims == bdims,
        (
            ResolvedType::Function {
                params: ap,
                return_ty: ar,
            },
            ResolvedType::Function {
                params: bp,
                return_ty: br,
            },
        ) => {
            ap.len() == bp.len()
                && ap
                    .iter()
                    .zip(bp.iter())
                    .all(|(a, b)| resolved_types_equal(a, b))
                && resolved_types_equal(ar, br)
        }
        _ => false,
    }
}
