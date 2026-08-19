//! The `extends` graph, checked once at open time.
//!
//! A `type` or `interface` may extend others, and the result has to be a
//! DAG whose fields agree: every parent resolves, nothing extends
//! itself, there are no cycles, and no two declarations in one ancestry
//! give the same field name incompatible types.
//!
//! This is the same relation [`inherit`](super::inherit) walks at
//! runtime to build a declaration's effective field set — checked here so
//! that walk can assume it terminates and that a field name means one
//! thing. Function-typed fields get the same structural relaxation
//! [`interfaces`](super::interfaces) applies, so the two paths agree
//! about what an override may narrow.

use std::collections::{HashMap, HashSet};

use crate::ast::{self, Span, TypeRef};
use crate::error::ParseError;

use crate::doc::validate::{open_error, resolve_path};

use super::declarations::CheckContext;

/// Walks every `type` and `interface` declaration and resolves its
/// `extends` clauses to canonical FQNs. Surfaces unknown parents,
/// cycles in the extends graph, and field-type conflicts (across
/// parents, or between a parent and a child redeclaration).
pub(super) fn validate_extends(ast: &ast::Source, cx: &CheckContext<'_>) -> Result<(), ParseError> {
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
///
/// "Equality" here ignores type arguments — they are metadata, so an
/// override that differs only in them is the same type. The interface
/// path compares `ResolvedType`s, which never carry arguments at all;
/// this keeps the two in agreement.
fn extends_field_types_compatible(a: &TypeRef, b: &TypeRef) -> bool {
    if matches!((a, b), (TypeRef::Function { .. }, TypeRef::Function { .. })) {
        return true;
    }
    a.same_ignoring_type_args(b)
}
