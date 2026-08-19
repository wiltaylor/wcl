//! Open-time checking of the declarations a source writes.
//!
//! Runs once, from [`validate_document`](crate::doc::validate), after
//! that pass has resolved the file's namespace and `use` aliases and
//! before any evaluation happens. Three rules live here, and all three
//! are about a declaration being well-formed on its own terms:
//!
//! - every name a type reference mentions resolves to something declared,
//!   and an interface is only named through a reference ([`check_type_ref`]);
//! - a union declares each variant name once, and a `symbol_set` each
//!   symbol once;
//! - a `connection`'s endpoints resolve and its kind names a `symbol_set`.
//!
//! The `extends` graph is checked from here too, by
//! [`extends`](super::extends).
//!
//! Failures are [`ParseError`]s rather than the [`EvalError`](crate::EvalError)s
//! the rest of this module deals in: a document whose declarations don't
//! resolve never opens, so there is nothing to evaluate against.

use std::collections::{HashMap, HashSet};

use crate::ast::{self, Span, TypeRef};
use crate::diagnostics::ParseError;
use crate::symbols::{SymbolIndex, SymbolKind};

use crate::doc::validate::{open_error, resolve_path};

use super::extends::validate_extends;

/// Snapshot of the bookkeeping `validate_document` builds up before it
/// reaches its TypeRef / extends / connection passes. Threaded through
/// the deep validators so they can resolve paths against the document's
/// alias/wildcard tables without 11-arg signatures.
pub(in crate::doc) struct CheckContext<'a> {
    /// Every type name the document declares.
    pub(in crate::doc) declared: &'a HashSet<Vec<String>>,
    /// Every interface name the document declares.
    pub(in crate::doc) interfaces: &'a HashSet<Vec<String>>,
    /// Namespace of the source being checked.
    pub(in crate::doc) file_ns: &'a [String],
    /// Item aliases in scope.
    pub(in crate::doc) item_aliases: &'a HashMap<String, Vec<String>>,
    /// Namespace aliases in scope.
    pub(in crate::doc) ns_aliases: &'a HashMap<String, Vec<String>>,
    /// Wildcard namespaces in scope.
    pub(in crate::doc) wildcards: &'a [Vec<String>],
    /// Source text, for rendering diagnostics.
    pub(in crate::doc) source: &'a str,
    /// Source name, for rendering diagnostics.
    pub(in crate::doc) file: &'a str,
}

/// Check that every name a type reference mentions resolves to
/// something the document declares.
fn check_type_ref(
    t: &TypeRef,
    ty_span: Span,
    parent_is_ref: bool,
    cx: &CheckContext<'_>,
) -> Result<(), ParseError> {
    match t {
        TypeRef::Builtin(_) => Ok(()),
        TypeRef::Named { path, .. } => {
            let Some(resolved) = resolve_path(
                path,
                cx.file_ns,
                cx.item_aliases,
                cx.ns_aliases,
                cx.wildcards,
                cx.declared,
            ) else {
                return Err(open_error(
                    cx.source,
                    cx.file,
                    format!("unknown type '{}'", path.join(".")),
                    ty_span,
                    "type not declared",
                ));
            };
            if cx.interfaces.contains(&resolved) && !parent_is_ref {
                return Err(open_error(
                    cx.source,
                    cx.file,
                    format!(
                        "interface '{}' must be used through a reference (`&{}`)",
                        path.join("."),
                        path.join(".")
                    ),
                    ty_span,
                    "interface in non-reference position",
                ));
            }
            Ok(())
        }
        TypeRef::Reference(inner) => check_type_ref(inner, ty_span, true, cx),
        // `list<Interface>` is allowed: the list element is a slot for
        // a `@children(Interface)` collection; the interface tag never
        // materialises as a stored value, only routes child blocks.
        TypeRef::List(inner) => check_type_ref(inner, ty_span, true, cx),
        TypeRef::Tensor { element, .. } => check_type_ref(element, ty_span, false, cx),
        TypeRef::Function { params, return_ty } => {
            for p in params {
                check_type_ref(p, ty_span, false, cx)?;
            }
            check_type_ref(return_ty, ty_span, false, cx)
        }
    }
}

/// Check every declaration in `ast`: type references resolve, variant
/// and symbol names are unique, connection endpoints and kind sets name
/// the right things, and the `extends` graph is sound.
pub(in crate::doc) fn check_declarations(
    ast: &ast::Source,
    symbols: &SymbolIndex,
    cx: &CheckContext<'_>,
) -> Result<(), ParseError> {
    let (source, file) = (cx.source, cx.file);
    let (file_ns, declared) = (cx.file_ns, cx.declared);
    let (item_aliases, ns_aliases, wildcards) = (cx.item_aliases, cx.ns_aliases, cx.wildcards);
    for item in &ast.items {
        match item {
            ast::Item::TypeDecl(t) => {
                for f in &t.fields {
                    check_type_ref(&f.ty, f.ty_span, false, cx)?;
                }
            }
            ast::Item::InterfaceDecl(i) => {
                for f in &i.fields {
                    check_type_ref(&f.ty, f.ty_span, false, cx)?;
                }
            }
            ast::Item::UnionDecl(u) => {
                let mut seen: HashSet<String> = HashSet::new();
                for v in &u.variants {
                    if !seen.insert(v.name.clone()) {
                        return Err(open_error(
                            source,
                            file,
                            format!(
                                "duplicate variant '{}' in union '{}'",
                                v.name,
                                u.name.join(".")
                            ),
                            v.span,
                            "duplicate variant",
                        ));
                    }
                    match &v.body {
                        ast::VariantBody::Record { fields, .. } => {
                            for f in fields {
                                check_type_ref(&f.ty, f.ty_span, false, cx)?;
                            }
                        }
                        ast::VariantBody::TypeRef { ty, ty_span } => {
                            check_type_ref(ty, *ty_span, false, cx)?;
                        }
                        ast::VariantBody::InterfaceRef { iface, iface_span } => {
                            // InterfaceRef body is the moral equivalent of
                            // `&Iface` — the interface is the contract, the
                            // payload is any value satisfying it. Pass
                            // `parent_is_ref=true` so interface paths are
                            // accepted (the same rule that lets `&Iface`
                            // appear in field types).
                            check_type_ref(
                                &crate::ast::TypeRef::named(iface.clone()),
                                *iface_span,
                                true,
                                cx,
                            )?;
                        }
                        ast::VariantBody::Unit => {}
                    }
                }
            }
            ast::Item::SymbolSetDecl(s) => {
                let mut seen: HashSet<String> = HashSet::new();
                for entry in &s.symbols {
                    if !seen.insert(entry.name.clone()) {
                        return Err(open_error(
                            source,
                            file,
                            format!(
                                "duplicate symbol '{}' in symbol_set '{}'",
                                entry.name,
                                s.name.join(".")
                            ),
                            entry.span,
                            "duplicate symbol",
                        ));
                    }
                }
            }
            ast::Item::ConnectionDecl(c) => {
                check_type_ref(&c.source, c.source_span, false, cx)?;
                check_type_ref(&c.destination, c.destination_span, false, cx)?;
                // kind_set must resolve to a declared symbol_set FQN.
                let resolved = resolve_path(
                    &c.kind_set,
                    file_ns,
                    item_aliases,
                    ns_aliases,
                    wildcards,
                    declared,
                );
                let kind_set_fqn = match resolved {
                    Some(fqn) => fqn,
                    None => {
                        return Err(open_error(
                            source,
                            file,
                            format!(
                                "unknown symbol_set '{}' in connection '{}'",
                                c.kind_set.join("."),
                                c.name.join(".")
                            ),
                            c.kind_set_span,
                            "symbol_set not declared",
                        ));
                    }
                };
                let kind_set_fqn_str = kind_set_fqn.join(".");
                let is_symbol_set = symbols
                    .lookup(&kind_set_fqn_str)
                    .map(|r| matches!(r.kind, SymbolKind::SymbolSetDecl))
                    .unwrap_or(false);
                if !is_symbol_set {
                    return Err(open_error(
                        source,
                        file,
                        format!(
                            "kind '{}' in connection '{}' must be a symbol_set",
                            kind_set_fqn_str,
                            c.name.join(".")
                        ),
                        c.kind_set_span,
                        "not a symbol_set",
                    ));
                }
            }
            _ => {}
        }
    }

    // 5. `extends` validation: unknown parents, cycles, and
    // conflicting field types across the effective field set.
    validate_extends(ast, cx)?;

    Ok(())
}
