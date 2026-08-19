//! Connection statements: resolving their operands, and projecting them
//! into the records a `@connections` field gathers.
//!
//! An operand names a block by id, which is an ordinary name lookup —
//! except that resolving one can re-enter the very index being built, so
//! two thread-local guards break that cycle. See [`ConnOperandGuard`] and
//! [`BuildingConnIndexGuard`].

use std::collections::HashMap;

use crate::ast;
use crate::value::Value;

use super::cells::{ItemCellKind, ItemCells};
use super::imports::load_import_lazily;
use super::interfaces;
use super::scope::Scope;
use super::views::{ConnectionDecl, DeclName, TypeDecl, UnionDecl};
use super::{Document, match_block_first_label, match_block_id_field};

impl Document {
    /// Resolve a connection-statement operand (a bare identifier) by
    /// walking the scope chain looking for a block whose first label
    /// equals `name`. Falls back to the document root. Returns the
    /// label value plus the block kind so callers can dispatch.
    pub(crate) fn resolve_connection_operand(
        &self,
        scope: &Scope<'_>,
        name: &str,
    ) -> Option<ConnOperand> {
        // Identifying a block here means evaluating its first label / `id`
        // field. That evaluation must not re-enter `@connections`
        // projection (a block's identity can't depend on the connection
        // set) — see `RESOLVING_CONN_OPERAND` in `resolve_root`. Without
        // this guard, projection → operand resolution → label eval →
        // projection recurses until the stack overflows.
        let _guard = ConnOperandGuard::enter();
        // Innermost scope frames first.
        for frame in scope.frames().iter().rev() {
            if let ItemCellKind::Block { items: fcells, .. } = &frame.cells.kind
                && let Some(found) =
                    match_block_label_in_items(self, &frame.ast.items, fcells, frame.file_ns, name)
            {
                return Some(found);
            }
        }
        // Fall back to document root, served from a once-built index
        // (label/id value → block) that preserves the DFS first-match
        // order of the per-name walk it replaces.
        //
        // A re-entrant call *while the index is being built* (a label eval
        // looped back here) must not touch the index again — re-entering its
        // `OnceLock::get_or_init` would deadlock. The local frames above were
        // already consulted; give up on the global fallback for this call.
        if BUILDING_CONN_INDEX.with(std::cell::Cell::get) {
            return None;
        }
        self.conn_operand_index().get(name).cloned()
    }

    /// The root-scope operand index — see the `conn_operand_index`
    /// field. Built under the caller's `ConnOperandGuard`, so the
    /// label / `id` evaluations it runs can't re-enter `@connections`
    /// projection, exactly like the per-name walk did. The
    /// [`BuildingConnIndexGuard`] additionally stops a label eval that
    /// loops back into operand resolution from re-entering this
    /// `OnceLock::get_or_init` (which would deadlock).
    fn conn_operand_index(&self) -> &HashMap<String, ConnOperand> {
        self.conn_operand_index.get_or_init(|| {
            let _building = BuildingConnIndexGuard::enter();
            fn insert_identity(
                map: &mut HashMap<String, ConnOperand>,
                v: Value,
                b: &ast::Block,
                file_ns: &[String],
            ) {
                let (Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s)) = &v else {
                    return;
                };
                if !map.contains_key(s) {
                    map.insert(
                        s.clone(),
                        ConnOperand {
                            value: v.clone(),
                            kind: b.kind.clone(),
                            kind_ns: b.kind_ns.clone(),
                            file_ns: file_ns.to_vec(),
                        },
                    );
                }
            }
            fn walk(
                doc: &Document,
                items: &[ast::Item],
                cells: &[ItemCells],
                file_ns: &[String],
                map: &mut HashMap<String, ConnOperand>,
            ) {
                for (item, cell) in items.iter().zip(cells) {
                    match (item, &cell.kind) {
                        (ast::Item::Block(b), ItemCellKind::Block { items: bcells, .. }) => {
                            // Identity precedence per block: first label,
                            // then `id` field — mirroring
                            // `match_block_first_label` / `match_block_id_field`.
                            // Use `eval_literal` (not full resolution) so a
                            // bare-identifier label stays an opaque name in
                            // O(1); resolving every block's label as a
                            // reference made this index quadratic.
                            if let Some(first) = b.labels.first()
                                && let Ok(v) = doc.eval_literal(first)
                            {
                                insert_identity(map, v, b, file_ns);
                            }
                            for it in &b.items {
                                if let ast::Item::Field(f) = it
                                    && f.name == "id"
                                    && let Ok(v) = doc.eval_literal(&f.expr)
                                {
                                    insert_identity(map, v, b, file_ns);
                                }
                            }
                            walk(doc, &b.items, bcells, file_ns, map);
                        }
                        (
                            ast::Item::Import(_),
                            ItemCellKind::Import {
                                path,
                                system,
                                base_dir,
                                path_span,
                                loaded,
                            },
                        ) => {
                            let li = loaded.get_or_init(|| {
                                load_import_lazily(
                                    path,
                                    base_dir.as_deref(),
                                    *system,
                                    *path_span,
                                    doc.loader(),
                                )
                            });
                            if let Ok(li) = li {
                                walk(doc, &li.items, &li.cells, &li.file_ns, map);
                            }
                        }
                        _ => {}
                    }
                }
            }
            let mut map = HashMap::new();
            for src in self.all_sources() {
                walk(self, src.items, src.cells, src.file_ns, &mut map);
            }
            map
        })
    }

    /// The schema of the block a connection operand resolved to,
    /// looked up with the operand's own namespace context (the `::`
    /// qualifier at the instance site, and the namespace of the file
    /// the block lives in) — mirroring [`Block::schema`].
    pub(crate) fn operand_schema(&self, op: &ConnOperand) -> Option<TypeDecl<'_>> {
        self.block_schema_in(&op.kind_ns, &op.kind, &op.file_ns)
            .or_else(|| self.table_schema_in(&op.kind_ns, &op.kind, &op.file_ns))
    }

    /// Project sibling `Item::Connection` statements through a
    /// `@connections(SchemaName)` decorator: gather every statement
    /// whose `(lhs_type, rhs_type)` matches the schema and produce a
    /// `Value::Record` per match.
    pub(crate) fn project_connections(
        &self,
        items: &[ast::Item],
        schema: ConnectionDecl<'_>,
        scope: &Scope<'_>,
    ) -> Vec<Value> {
        let mut out: Vec<Value> = Vec::new();
        // Endpoint types resolve relative to the file that declared the
        // connection, so a namespaced library's bare `Adr` means its own
        // `lib.Adr`.
        let source_fqn = self.resolve_type_fqn_in(schema.source_type(), schema.file_ns());
        let dest_fqn = self.resolve_type_fqn_in(schema.destination_type(), schema.file_ns());
        for item in items {
            let ast::Item::Connection(stmt) = item else {
                continue;
            };
            let Some(record) = self.build_connection_record(
                stmt,
                schema,
                scope,
                source_fqn.as_deref(),
                dest_fqn.as_deref(),
            ) else {
                continue;
            };
            out.push(record);
        }
        out
    }

    /// Project one connection statement into the record shape its
    /// `@connections` schema declares.
    fn build_connection_record(
        &self,
        stmt: &ast::ConnectionStmt,
        schema: ConnectionDecl<'_>,
        scope: &Scope<'_>,
        source_fqn: Option<&str>,
        dest_fqn: Option<&str>,
    ) -> Option<Value> {
        let lhs = self.resolve_connection_operand(scope, &stmt.lhs);
        let rhs = self.resolve_connection_operand(scope, &stmt.rhs);

        // Does each *resolved* operand's block type satisfy this schema's
        // source / destination role? (An unresolved operand has no AST
        // block, so it can't be type-checked — see the dynamic path below.)
        let lhs_type_ok = matches!(&lhs, Some(op)
            if self.operand_schema(op).is_some_and(|d| connection_type_matches(self, &d, source_fqn)));
        let rhs_type_ok = matches!(&rhs, Some(op)
            if self.operand_schema(op).is_some_and(|d| connection_type_matches(self, &d, dest_fqn)));

        if lhs.is_some() && rhs.is_some() {
            // Both operands name a literal block — strict path, unchanged:
            // both must type-match for this schema to claim the statement.
            if !(lhs_type_ok && rhs_type_ok) {
                return None;
            }
        } else {
            // At least one operand didn't resolve to a literal block — e.g.
            // an id GENERATED by a `@contextual` block's expansion. Only a
            // `@dynamic` connection accepts that; it emits the raw operand
            // string so a downstream consumer can match it against the
            // generated shape ids. The resolved side(s) must still type-match
            // so we don't claim a statement meant for a different schema.
            if !schema.is_dynamic() {
                return None;
            }
            if (lhs.is_some() && !lhs_type_ok) || (rhs.is_some() && !rhs_type_ok) {
                return None;
            }
        }

        let kind_name = match &stmt.kind {
            Some(s) => s.clone(),
            None => schema.default_kind()?,
        };
        let lhs_val = lhs
            .map(|op| op.value)
            .unwrap_or_else(|| Value::Identifier(stmt.lhs.clone()));
        let rhs_val = rhs
            .map(|op| op.value)
            .unwrap_or_else(|| Value::Identifier(stmt.rhs.clone()));
        let mut fields: std::collections::BTreeMap<String, Value> =
            std::collections::BTreeMap::new();
        fields.insert("source".to_string(), lhs_val);
        fields.insert("destination".to_string(), rhs_val);
        fields.insert("kind".to_string(), Value::Symbol(kind_name));
        Some(Value::Record {
            ty: schema.ast.name.clone(),
            fields: std::sync::Arc::new(fields),
        })
    }
}

/// A connection-statement operand resolved to a literal block: its
/// identifying value plus enough namespace context to look up the
/// block's schema the way [`Block::schema`] would.
#[derive(Clone)]
pub(crate) struct ConnOperand {
    /// The identifying label / `id` value.
    pub(crate) value: Value,
    /// The block's kind as written (`ast::Block.kind`).
    pub(crate) kind: String,
    /// The `::` namespace qualifier at the instance site (empty for a
    /// bare kind).
    pub(crate) kind_ns: Vec<String>,
    /// Namespace of the file the matched block lives in.
    pub(crate) file_ns: Vec<String>,
}

/// Recursively search a slice of items for an `Item::Block` that
/// identifies as `name`. `file_ns` is the namespace of the file the
/// items live in. Identity sources, in priority order:
///
///   1. the block's first label (evaluated as a literal)
///   2. a field named `id` whose value evaluates to the name
///   3. nested blocks reachable through the block's own `items`
///
/// Returns the identifying value plus the block's kind and namespace
/// context so callers can dispatch on the block's declared type.
fn match_block_label_in_items(
    doc: &Document,
    items: &[ast::Item],
    cells: &[ItemCells],
    file_ns: &[String],
    name: &str,
) -> Option<ConnOperand> {
    let operand = |value: Value, b: &ast::Block| ConnOperand {
        value,
        kind: b.kind.clone(),
        kind_ns: b.kind_ns.clone(),
        file_ns: file_ns.to_vec(),
    };
    for (item, cell) in items.iter().zip(cells) {
        match (item, &cell.kind) {
            (ast::Item::Block(b), ItemCellKind::Block { items: bcells, .. }) => {
                if let Some(v) = match_block_first_label(doc, b, name) {
                    return Some(operand(v, b));
                }
                if let Some(v) = match_block_id_field(doc, b, name) {
                    return Some(operand(v, b));
                }
                // Nested blocks live in the same file as their parent.
                if let Some(found) =
                    match_block_label_in_items(doc, &b.items, bcells, file_ns, name)
                {
                    return Some(found);
                }
            }
            // An in-block `import` splices its top-level blocks into this
            // scope, so a connection endpoint can name a block that lives
            // in the imported fragment. Force the lazy load and recurse
            // with the imported file's own namespace.
            (
                ast::Item::Import(_),
                ItemCellKind::Import {
                    path,
                    system,
                    base_dir,
                    path_span,
                    loaded,
                },
            ) => {
                let li = loaded.get_or_init(|| {
                    load_import_lazily(path, base_dir.as_deref(), *system, *path_span, doc.loader())
                });
                if let Ok(li) = li
                    && let Some(found) =
                        match_block_label_in_items(doc, &li.items, &li.cells, &li.file_ns, name)
                {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

/// `true` when a concrete block's [`TypeDecl`] satisfies a connection
/// schema's declared source or destination FQN. Resolution is
/// polymorphic, in priority order:
///
/// 1. **Nominal** — direct FQN equality, or `decl` is an `extends`
///    descendant of `target` (a connection declared against a supertype
///    admits any conforming subtype).
/// 2. **Interface endpoint** (`connection Rel: &Iface -> …`) — `target`
///    names an interface that `decl`'s block type implements. One
///    `&Entity -> &Entity` connection then spans every entity pair.
/// 3. **Union endpoint** (`connection Rel: SomeUnion -> …`) — `target`
///    names a union one of whose variants `decl` satisfies (a `: Type`
///    body it is/extends, or an `: &Iface` body it implements).
pub(crate) fn connection_type_matches(
    doc: &Document,
    decl: &TypeDecl<'_>,
    target_fqn: Option<&str>,
) -> bool {
    let Some(target) = target_fqn else {
        return false;
    };
    if decl.full_name() == target || decl.is_descendant_of(target) {
        return true;
    }
    // Interface endpoint: the operand's concrete block type implements it.
    if let Some(iface) = doc.interface(target)
        && interfaces::check_interface_conformance(doc, &iface, decl, crate::ast::Span::new(0, 0))
            .is_ok()
    {
        return true;
    }
    // Union endpoint: the operand's concrete type satisfies a variant.
    if let Some(union) = doc.union_decl(target)
        && union_admits_type(doc, &union, decl)
    {
        return true;
    }
    false
}

/// `true` when concrete block type `decl` satisfies any variant of
/// `union` — a `: Type` variant body that `decl` is or extends, or an
/// `: &Iface` variant body that `decl` implements. Record / unit variant
/// bodies carry no block type, so they never admit a connection operand.
fn union_admits_type(doc: &Document, union: &UnionDecl<'_>, decl: &TypeDecl<'_>) -> bool {
    let Ok(variants) = doc.effective_variants_of(union.ast) else {
        return false;
    };
    let ns = union.file_ns();
    for v in variants {
        match &v.body {
            crate::ast::VariantBody::TypeRef { ty, .. } => {
                if let Some(fqn) = doc.resolve_type_fqn_in(ty, ns)
                    && (decl.full_name() == fqn || decl.is_descendant_of(&fqn))
                {
                    return true;
                }
            }
            crate::ast::VariantBody::InterfaceRef { iface, .. } => {
                if let Some(fqn) = doc.resolve_path_in(iface, ns).map(|p| p.join("."))
                    && let Some(iface_decl) = doc.interface(&fqn)
                    && interfaces::check_interface_conformance(
                        doc,
                        &iface_decl,
                        decl,
                        crate::ast::Span::new(0, 0),
                    )
                    .is_ok()
                {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// RAII guard that sets [`RESOLVING_CONN_OPERAND`] for its lifetime and
/// restores the previous value on drop (so nested operand resolution
/// stays correct).
struct ConnOperandGuard(bool);

/// RAII guard that marks the root operand index as being built — see
/// [`BUILDING_CONN_INDEX`].
struct BuildingConnIndexGuard(bool);

thread_local! {
    /// Set while a connection operand's identifying block label is being
    /// evaluated. `Document::resolve_root` consults it to suppress
    /// `@connections` projection during that window, breaking what would
    /// otherwise be unbounded recursion (operand → label eval → projection
    /// → operand → …).
    pub(super) static RESOLVING_CONN_OPERAND: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };

    /// Set while the root operand index (`conn_operand_index`) is being
    /// built. The build evaluates every block's identifying label, and a
    /// label eval can loop back into `resolve_connection_operand`; without
    /// this flag that re-entrant call would re-enter the index's
    /// `OnceLock::get_or_init` and deadlock. While set, operand resolution
    /// skips the global index (local scope frames are still consulted).
    static BUILDING_CONN_INDEX: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

impl ConnOperandGuard {
    /// Set the flag, remembering the previous value for `Drop`.
    fn enter() -> Self {
        let prev = RESOLVING_CONN_OPERAND.with(|f| f.replace(true));
        Self(prev)
    }
}

impl Drop for ConnOperandGuard {
    fn drop(&mut self) {
        RESOLVING_CONN_OPERAND.with(|f| f.set(self.0));
    }
}

impl BuildingConnIndexGuard {
    /// Set the flag, remembering the previous value for `Drop`.
    fn enter() -> Self {
        let prev = BUILDING_CONN_INDEX.with(|f| f.replace(true));
        Self(prev)
    }
}

impl Drop for BuildingConnIndexGuard {
    fn drop(&mut self) {
        BUILDING_CONN_INDEX.with(|f| f.set(self.0));
    }
}
