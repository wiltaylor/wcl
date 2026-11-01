//! Open-time checking of the declarations a source writes.
//!
//! Runs once, from [`validate_document`](crate::doc::validate), after
//! that pass has resolved the root file's namespace and `use` aliases and
//! before any evaluation happens. Four rules live here, and all four
//! are about a declaration being well-formed on its own terms:
//!
//! - every name a type reference mentions resolves to something declared,
//!   and an interface is only named through a reference ([`check_type_ref`]);
//! - every function signature's parameter and return types pass the same
//!   check, though the evaluator never enforces them against values;
//! - a union declares each variant name once, and a `symbol_set` each
//!   symbol once;
//! - a `connection`'s endpoints resolve and its kind names a `symbol_set`.
//!
//! The rules cover every source the document opens with: the root file
//! and each eagerly-imported file, transitively. Each file is checked in
//! the context its names resolve in at runtime
//! ([`Document::resolve_path_in`](crate::doc::Document::resolve_path_in)):
//! its own namespace first, then the *root's* `use` aliases and search
//! path, against the whole document's declarations. An imported file's
//! own `use` lines bind nothing at runtime, so they bind nothing here.
//! A failure renders against the file that wrote the declaration.
//!
//! In-block imports load lazily, during evaluation. Each is checked by
//! the same rules when it loads ([`check_lazy_import`]), against the
//! document's declarations plus its own; a failure there is the import's
//! failure, an [`EvalError::ImportFailed`](crate::EvalError::ImportFailed)
//! rendered against the loaded file.
//!
//! The `extends` graph is checked from here too, by
//! [`extends`](super::extends): its parents per file, the graph once.
//!
//! Failures are [`ParseError`]s rather than the [`EvalError`](crate::EvalError)s
//! the rest of this module deals in: a document whose declarations don't
//! resolve never opens, so there is nothing to evaluate against. A lazily
//! imported file's failure is a `ParseError` too, which the loader wraps
//! as the import's `EvalError`, keeping the file and span it names.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::ast::{self, Span, TypeRef};
use crate::diagnostics::ParseError;

use crate::doc::Document;
use crate::doc::cells::LoadedImport;
use crate::doc::expr_nodes::{ExprNode, visit_expr_nodes};
use crate::doc::imports::{collect_import_namespaces, collect_import_symbols};
use crate::doc::validate::{open_error, resolve_path};

use super::extends::{check_extends_targets, validate_extends};

/// Snapshot of the bookkeeping `validate_document` builds up before it
/// reaches its TypeRef / extends / connection passes. Threaded through
/// the deep validators so they can resolve paths against the document's
/// alias/wildcard tables without 11-arg signatures.
pub(in crate::doc) struct CheckContext<'a> {
    /// Every type name the document declares.
    pub(in crate::doc) declared: &'a HashSet<Vec<String>>,
    /// Every interface name the document declares.
    pub(in crate::doc) interfaces: &'a HashSet<Vec<String>>,
    /// Every `symbol_set` name the document declares, synthetic ones
    /// included — what a connection's kind must name.
    pub(in crate::doc) symbol_sets: &'a HashSet<Vec<String>>,
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
    /// Every eagerly-imported file, nested as loaded. The `extends`
    /// checks read their declarations so a parent in another file is
    /// checked like one in this file.
    pub(in crate::doc) imports: &'a [LoadedImport],
    /// The environment's synthetic types, which may be parents too.
    pub(in crate::doc) synthetic: &'a [ast::TypeDecl],
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
        // A function type is a signature wherever it is written, so its
        // parameter and return types get the signature allowances: a
        // `lower = fn(…) -> list<any>` field is typed by its literal.
        TypeRef::Function { params, return_ty } => {
            for p in params {
                check_signature_type(p, ty_span, cx)?;
            }
            check_signature_type(return_ty, ty_span, cx)
        }
    }
}

/// The builtin signature notation's value classes, which a function
/// signature may name although no field can: any value, any number,
/// any integer, any record.
const SIGNATURE_WORDS: &[&str] = &["any", "number", "integer", "record"];

/// Check that a function literal's parameter and return types resolve.
///
/// Two allowances separate a signature from a field slot. An interface
/// may be named directly: a field slot holds a value, so an interface
/// there must go through `&`, while a signature only documents what
/// flows through it. And the [`SIGNATURE_WORDS`] stand for a whole class
/// of values, at any depth (`list<any>`). They are not types a field may
/// declare, only words a signature may use, and they are how the builtin
/// signatures and existing documents already spell those classes.
fn check_signature(function: &ast::FunctionLit, cx: &CheckContext<'_>) -> Result<(), ParseError> {
    for param in &function.params {
        check_signature_type(&param.ty, param.ty_span, cx)?;
    }
    check_signature_type(&function.return_ty, function.return_ty_span, cx)
}

/// [`check_type_ref`] for one signature annotation, with the
/// allowances [`check_signature`] describes.
fn check_signature_type(ty: &TypeRef, span: Span, cx: &CheckContext<'_>) -> Result<(), ParseError> {
    match ty {
        TypeRef::Named { path, args }
            if args.is_empty()
                && path.len() == 1
                && SIGNATURE_WORDS.contains(&path[0].as_str()) =>
        {
            Ok(())
        }
        TypeRef::Named { .. } | TypeRef::Builtin(_) => check_type_ref(ty, span, true, cx),
        TypeRef::Reference(inner) | TypeRef::List(inner) => check_signature_type(inner, span, cx),
        TypeRef::Tensor { element, .. } => check_signature_type(element, span, cx),
        TypeRef::Function { params, return_ty } => {
            for p in params {
                check_signature_type(p, span, cx)?;
            }
            check_signature_type(return_ty, span, cx)
        }
    }
}

/// Check every declaration the document opens with: the root file's
/// `ast` under `cx`, then each eagerly-imported file under its own
/// context, then the `extends` graph once over all of them.
pub(in crate::doc) fn check_declarations(
    ast: &ast::Source,
    cx: &CheckContext<'_>,
) -> Result<(), ParseError> {
    check_source(&ast.items, cx)?;
    check_imports(cx.imports, cx, &HashSet::new())?;
    validate_extends(ast, cx, None)
}

/// Check a file an in-block `import` has just loaded, by the rules
/// [`check_declarations`] applies at open: the file itself, then each of
/// its own top-level imports, then the `extends` graph its declarations
/// join.
///
/// Names resolve against the document's declarations plus the loaded
/// file's own and its imports' — the file is checked as a unit, so a
/// type it declares is in scope for its other declarations — each file
/// in its own namespace under the root's `use` aliases and search path,
/// with the namespaces of the loaded file's imports added to that path
/// as the root's are. A file the document already loaded eagerly passed
/// these checks at open and is skipped, whether it is the loaded file
/// itself or one of its imports. A nested in-block import inside the
/// loaded file is checked when it loads in turn, against the document's
/// names and its own: the file that holds it contributes nothing.
///
/// Only the loaded file's declarations are judged by the `extends`
/// checks; the document's were judged when it opened.
pub(in crate::doc) fn check_lazy_import(
    doc: &Document,
    imp: &LoadedImport,
) -> Result<(), ParseError> {
    let mut opened = HashSet::new();
    collect_paths(&doc.eager_imports, &mut opened);
    if opened.contains(&imp.path) {
        return Ok(());
    }
    let mut names = doc.declared_names.clone();
    names.add_imported(&imp.symbols);
    let mut imported = Vec::new();
    collect_import_symbols(&imp.eager_imports, &mut imported);
    for symbols in imported {
        names.add_imported(symbols);
    }
    let mut wildcards = doc.wildcards.clone();
    let mut namespaces = Vec::new();
    collect_import_namespaces(&imp.eager_imports, &mut namespaces);
    for ns in namespaces {
        if !wildcards.contains(&ns) {
            wildcards.push(ns);
        }
    }
    let root_file = doc.src.name().to_string();
    let root = CheckContext {
        declared: &names.declared,
        interfaces: &names.interfaces,
        symbol_sets: &names.symbol_sets,
        file_ns: &doc.file_ns,
        item_aliases: &doc.item_aliases,
        ns_aliases: &doc.ns_aliases,
        wildcards: &wildcards,
        source: doc.src.inner(),
        file: &root_file,
        imports: &doc.eager_imports,
        synthetic: &doc.synthetic_types,
    };
    let file = imp.path.display().to_string();
    let cx = CheckContext {
        file_ns: &imp.file_ns,
        source: &imp.source,
        file: &file,
        ..root
    };
    check_source(&imp.items, &cx)?;
    check_imports(&imp.eager_imports, &root, &opened)?;
    validate_extends(&doc.ast, &root, Some(imp))
}

/// Every path in `imports`, transitively.
fn collect_paths(imports: &[LoadedImport], out: &mut HashSet<PathBuf>) {
    for imp in imports {
        out.insert(imp.path.clone());
        collect_paths(&imp.eager_imports, out);
    }
}

/// Run [`check_source`] over each import in `imports`, depth first in
/// load order, each under `root`'s context with the import's own
/// namespace and text, skipping any file in `checked`. A file loads once
/// per document, so each is checked once.
fn check_imports(
    imports: &[LoadedImport],
    root: &CheckContext<'_>,
    checked: &HashSet<PathBuf>,
) -> Result<(), ParseError> {
    for imp in imports {
        if !checked.contains(&imp.path) {
            let file = imp.path.display().to_string();
            let cx = CheckContext {
                file_ns: &imp.file_ns,
                source: &imp.source,
                file: &file,
                ..*root
            };
            check_source(&imp.items, &cx)?;
        }
        check_imports(&imp.eager_imports, root, checked)?;
    }
    Ok(())
}

/// Refuse a bare `@schemaless` on a `@document` type.
///
/// Every `@document` visible in a namespace merges into one root schema,
/// so a waiver on one of them has no single root to open, and it used to
/// be accepted and do nothing. The root opens one field at a time, with
/// `@schemaless` on a field declaration or on a root assignment.
/// `@schemaless(annotations = true)` still applies: it only exempts the
/// type's own decorators.
fn check_document_not_schemaless(
    t: &ast::TypeDecl,
    cx: &CheckContext<'_>,
) -> Result<(), ParseError> {
    let is_document = t
        .decorators
        .iter()
        .any(|d| d.name.len() == 1 && d.name[0] == crate::doc::BuiltinDecorator::Document.as_str());
    if !is_document {
        return Ok(());
    }
    let Some(waiver) = t
        .decorators
        .iter()
        .find(|d| matches!(d.schemaless_mode(), Some(ast::SchemalessMode::Full)))
    else {
        return Ok(());
    };
    Err(open_error(
        cx.source,
        cx.file,
        format!(
            "@schemaless cannot open the @document type '{}': put @schemaless on a field \
             declaration or on a root assignment instead",
            t.name.join(".")
        ),
        waiver.span,
        "not allowed on a @document type",
    ))
}

/// Check one source's declarations: type references resolve, variant
/// and symbol names are unique, connection endpoints and kind sets name
/// the right things, function signatures name real types, and every
/// `extends` parent resolves.
fn check_source(items: &[ast::Item], cx: &CheckContext<'_>) -> Result<(), ParseError> {
    let (source, file) = (cx.source, cx.file);
    let (file_ns, declared) = (cx.file_ns, cx.declared);
    let (item_aliases, ns_aliases, wildcards) = (cx.item_aliases, cx.ns_aliases, cx.wildcards);
    for item in items {
        match item {
            ast::Item::TypeDecl(t) => {
                check_document_not_schemaless(t, cx)?;
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
                if !cx.symbol_sets.contains(&kind_set_fqn) {
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

    // 5. Function signatures, wherever a function literal or `fn` item
    // appears. An annotation the evaluator never checks against a value
    // must still name a type that exists.
    let mut signatures = Ok(());
    visit_expr_nodes(items, &mut |node| {
        if let (Ok(()), ExprNode::Function(function)) = (&signatures, node) {
            signatures = check_signature(function, cx);
        }
    });
    signatures?;

    // 6. Every `extends` parent resolves. Cycles and field conflicts are
    // the whole graph's, checked once by `check_declarations`.
    check_extends_targets(items, cx)
}
