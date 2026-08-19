//! Union declarations: the checks that keep variant shapes distinct.
//!
//! [`validate_union`] runs once, at open time, over every `union`
//! declaration: variant names must be unique across the whole `extends`
//! chain, and no two variants may share a *shape*. That second rule is
//! what makes shape dispatch in [`variant_dispatch`](super::variant_dispatch)
//! total — if two variants could accept the same record, the dispatcher
//! would have no principled way to choose, so the ambiguity is rejected
//! where it is written rather than where it is used.

use crate::ast::{self, TypeRef};
use crate::diagnostics::EvalError;

use crate::doc::Document;
use crate::doc::views::UnionDecl;

/// Render a comma-separated list of all variant names across the
/// given union slots — used to enrich `UnregisteredKind` errors with
/// a "did you mean" hint when a nearby `@children(SomeUnion)` field
/// exists.
pub(crate) fn format_union_variants_hint(doc: &Document, slots: &[UnionDecl<'_>]) -> String {
    let mut names: Vec<String> = Vec::new();
    for u in slots {
        if let Ok(effective) = doc.effective_variants_of(u.ast) {
            for v in effective {
                names.push(format!("{}::{}", u.ast.name.join("."), v.name));
            }
        }
    }
    names.join(", ")
}

/// Declaration-time validation for a single union: cycles, duplicate
/// variant names across the `extends` chain, and structural-shape
/// collisions between variant bodies that would make dispatch
/// ambiguous.
pub(crate) fn validate_union(doc: &Document, u: &ast::UnionDecl) -> Vec<EvalError> {
    use crate::diagnostics::SchemaViolationKind as Kind;
    let mut out = Vec::new();
    let effective = match doc.effective_variants_of(u) {
        Ok(v) => v,
        Err(e) => {
            out.push(e);
            return out;
        }
    };
    // Duplicate variant names across the chain: walk own + all parents
    // and report any name appearing more than once. effective_variants
    // dedups silently — we re-walk the raw lists here to catch.
    let mut seen: std::collections::HashMap<String, ast::Span> = Default::default();
    fn walk(
        doc: &Document,
        u: &ast::UnionDecl,
        seen: &mut std::collections::HashMap<String, ast::Span>,
        out: &mut Vec<EvalError>,
        visiting: &mut std::collections::HashSet<String>,
    ) {
        use crate::diagnostics::SchemaViolationKind as Kind;
        let key = u.name.join(".");
        if visiting.contains(&key) {
            return;
        }
        visiting.insert(key);
        for parent_path in &u.extends {
            let resolved = doc
                .resolve_path_in(parent_path, &doc.file_ns)
                .map(|p| p.join("."))
                .unwrap_or_else(|| parent_path.join("."));
            if let Some(p) = doc
                .union_decl(&resolved)
                .or_else(|| doc.union_decl(&parent_path.join(".")))
            {
                walk(doc, p.ast, seen, out, visiting);
            }
        }
        for v in &u.variants {
            if let Some(prev) = seen.get(&v.name) {
                EvalError::push_schema_violation(
                    out,
                    Kind::DuplicateVariant,
                    format!(
                        "variant '{}' is declared more than once in union '{}' (first at offset {})",
                        v.name,
                        u.name.join("."),
                        prev.start,
                    ),
                    v.span,
                );
            } else {
                seen.insert(v.name.clone(), v.span);
            }
        }
    }
    walk(
        doc,
        u,
        &mut seen,
        &mut out,
        &mut std::collections::HashSet::new(),
    );
    // Structural-shape collisions among effective variants. Each pair
    // is checked once; collisions are flagged on the second offender.
    for i in 0..effective.len() {
        for j in (i + 1)..effective.len() {
            if variant_bodies_collide(&effective[i].body, &effective[j].body) {
                EvalError::push_schema_violation(
                    &mut out,
                    Kind::VariantShapeCollision,
                    format!(
                        "variants '{}' and '{}' in union '{}' have identical bodies",
                        effective[i].name,
                        effective[j].name,
                        u.name.join("."),
                    ),
                    effective[j].span,
                );
            }
        }
    }
    out
}

/// Bodies "collide" when they're indistinguishable for dispatch:
/// same set of record-field (name, type) pairs, or identical Unit /
/// TypeRef / InterfaceRef references.
///
/// Type arguments don't distinguish anything — dispatch resolves a
/// named type by path — so `A(S<X>)` and `B(S<Y>)` collide just as
/// `A(S)` and `B(S)` do.
fn variant_bodies_collide(a: &ast::VariantBody, b: &ast::VariantBody) -> bool {
    use ast::VariantBody as VB;
    match (a, b) {
        (VB::Unit, VB::Unit) => true,
        (VB::TypeRef { ty: a, .. }, VB::TypeRef { ty: b, .. }) => a.same_ignoring_type_args(b),
        (VB::InterfaceRef { iface: a, .. }, VB::InterfaceRef { iface: b, .. }) => a == b,
        (VB::Record { fields: af, .. }, VB::Record { fields: bf, .. }) => {
            if af.len() != bf.len() {
                return false;
            }
            let mut a_sorted: Vec<(&String, &TypeRef)> =
                af.iter().map(|f| (&f.name, &f.ty)).collect();
            let mut b_sorted: Vec<(&String, &TypeRef)> =
                bf.iter().map(|f| (&f.name, &f.ty)).collect();
            a_sorted.sort_by_key(|(n, _)| (*n).clone());
            b_sorted.sort_by_key(|(n, _)| (*n).clone());
            a_sorted
                .iter()
                .zip(b_sorted.iter())
                .all(|((an, at), (bn, bt))| an == bn && at.same_ignoring_type_args(bt))
        }
        _ => false,
    }
}

/// Whether a pattern's (possibly unqualified) union path matches a
/// fully-qualified union name, comparing from the right.
pub(super) fn path_matches_suffix(pat_path: &[String], union_fqn: &[String]) -> bool {
    if pat_path.len() > union_fqn.len() {
        return false;
    }
    let offset = union_fqn.len() - pat_path.len();
    union_fqn[offset..] == *pat_path
}
