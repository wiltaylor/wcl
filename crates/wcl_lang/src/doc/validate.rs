//! Static document validation: runs once at `Document::open` time.
//!
//! Responsibilities:
//! - Namespace position / uniqueness.
//! - `use` declarations → alias tables (single-item, namespace, wildcard).
//! - Per-type/interface/union/symbol_set field-type, variant-name, and
//!   symbol-name checks (`check_type_ref`).
//! - Cross-declaration `extends` graph: unknown parents, self-extension,
//!   cycles, conflicting field types across the effective field set
//!   (`validate_extends`).
//!
//! On success returns a [`Resolved`] bundle (file namespace + alias tables)
//! which the document then stashes for runtime name resolution.

use std::collections::{HashMap, HashSet};

use miette::NamedSource;

use crate::ast::{self, Span};
use crate::error::ParseError;
use crate::symbols::{SymbolIndex, SymbolKind};
use crate::value::TypeRef;

use super::span_to_miette;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Resolved {
    pub(crate) file_ns: Vec<String>,
    pub(crate) item_aliases: HashMap<String, Vec<String>>,
    pub(crate) ns_aliases: HashMap<String, Vec<String>>,
    pub(crate) wildcards: Vec<Vec<String>>,
}

pub(crate) fn decl_fqn_matches(decl: &[String], target: &[&str]) -> bool {
    decl.len() == target.len() && decl.iter().zip(target.iter()).all(|(a, b)| a == b)
}

pub(crate) fn resolve_path(
    path: &[String],
    file_ns: &[String],
    item_aliases: &HashMap<String, Vec<String>>,
    ns_aliases: &HashMap<String, Vec<String>>,
    wildcards: &[Vec<String>],
    registry: &HashSet<Vec<String>>,
) -> Option<Vec<String>> {
    // 1. file_ns + path
    let candidate: Vec<String> = file_ns.iter().chain(path.iter()).cloned().collect();
    if registry.contains(&candidate) {
        return Some(candidate);
    }
    // 2. item alias on single-segment path
    if path.len() == 1
        && let Some(fqn) = item_aliases.get(&path[0])
        && registry.contains(fqn)
    {
        return Some(fqn.clone());
    }
    // 3. namespace alias on first segment of multi-segment path
    if path.len() > 1
        && let Some(prefix) = ns_aliases.get(&path[0])
    {
        let candidate: Vec<String> = prefix.iter().chain(path[1..].iter()).cloned().collect();
        if registry.contains(&candidate) {
            return Some(candidate);
        }
    }
    // 4. each wildcard prefix
    for w in wildcards {
        let candidate: Vec<String> = w.iter().chain(path.iter()).cloned().collect();
        if registry.contains(&candidate) {
            return Some(candidate);
        }
    }
    // 5. absolute
    if registry.contains(path) {
        return Some(path.to_vec());
    }
    None
}

pub(crate) fn open_error(
    source: &str,
    file: &str,
    message: String,
    span: Span,
    label: &str,
) -> ParseError {
    ParseError::syntax(
        message,
        NamedSource::new(file, source.to_string()),
        span_to_miette(span),
        label.to_string(),
    )
}

pub(crate) fn validate_document(
    ast: &ast::Source,
    symbols: &SymbolIndex,
    synthetic: &[ast::TypeDecl],
    imported_symbols: &[&SymbolIndex],
    import_namespaces: &[Vec<String>],
    source: &str,
    file: &str,
) -> Result<Resolved, ParseError> {
    // 1. Namespace must be first if present; at most one.
    let mut file_ns: Vec<String> = Vec::new();
    let mut saw_ns = false;
    for (idx, item) in ast.items.iter().enumerate() {
        if let ast::Item::NamespaceDecl(n) = item {
            if saw_ns {
                return Err(open_error(
                    source,
                    file,
                    "duplicate namespace declaration".to_string(),
                    n.span,
                    "duplicate namespace",
                ));
            }
            if idx != 0 {
                return Err(open_error(
                    source,
                    file,
                    "namespace declaration must be the first item in the file".to_string(),
                    n.span,
                    "must be first item",
                ));
            }
            file_ns = n.path.clone();
            saw_ns = true;
        }
    }

    // 2. Build the declared-FQN set and prefix set used for name resolution.
    // Top-level decls were already added to `symbols` by the parser (and the
    // duplicate check already fired there); we just project them into the
    // shapes that the rest of this function expects.
    let mut declared: HashSet<Vec<String>> = HashSet::new();
    let mut prefixes: HashSet<Vec<String>> = HashSet::new();
    for t in synthetic {
        let fqn = t.name.clone();
        declared.insert(fqn.clone());
        for n in 1..fqn.len() {
            prefixes.insert(fqn[..n].to_vec());
        }
    }
    // Track interface FQNs separately so `check_type_ref` can enforce
    // the "must be behind `&`" rule when a path resolves to one.
    let mut interfaces: HashSet<Vec<String>> = HashSet::new();
    for rec in symbols.iter() {
        if !matches!(
            rec.kind,
            SymbolKind::TypeDecl
                | SymbolKind::InterfaceDecl
                | SymbolKind::UnionDecl
                | SymbolKind::SymbolSetDecl
                | SymbolKind::ConnectionDecl
        ) {
            continue;
        }
        let fqn: Vec<String> = rec.fqn.split('.').map(str::to_string).collect();
        if !declared.insert(fqn.clone()) {
            // A registry-injected (synthetic) type already owns this FQN.
            return Err(open_error(
                source,
                file,
                format!("duplicate declaration '{}'", rec.fqn),
                rec.span,
                "duplicate declaration",
            ));
        }
        if matches!(rec.kind, SymbolKind::InterfaceDecl) {
            interfaces.insert(fqn.clone());
        }
        for n in 1..fqn.len() {
            prefixes.insert(fqn[..n].to_vec());
        }
    }

    // 2b. Declarations from eagerly-imported files also participate in
    // name resolution for this file's type references (e.g. a user
    // `lower` returning `list<SvgFundamental>` where the union lives in
    // an imported schema file). Insert-only: imported FQNs never trigger
    // the duplicate-declaration check — a local declaration may
    // legitimately coexist with, or shadow, an imported one.
    for idx in imported_symbols {
        for rec in idx.iter() {
            if !matches!(
                rec.kind,
                SymbolKind::TypeDecl
                    | SymbolKind::InterfaceDecl
                    | SymbolKind::UnionDecl
                    | SymbolKind::SymbolSetDecl
                    | SymbolKind::ConnectionDecl
            ) {
                continue;
            }
            let fqn: Vec<String> = rec.fqn.split('.').map(str::to_string).collect();
            declared.insert(fqn.clone());
            if matches!(rec.kind, SymbolKind::InterfaceDecl) {
                interfaces.insert(fqn.clone());
            }
            for n in 1..fqn.len() {
                prefixes.insert(fqn[..n].to_vec());
            }
        }
    }

    // 3. Use declarations.
    let mut item_aliases: HashMap<String, Vec<String>> = HashMap::new();
    let mut ns_aliases: HashMap<String, Vec<String>> = HashMap::new();
    let mut wildcards: Vec<Vec<String>> = Vec::new();
    let mut alias_taken: HashSet<String> = HashSet::new();

    let record_alias =
        |alias: String, span: Span, taken: &mut HashSet<String>| -> Result<(), ParseError> {
            if !taken.insert(alias.clone()) {
                return Err(open_error(
                    source,
                    file,
                    format!("duplicate use alias '{alias}'"),
                    span,
                    "duplicate alias",
                ));
            }
            Ok(())
        };

    for item in &ast.items {
        let ast::Item::UseDecl(u) = item else {
            continue;
        };
        match &u.form {
            ast::UseForm::Bare(alias) => {
                let path_is_leaf = declared.contains(&u.path);
                let path_is_prefix = prefixes.contains(&u.path);
                match alias {
                    None => {
                        if path_is_leaf {
                            let local = u.path.last().expect("non-empty path").clone();
                            record_alias(local.clone(), u.span, &mut alias_taken)?;
                            item_aliases.insert(local, u.path.clone());
                        } else if path_is_prefix {
                            wildcards.push(u.path.clone());
                        } else {
                            return Err(open_error(
                                source,
                                file,
                                format!("unknown use target '{}'", u.path.join(".")),
                                u.span,
                                "not declared",
                            ));
                        }
                    }
                    Some(alias_name) => {
                        if path_is_leaf {
                            record_alias(alias_name.clone(), u.span, &mut alias_taken)?;
                            item_aliases.insert(alias_name.clone(), u.path.clone());
                        } else if path_is_prefix {
                            record_alias(alias_name.clone(), u.span, &mut alias_taken)?;
                            ns_aliases.insert(alias_name.clone(), u.path.clone());
                        } else {
                            return Err(open_error(
                                source,
                                file,
                                format!("unknown use target '{}'", u.path.join(".")),
                                u.span,
                                "not declared",
                            ));
                        }
                    }
                }
            }
            ast::UseForm::List(items) => {
                if declared.contains(&u.path) {
                    return Err(open_error(
                        source,
                        file,
                        format!(
                            "expected namespace, but '{}' names a type",
                            u.path.join(".")
                        ),
                        u.span,
                        "not a namespace",
                    ));
                }
                if !u.path.is_empty() && !prefixes.contains(&u.path) {
                    return Err(open_error(
                        source,
                        file,
                        format!("unknown use target '{}'", u.path.join(".")),
                        u.span,
                        "not declared",
                    ));
                }
                for it in items {
                    let mut full = u.path.clone();
                    full.push(it.name.clone());
                    if !declared.contains(&full) {
                        return Err(open_error(
                            source,
                            file,
                            format!("unknown use target '{}'", full.join(".")),
                            it.span,
                            "not declared",
                        ));
                    }
                    let local = it.alias.clone().unwrap_or_else(|| it.name.clone());
                    record_alias(local.clone(), it.span, &mut alias_taken)?;
                    item_aliases.insert(local, full);
                }
            }
        }
    }

    // A namespaced library brought in by `import <…>` contributes its
    // namespace as a resolution search path, so this file's bare
    // references to imported declarations (e.g. a user `lower` returning
    // `list<SvgFundamental>`) resolve to `wdoc.SvgFundamental` without an
    // explicit `use wdoc`.
    for ns in import_namespaces {
        if !ns.is_empty() && !wildcards.contains(ns) {
            wildcards.push(ns.clone());
        }
    }

    // 4. TypeRef resolution + variant-name uniqueness.
    let cx = CheckContext {
        declared: &declared,
        interfaces: &interfaces,
        file_ns: &file_ns,
        item_aliases: &item_aliases,
        ns_aliases: &ns_aliases,
        wildcards: &wildcards,
        source,
        file,
    };
    for item in &ast.items {
        match item {
            ast::Item::TypeDecl(t) => {
                for f in &t.fields {
                    check_type_ref(&f.ty, f.ty_span, false, &cx)?;
                }
            }
            ast::Item::InterfaceDecl(i) => {
                for f in &i.fields {
                    check_type_ref(&f.ty, f.ty_span, false, &cx)?;
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
                                check_type_ref(&f.ty, f.ty_span, false, &cx)?;
                            }
                        }
                        ast::VariantBody::TypeRef { ty, ty_span } => {
                            check_type_ref(ty, *ty_span, false, &cx)?;
                        }
                        ast::VariantBody::InterfaceRef { iface, iface_span } => {
                            // InterfaceRef body is the moral equivalent of
                            // `&Iface` — the interface is the contract, the
                            // payload is any value satisfying it. Pass
                            // `parent_is_ref=true` so interface paths are
                            // accepted (the same rule that lets `&Iface`
                            // appear in field types).
                            check_type_ref(
                                &crate::value::TypeRef::Named(iface.clone()),
                                *iface_span,
                                true,
                                &cx,
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
                check_type_ref(&c.source, c.source_span, false, &cx)?;
                check_type_ref(&c.destination, c.destination_span, false, &cx)?;
                // kind_set must resolve to a declared symbol_set FQN.
                let resolved = resolve_path(
                    &c.kind_set,
                    &file_ns,
                    &item_aliases,
                    &ns_aliases,
                    &wildcards,
                    &declared,
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
    validate_extends(ast, &cx)?;

    Ok(Resolved {
        file_ns,
        item_aliases,
        ns_aliases,
        wildcards,
    })
}

/// Walks every `type` and `interface` declaration and resolves its
/// `extends` clauses to canonical FQNs. Surfaces unknown parents,
/// cycles in the extends graph, and field-type conflicts (across
/// parents, or between a parent and a child redeclaration).
fn validate_extends(ast: &ast::Source, cx: &CheckContext<'_>) -> Result<(), ParseError> {
    let declared = cx.declared;
    let file_ns = cx.file_ns;
    let item_aliases = cx.item_aliases;
    let ns_aliases = cx.ns_aliases;
    let wildcards = cx.wildcards;
    let source = cx.source;
    let file = cx.file;
    // Snapshot every parent/child as (decl_fqn, [parent_fqn]) so we
    // can walk the graph without distinguishing type vs interface.
    let mut parent_map: HashMap<Vec<String>, Vec<Vec<String>>> = HashMap::new();
    // Field map: decl_fqn -> [(field_name, TypeRef, span)].
    type FieldRow<'a> = (&'a str, &'a TypeRef, Span);
    let mut field_map: HashMap<Vec<String>, Vec<FieldRow<'_>>> = HashMap::new();

    let compose_fqn = |name: &[String]| -> Vec<String> {
        let mut v = file_ns.to_vec();
        v.extend(name.iter().cloned());
        v
    };

    for item in &ast.items {
        let (name, extends, fields, decl_span) = match item {
            ast::Item::TypeDecl(t) => (&t.name, &t.extends, &t.fields, t.span),
            ast::Item::InterfaceDecl(i) => (&i.name, &i.extends, &i.fields, i.span),
            _ => continue,
        };
        let self_fqn = compose_fqn(name);
        let mut resolved_parents = Vec::with_capacity(extends.len());
        for parent_path in extends {
            let Some(resolved) = resolve_path(
                parent_path,
                file_ns,
                item_aliases,
                ns_aliases,
                wildcards,
                declared,
            ) else {
                return Err(open_error(
                    source,
                    file,
                    format!(
                        "unknown extends target '{}' in '{}'",
                        parent_path.join("."),
                        self_fqn.join("."),
                    ),
                    decl_span,
                    "no such type or interface",
                ));
            };
            if resolved == self_fqn {
                return Err(open_error(
                    source,
                    file,
                    format!("'{}' cannot extend itself", self_fqn.join(".")),
                    decl_span,
                    "self-extension",
                ));
            }
            resolved_parents.push(resolved);
        }
        parent_map.insert(self_fqn.clone(), resolved_parents);
        let rows: Vec<FieldRow<'_>> = fields
            .iter()
            .map(|f| (f.name.as_str(), &f.ty, f.span))
            .collect();
        field_map.insert(self_fqn, rows);
    }

    // Cycle detection via DFS coloring.
    fn dfs_cycle(
        node: &[String],
        parent_map: &HashMap<Vec<String>, Vec<Vec<String>>>,
        color: &mut HashMap<Vec<String>, u8>, // 0 = unvisited, 1 = on-stack, 2 = done
    ) -> Option<Vec<String>> {
        let key = node.to_vec();
        match color.get(&key).copied().unwrap_or(0) {
            1 => return Some(key),
            2 => return None,
            _ => {}
        }
        color.insert(key.clone(), 1);
        if let Some(parents) = parent_map.get(&key) {
            for p in parents {
                if let Some(cycle) = dfs_cycle(p, parent_map, color) {
                    return Some(cycle);
                }
            }
        }
        color.insert(key, 2);
        None
    }
    let mut color: HashMap<Vec<String>, u8> = HashMap::new();
    for node in parent_map.keys() {
        if let Some(cycle_node) = dfs_cycle(node, &parent_map, &mut color) {
            return Err(open_error(
                source,
                file,
                format!("cyclic extends graph involving '{}'", cycle_node.join("."),),
                Span::new(0, 0),
                "cycle in extends",
            ));
        }
    }

    // Field-type compatibility: walk each declaration's transitive
    // ancestors, accumulate (name, TypeRef) pairs, and error on
    // conflicting types for the same name.
    fn collect_ancestor_fields<'a>(
        node: &[String],
        parent_map: &HashMap<Vec<String>, Vec<Vec<String>>>,
        field_map: &HashMap<Vec<String>, Vec<(&'a str, &'a TypeRef, Span)>>,
        out: &mut Vec<(&'a str, &'a TypeRef, Span)>,
        seen: &mut HashSet<Vec<String>>,
    ) {
        let key = node.to_vec();
        if !seen.insert(key.clone()) {
            return;
        }
        if let Some(parents) = parent_map.get(&key) {
            for p in parents {
                collect_ancestor_fields(p, parent_map, field_map, out, seen);
            }
        }
        if let Some(rows) = field_map.get(&key) {
            out.extend(rows.iter().copied());
        }
    }

    for decl_fqn in parent_map.keys() {
        let mut all_fields: Vec<FieldRow<'_>> = Vec::new();
        let mut seen: HashSet<Vec<String>> = HashSet::new();
        collect_ancestor_fields(
            decl_fqn,
            &parent_map,
            &field_map,
            &mut all_fields,
            &mut seen,
        );
        // Check pairwise. Function-typed fields are compared by
        // return type only — the same relaxation interface
        // conformance applies, so a concrete impl can narrow a
        // `fn(&I) -> R` declared on the parent to `fn(Concrete) -> R`.
        let mut by_name: HashMap<&str, (&TypeRef, Span)> = HashMap::new();
        for (name, ty, span) in &all_fields {
            match by_name.get(name) {
                Some((existing_ty, _)) if !extends_field_types_compatible(existing_ty, ty) => {
                    return Err(open_error(
                        source,
                        file,
                        format!(
                            "conflicting type for field '{}' in '{}' (across extends)",
                            name,
                            decl_fqn.join("."),
                        ),
                        *span,
                        "field type conflict",
                    ));
                }
                _ => {
                    by_name.insert(name, (ty, *span));
                }
            }
        }
    }

    Ok(())
}

/// `true` if two `TypeRef`s are compatible across an `extends`
/// boundary. Strict equality, except for function types which are
/// purely structural: the child may narrow a parent's
/// `fn(&Iface) -> R` to `fn(Concrete) -> Whatever`. Mirrors the
/// relaxation in `interfaces::iface_field_type_compatible`.
fn extends_field_types_compatible(a: &TypeRef, b: &TypeRef) -> bool {
    if matches!((a, b), (TypeRef::Function { .. }, TypeRef::Function { .. })) {
        return true;
    }
    a == b
}

/// Snapshot of the bookkeeping `validate_document` builds up before it
/// reaches its TypeRef / extends / connection passes. Threaded through
/// the deep validators so they can resolve paths against the document's
/// alias/wildcard tables without 11-arg signatures.
struct CheckContext<'a> {
    declared: &'a HashSet<Vec<String>>,
    interfaces: &'a HashSet<Vec<String>>,
    file_ns: &'a [String],
    item_aliases: &'a HashMap<String, Vec<String>>,
    ns_aliases: &'a HashMap<String, Vec<String>>,
    wildcards: &'a [Vec<String>],
    source: &'a str,
    file: &'a str,
}

fn check_type_ref(
    t: &TypeRef,
    ty_span: Span,
    parent_is_ref: bool,
    cx: &CheckContext<'_>,
) -> Result<(), ParseError> {
    match t {
        TypeRef::Builtin(_) => Ok(()),
        TypeRef::Named(path) => {
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
