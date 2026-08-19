//! Static document validation: runs once at `Document::open` time.
//!
//! Responsibilities:
//! - Namespace position / uniqueness.
//! - `use` declarations → alias tables (single-item, namespace, wildcard).
//! - Path resolution against those tables ([`resolve_path`]), which every
//!   later name lookup in the document is built on.
//!
//! On success returns a [`Resolved`] bundle (file namespace + alias tables)
//! which the document then stashes for runtime name resolution.
//!
//! What the declarations themselves must satisfy — type references that
//! resolve, unique variant and symbol names, a sound `extends` graph — is
//! the type system's business, and is checked from here by
//! [`types::declarations`](super::types) once the alias tables are built.

use std::collections::{HashMap, HashSet};

use miette::NamedSource;

use crate::ast::{self, Span};
use crate::diagnostics::ParseError;
use crate::symbols::{SymbolIndex, SymbolKind};

use super::span_to_miette;
use super::types::declarations::{CheckContext, check_declarations};

#[derive(Debug, PartialEq, Eq)]
/// What a source's `namespace` and `use` declarations resolved to.
pub(crate) struct Resolved {
    /// Namespace the source declares.
    pub(crate) file_ns: Vec<String>,
    /// Aliases binding one item name to a full path.
    pub(crate) item_aliases: HashMap<String, Vec<String>>,
    /// Aliases binding a namespace prefix to a full path.
    pub(crate) ns_aliases: HashMap<String, Vec<String>>,
    /// Namespaces pulled in wholesale.
    pub(crate) wildcards: Vec<Vec<String>>,
}

/// Whether a declared name equals a target path, segment by segment.
pub(crate) fn decl_fqn_matches(decl: &[String], target: &[&str]) -> bool {
    decl.len() == target.len() && decl.iter().zip(target.iter()).all(|(a, b)| a == b)
}

/// Resolve a written path to its fully-qualified form, following the
/// source's aliases and wildcards.
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

/// Build the `ParseError` for a validation failure found while
/// opening a document.
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

/// Run the parse-time validation pass: namespace and `use`
/// resolution, plus the type-reference checks that do not need
/// evaluation.
pub(crate) fn validate_document(
    ast: &ast::Source,
    symbols: &SymbolIndex,
    synthetic: (&[ast::TypeDecl], &[ast::SymbolSetDecl]),
    imported_symbols: &[&SymbolIndex],
    import_namespaces: &[Vec<String>],
    source: &str,
    file: &str,
) -> Result<Resolved, ParseError> {
    let (synthetic_types, synthetic_symbol_sets) = synthetic;
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
    for t in synthetic_types {
        let fqn = t.name.clone();
        declared.insert(fqn.clone());
        for n in 1..fqn.len() {
            prefixes.insert(fqn[..n].to_vec());
        }
    }
    for set in synthetic_symbol_sets {
        let fqn = set.name.clone();
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
            // A registry-injected (synthetic) declaration already owns this FQN.
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
    // `lower` returning `list<Svg>` where the union lives in
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
    // `list<Svg>`) resolve to `wdoc.Svg` without an
    // explicit `use wdoc`.
    for ns in import_namespaces {
        if !ns.is_empty() && !wildcards.contains(ns) {
            wildcards.push(ns.clone());
        }
    }

    // 4. Declaration checking: type references, variant / symbol
    // uniqueness, connections and the `extends` graph — all of which
    // are the type system's rules, so they live in `types::declarations`.
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
    check_declarations(ast, symbols, &cx)?;

    Ok(Resolved {
        file_ns,
        item_aliases,
        ns_aliases,
        wildcards,
    })
}
