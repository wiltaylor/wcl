//! The `extends` graph, checked once at open time.
//!
//! A `type` or `interface` may extend others, and the result has to be a
//! DAG whose fields agree: every parent resolves, nothing extends
//! itself, there are no cycles, no two declarations in one ancestry
//! give the same field name incompatible types, and no two differently
//! named fields of the effective set claim the same `@inline(N)` label
//! slot.
//!
//! The graph is the **whole document's**: the root file's declarations,
//! every eagerly-imported file's (transitively) and the environment's
//! synthetic types. A parent written in another file is checked exactly
//! like one in the same file, and a chain may cross files at every level.
//! Parents resolve the way [`inherit`](super::inherit)'s `lookup_parent`
//! resolves them at runtime: in the declaring file's namespace first, then
//! the document's aliases and wildcards. Only the root file's unknown
//! parents are reported; an imported file's unresolvable parent is
//! skipped, as the runtime walk skips it.
//!
//! This is the same relation [`inherit`](super::inherit) walks at
//! runtime to build a declaration's effective field set — checked here so
//! that walk can assume it terminates and that a field name means one
//! thing. Function-typed fields get the same structural relaxation
//! [`interfaces`](super::interfaces) applies, so the two paths agree
//! about what an override may narrow.

use std::collections::{HashMap, HashSet};

use crate::ast::{self, Span, TypeRef};
use crate::diagnostics::ParseError;

use crate::doc::cells::LoadedImport;
use crate::doc::validate::{open_error, resolve_path};

use super::declarations::CheckContext;

/// A file the graph's declarations come from, for rendering a
/// diagnostic against the right text.
struct Origin<'a> {
    /// Source text.
    source: &'a str,
    /// Name diagnostics print for it.
    file: String,
}

/// One field a declaration writes: name, type, span, its literal
/// `@inline` slot, and the index of the declaring [`Node`].
type FieldRow<'a> = (&'a str, &'a TypeRef, Span, Option<u64>, usize);

/// One `type` or `interface` declaration in the document-wide graph.
struct Node<'a> {
    /// Fully-qualified name.
    fqn: Vec<String>,
    /// The declaration's span, in its origin file.
    span: Span,
    /// Index into [`Graph::origins`], or `None` for a synthetic type,
    /// which has no source text.
    origin: Option<usize>,
    /// Resolved parents, in `extends` order.
    parents: Vec<usize>,
    /// Fields written on the declaration itself.
    fields: Vec<FieldRow<'a>>,
}

/// The document-wide `extends` graph.
struct Graph<'a> {
    /// Every file a declaration came from; index 0 is the root.
    origins: Vec<Origin<'a>>,
    /// Every declaration: root first, then imports in load order, then
    /// synthetic types.
    nodes: Vec<Node<'a>>,
    /// FQN → node index. The first declaration of a name wins, as in the
    /// runtime lookup.
    by_fqn: HashMap<Vec<String>, usize>,
    /// Per node, its written parent paths and the namespace they were
    /// written in. Resolved once every node is known.
    pending: Vec<(&'a [Vec<String>], &'a [String])>,
}

impl<'a> Graph<'a> {
    /// Add one declaration, unless an earlier one already owns its name.
    fn add(
        &mut self,
        fqn: Vec<String>,
        span: Span,
        origin: Option<usize>,
        decl: (&'a [Vec<String>], &'a [ast::TypeField]),
        file_ns: &'a [String],
    ) {
        if self.by_fqn.contains_key(&fqn) {
            return;
        }
        let (extends, fields) = decl;
        let idx = self.nodes.len();
        self.by_fqn.insert(fqn.clone(), idx);
        let fields = fields
            .iter()
            .map(|f| (f.name.as_str(), &f.ty, f.span, literal_inline_slot(f), idx))
            .collect();
        self.nodes.push(Node {
            fqn,
            span,
            origin,
            parents: Vec::new(),
            fields,
        });
        self.pending.push((extends, file_ns));
    }

    /// Add every `type` and `interface` an item list declares.
    fn add_items(&mut self, items: &'a [ast::Item], file_ns: &'a [String], origin: usize) {
        for item in items {
            let (name, extends, fields, span) = match item {
                ast::Item::TypeDecl(t) => (&t.name, &t.extends, &t.fields, t.span),
                ast::Item::InterfaceDecl(i) => (&i.name, &i.extends, &i.fields, i.span),
                _ => continue,
            };
            let fqn = file_ns.iter().chain(name.iter()).cloned().collect();
            self.add(fqn, span, Some(origin), (extends, fields), file_ns);
        }
    }

    /// Add every eagerly-imported file, depth first in load order — the
    /// order the document searches its sources in.
    fn add_imports(&mut self, imports: &'a [LoadedImport]) {
        for imp in imports {
            let origin = self.origins.len();
            self.origins.push(Origin {
                source: &imp.source,
                file: imp.path.display().to_string(),
            });
            self.add_items(&imp.items, &imp.file_ns, origin);
            self.add_imports(&imp.eager_imports);
        }
    }

    /// An open-time error raised against `node`'s file. A synthetic type
    /// has no source text, so its errors render against the root file at
    /// its start.
    fn error(&self, node: usize, message: String, span: Span, label: &str) -> ParseError {
        let (origin, span) = match self.nodes[node].origin {
            Some(i) => (&self.origins[i], span),
            None => (&self.origins[0], Span::new(0, 0)),
        };
        open_error(origin.source, &origin.file, message, span, label)
    }

    /// `"; 'T' in <file> <what>"` when `there` was declared in a different
    /// file than `here`, the file the error renders against — so a
    /// cross-file diagnostic names both ends. Empty within one file.
    fn elsewhere(&self, here: usize, there: usize, what: &str) -> String {
        let there_origin = self.nodes[there].origin;
        if self.nodes[here].origin == there_origin {
            return String::new();
        }
        let file = match there_origin {
            Some(i) => self.origins[i].file.as_str(),
            None => "the host environment",
        };
        format!("; '{}' in {file} {what}", self.nodes[there].fqn.join("."))
    }
}

/// Walks every `type` and `interface` declaration in the document — the
/// root file's, every eager import's and the synthetic ones — and resolves
/// its `extends` clauses to canonical FQNs. Surfaces unknown parents (in
/// the root file), self-extension, cycles in the extends graph, field-type
/// conflicts (across parents, or between a parent and a child
/// redeclaration) and `@inline` slot clashes, wherever the declarations
/// involved were written.
pub(super) fn validate_extends(ast: &ast::Source, cx: &CheckContext<'_>) -> Result<(), ParseError> {
    let file_ns = cx.file_ns;
    let (source, file) = (cx.source, cx.file);
    let resolve = |path: &[String], ns: &[String]| {
        resolve_path(
            path,
            ns,
            cx.item_aliases,
            cx.ns_aliases,
            cx.wildcards,
            cx.declared,
        )
    };

    // The root file's parents must all resolve, and none may be the
    // declaration itself. Reported against the declaration that writes
    // the path.
    for item in &ast.items {
        let (name, extends, decl_span) = match item {
            ast::Item::TypeDecl(t) => (&t.name, &t.extends, t.span),
            ast::Item::InterfaceDecl(i) => (&i.name, &i.extends, i.span),
            _ => continue,
        };
        let self_fqn: Vec<String> = file_ns.iter().chain(name.iter()).cloned().collect();
        for parent_path in extends {
            let Some(resolved) = resolve(parent_path, file_ns) else {
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
        }
    }

    // The document-wide graph: root, then imports, then synthetic types.
    let mut graph = Graph {
        origins: vec![Origin {
            source,
            file: file.to_string(),
        }],
        nodes: Vec::new(),
        by_fqn: HashMap::new(),
        pending: Vec::new(),
    };
    graph.add_items(&ast.items, file_ns, 0);
    graph.add_imports(cx.imports);
    for t in cx.synthetic {
        graph.add(t.name.clone(), t.span, None, (&t.extends, &t.fields), &[]);
    }
    let pending = std::mem::take(&mut graph.pending);
    for (idx, (paths, ns)) in pending.into_iter().enumerate() {
        let parents = paths
            .iter()
            .filter_map(|p| resolve(p, ns))
            .filter_map(|fqn| graph.by_fqn.get(&fqn).copied())
            .collect();
        graph.nodes[idx].parents = parents;
    }

    // Cycle detection via DFS colouring. The post-order it records is the
    // order the field checks run in: every ancestor before its
    // descendants, so a conflict is reported on the declaration that
    // introduces it rather than on whichever descendant is reached first.
    fn visit(
        node: usize,
        nodes: &[Node<'_>],
        color: &mut [u8], // 0 = unvisited, 1 = on-stack, 2 = done
        order: &mut Vec<usize>,
    ) -> Option<usize> {
        match color[node] {
            1 => return Some(node),
            2 => return None,
            _ => {}
        }
        color[node] = 1;
        for &p in &nodes[node].parents {
            if let Some(cycle) = visit(p, nodes, color, order) {
                return Some(cycle);
            }
        }
        color[node] = 2;
        order.push(node);
        None
    }
    let mut color = vec![0u8; graph.nodes.len()];
    let mut order: Vec<usize> = Vec::with_capacity(graph.nodes.len());
    for start in 0..graph.nodes.len() {
        if let Some(at) = visit(start, &graph.nodes, &mut color, &mut order) {
            let node = &graph.nodes[at];
            let name = node.fqn.join(".");
            // The root file's self-extension was caught above; this is an
            // imported file's.
            let (message, label) = if node.parents.contains(&at) {
                (format!("'{name}' cannot extend itself"), "self-extension")
            } else {
                (
                    format!("cyclic extends graph involving '{name}'"),
                    "cycle in extends",
                )
            };
            return Err(graph.error(at, message, node.span, label));
        }
    }

    // Field-type compatibility: walk each declaration's transitive
    // ancestors, accumulate (name, TypeRef) pairs, and error on
    // conflicting types for the same name.
    fn collect_ancestor_fields<'a>(
        node: usize,
        nodes: &[Node<'a>],
        out: &mut Vec<FieldRow<'a>>,
        seen: &mut HashSet<usize>,
    ) {
        if !seen.insert(node) {
            return;
        }
        for &p in &nodes[node].parents {
            collect_ancestor_fields(p, nodes, out, seen);
        }
        out.extend(nodes[node].fields.iter().copied());
    }

    for decl in order {
        let decl_name = graph.nodes[decl].fqn.join(".");
        let mut all_fields: Vec<FieldRow<'_>> = Vec::new();
        let mut seen: HashSet<usize> = HashSet::new();
        collect_ancestor_fields(decl, &graph.nodes, &mut all_fields, &mut seen);
        // Check pairwise. Function-typed fields are compared by
        // return type only — the same relaxation interface
        // conformance applies, so a concrete impl can narrow a
        // `fn(&I) -> R` declared on the parent to `fn(Concrete) -> R`.
        let mut by_name: HashMap<&str, (&TypeRef, usize)> = HashMap::new();
        for &(name, ty, span, _, owner) in &all_fields {
            match by_name.get(name) {
                Some(&(existing, first)) if !extends_field_types_compatible(existing, ty) => {
                    let also = graph.elsewhere(owner, first, &format!("declares it as {existing}"));
                    return Err(graph.error(
                        owner,
                        format!(
                            "conflicting type for field '{name}' in '{decl_name}' (across extends){also}"
                        ),
                        span,
                        "field type conflict",
                    ));
                }
                _ => {
                    by_name.insert(name, (ty, owner));
                }
            }
        }

        // `@inline(N)` slots across the effective set. A redeclaration
        // replaces the inherited field, slot and all, so resolve by name
        // first — rows arrive ancestors-first, so the child-most
        // declaration (and, between parents, the later one) is the one
        // kept, as in `inherit::build_effective_fields`. Two different
        // names left on one slot would both read the same label.
        let mut slot_of: Vec<(&str, Option<u64>, Span, usize)> = Vec::new();
        for &(name, _, span, slot, owner) in &all_fields {
            match slot_of.iter_mut().find(|(n, ..)| *n == name) {
                Some(entry) => *entry = (name, slot, span, owner),
                None => slot_of.push((name, slot, span, owner)),
            }
        }
        let mut claimed: HashMap<u64, (&str, usize)> = HashMap::new();
        for &(name, slot, span, owner) in &slot_of {
            let Some(slot) = slot else { continue };
            if let Some((first, first_owner)) = claimed.insert(slot, (name, owner)) {
                let also = graph.elsewhere(owner, first_owner, &format!("declares '{first}'"));
                return Err(graph.error(
                    owner,
                    format!(
                        "fields '{first}' and '{name}' of '{decl_name}' both claim @inline({slot}){also}"
                    ),
                    span,
                    "inline slot already taken",
                ));
            }
        }
    }

    Ok(())
}

/// The label slot a field's `@inline(N)` names, when `N` is written as an
/// integer literal — the only form the open-time slot check can read
/// without evaluating anything.
fn literal_inline_slot(field: &ast::TypeField) -> Option<u64> {
    let decorator = field
        .decorators
        .iter()
        .find(|d| d.name.len() == 1 && d.name[0] == "inline")?;
    match decorator.positional.first()? {
        ast::Expr::U64(n) => Some(*n),
        ast::Expr::I64(n) => u64::try_from(*n).ok(),
        _ => None,
    }
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
