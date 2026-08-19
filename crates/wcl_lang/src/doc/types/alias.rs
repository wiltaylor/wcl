//! Transparent type aliases: `type Port = u16`.
//!
//! An alias is a name for another type, not a type of its own, so almost
//! everything that reads a declared type wants the target instead — the
//! schema value checks, the constraint decorators that ride on each link,
//! and the `@unit` factors a unit-bearing alias declares. Each walk is
//! capped at [`ALIAS_DEPTH`](super::ALIAS_DEPTH) links, so the cyclic
//! `type A = B  type B = A` (which parses) cannot hang it.
//!
//! The un-peeled view is [`resolve`](super::resolve): it stops at the
//! alias declaration and hands it back.

use crate::value::Value;

use crate::doc::Document;
use crate::doc::views::{DeclName, TypeDecl};

use super::ALIAS_DEPTH;

impl Document {
    /// Resolve type aliases (`type Port = u16`) inside `ty`, deeply:
    /// a `Named` ref whose declaration is an alias is replaced by its
    /// target (transitively, cycle-capped), and list / reference /
    /// tensor element types resolve recursively. Non-alias refs come
    /// back unchanged. Used by the schema value checks so a field
    /// declared with an alias validates against the target type.
    pub fn resolve_alias(&self, ty: &crate::ast::TypeRef) -> crate::ast::TypeRef {
        self.resolve_alias_in(ty, &self.file_ns)
    }

    /// Resolve a path through the `use` aliases visible in the given
    /// namespace, returning the fully-qualified form.
    pub(crate) fn resolve_alias_in(
        &self,
        ty: &crate::ast::TypeRef,
        context_ns: &[String],
    ) -> crate::ast::TypeRef {
        use crate::ast::TypeRef as T;
        fn go(doc: &Document, ty: &T, context_ns: &[String], depth: u8) -> T {
            if depth == 0 {
                return ty.clone(); // alias cycle — give up, stay permissive
            }
            match ty {
                T::Named { path, args } => {
                    let resolved_path = doc
                        .resolve_path_in(path, context_ns)
                        .unwrap_or_else(|| path.clone());
                    match doc.type_decl(&resolved_path.join(".")) {
                        Some(declaration) if declaration.ast.alias.is_some() => go(
                            doc,
                            declaration.ast.alias.as_ref().expect("alias checked"),
                            declaration.file_ns(),
                            depth - 1,
                        ),
                        _ => T::Named {
                            // Preserve the authored path for runtime value
                            // matching, which deliberately accepts a shorter
                            // suffix (`RelatedEdge` against a namespaced
                            // variant tag). Namespace resolution above is
                            // needed only to locate an alias declaration.
                            path: path.clone(),
                            args: args
                                .iter()
                                .map(|arg| go(doc, arg, context_ns, depth - 1))
                                .collect(),
                        },
                    }
                }
                T::List(inner) => T::List(Box::new(go(doc, inner, context_ns, depth - 1))),
                T::Reference(inner) => {
                    T::Reference(Box::new(go(doc, inner, context_ns, depth - 1)))
                }
                T::Tensor { element, dims } => T::Tensor {
                    element: Box::new(go(doc, element, context_ns, depth - 1)),
                    dims: dims.clone(),
                },
                T::Function { params, return_ty } => T::Function {
                    params: params
                        .iter()
                        .map(|param| go(doc, param, context_ns, depth - 1))
                        .collect(),
                    return_ty: Box::new(go(doc, return_ty, context_ns, depth - 1)),
                },
                other => other.clone(),
            }
        }
        go(self, ty, context_ns, ALIAS_DEPTH)
    }

    /// The chain of alias declarations behind `ty`, outermost first —
    /// empty for a non-alias type. Constraint decorators on each link
    /// apply to values of the aliased type.
    pub(crate) fn alias_chain(&self, ty: &crate::ast::TypeRef) -> Vec<TypeDecl<'_>> {
        self.alias_chain_in(ty, &self.file_ns)
    }

    /// Follow a chain of `use` aliases to its end, bounded so a cyclic
    /// alias cannot hang the walk.
    pub(crate) fn alias_chain_in(
        &self,
        ty: &crate::ast::TypeRef,
        context_ns: &[String],
    ) -> Vec<TypeDecl<'_>> {
        let mut out = Vec::new();
        let mut current = ty.clone();
        let mut current_ns = context_ns;
        for _ in 0..ALIAS_DEPTH {
            let crate::ast::TypeRef::Named { path, .. } = &current else {
                break;
            };
            let resolved = self
                .resolve_path_in(path, current_ns)
                .unwrap_or_else(|| path.clone());
            let Some(decl) = self.type_decl(&resolved.join(".")) else {
                break;
            };
            let Some(target) = decl.ast.alias.clone() else {
                break;
            };
            out.push(decl);
            current = target;
            current_ns = decl.file_ns();
        }
        out
    }

    /// The multiplier declared for `unit` on `ty`'s alias chain via a
    /// `@unit(name, factor)` decorator, or `None` if the type declares no
    /// such unit. Mirrors `schema_check::constraint_violation`'s
    /// alias-chain decorator walk; the factor expression evaluates through
    /// the document (so `@unit("MiB", 1024 * 1024)` works). Backs both
    /// literal-unit resolution and the `format_unit` builtin.
    pub(crate) fn unit_factor(&self, ty: &crate::ast::TypeRef, unit: &str) -> Option<Value> {
        for link in self.alias_chain(ty) {
            for d in link.ast.decorators.iter() {
                if d.name.join(".") != "unit" {
                    continue;
                }
                let Some(name_val) = d.positional.first().and_then(|e| self.eval(e).ok()) else {
                    continue;
                };
                let matches = matches!(&name_val,
                    Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) | Value::Symbol(s)
                        if s == unit);
                if !matches {
                    continue;
                }
                if let Some(factor) = d.positional.get(1).and_then(|e| self.eval(e).ok()) {
                    return Some(factor);
                }
            }
        }
        None
    }
}
