//! Views over block instances, and the row-shaped things built from
//! them.
//!
//! [`Block`] is the largest view in the crate, because a block is where
//! most of the language happens: labels, nested children, gathered
//! fields, table rows, slot fills and `@contextual` expansion all hang
//! off it.

use super::decl::synth_child_from_value;
use super::decorator::iter_decorators;
use super::*;

/// Public view of an `lhs -> rhs [:sym]` connection statement.
#[derive(Debug, Clone, Copy)]
pub struct Connection<'a> {
    /// The AST node this view borrows.
    pub(in crate::doc) ast: &'a ast::ConnectionStmt,
}

impl<'a> Connection<'a> {
    /// Id of the block on the left of the statement.
    pub fn source(&self) -> &'a str {
        &self.ast.lhs
    }

    /// Id of the block on the right of the statement.
    pub fn destination(&self) -> &'a str {
        &self.ast.rhs
    }

    /// Explicit `:kind` symbol if present, or `None` when the writer
    /// relied on the connection schema's default symbol.
    pub fn kind(&self) -> Option<&'a str> {
        self.ast.kind.as_deref()
    }

    /// Source span of this node in the file that declares it.
    pub fn span(&self) -> Span {
        self.ast.span
    }
}

/// When a `DataRef` has a statically-known concrete type (because
/// it's a `Block` whose kind has a `@block`/`@table` schema, or a
/// `Field` whose declared type is a named type), return that
/// `TypeDecl`. Otherwise `None`.
#[derive(Clone)]
pub struct Block<'a> {
    /// The AST node this view borrows.
    pub(in crate::doc) ast: &'a ast::Block,
    /// Lazily-evaluated caches for this item's decorators and fields.
    pub(in crate::doc) cells: &'a ItemCells,
    /// The document these views read through.
    pub(in crate::doc) doc: &'a Document,
    /// Namespace declared by the file this block instance lexically
    /// lives in — the root document or the import that carries it.
    /// Synthesised blocks (table rows, computed-children splices,
    /// component bodies) inherit their owning declaration's. Bare-kind
    /// schema resolution prefers this namespace (see [`Block::schema`]).
    pub(in crate::doc) file_ns: &'a [String],
    /// When `Some`, overrides `ast.kind` for views derived from a
    /// synthesised row-Block (its stored `kind` is blank). Real
    /// blocks always have `None`.
    pub(in crate::doc) kind_override: Option<&'a str>,
    /// Lexical scope chain — outermost first, **excluding** this
    /// block. To get the scope a child expression sees from inside
    /// this block, push this block's frame: `self.scope.push(self_frame)`.
    pub(in crate::doc) scope: Scope<'a>,
}

impl<'a> Block<'a> {
    /// The block's label cache and per-item cells. Panics if the view was
    /// built over a non-block cell, which the constructors make
    /// unreachable.
    fn block_inner(&self) -> (&'a OnceLock<Result<Vec<Value>, EvalError>>, &'a [ItemCells]) {
        let ItemCellKind::Block { labels, items, .. } = &self.cells.kind else {
            unreachable!("Block view wraps a Block cell")
        };
        (labels, items)
    }

    /// Decorators attached to this item, in source order.
    pub fn decorators(&self) -> impl Iterator<Item = Decorator<'a>> + 'a {
        iter_decorators(
            &self.ast.decorators,
            &self.cells.decorators,
            self.doc,
            self.file_ns,
        )
    }

    /// The block's kind, unqualified.
    pub fn kind(&self) -> &'a str {
        self.kind_override.unwrap_or(&self.ast.kind)
    }

    /// Whether this block was authored as a conditional bare-name fill
    /// (`aside? { ... }`). Hosts interpret conditionality against their
    /// resolved slot contract.
    pub fn is_conditional(&self) -> bool {
        self.ast.conditional
    }

    /// Declared type of a `slot name: Type` block.
    pub fn slot_type_ref(&self) -> Option<&'a TypeRef> {
        self.ast.slot_decl.as_ref().map(|slot| &slot.ty)
    }

    /// `true` for a declaration carrying `?` after its type.
    pub fn slot_optional(&self) -> bool {
        self.ast
            .slot_decl
            .as_ref()
            .is_some_and(|slot| slot.optional)
    }

    /// `true` for a declaration carrying `*` after its type.
    pub fn slot_repeated(&self) -> bool {
        self.ast
            .slot_decl
            .as_ref()
            .is_some_and(|slot| slot.repeated)
    }

    /// The document this block belongs to. Lets host renderers reach
    /// document-level lookups (e.g. `kind_declarer`) from a block view.
    pub fn doc(&self) -> &'a Document {
        self.doc
    }

    /// The miette source (name + text) of the file that declares this
    /// block — the root document or the imported file it lives in. Hosts
    /// use this to render an eval diagnostic raised while processing this
    /// block against the correct file's snippet (a cross-file span won't
    /// line up with the root source's text). Falls back to the root
    /// source for synthesised blocks not backed by on-disk AST.
    pub fn named_source(&self) -> miette::NamedSource<String> {
        self.doc.named_source_for_block(self.ast)
    }

    /// Scope that child expressions inside this block see — the
    /// block's own `scope` extended with one frame for itself.
    /// Read a memoised `typed_field` projection for this block's cells.
    /// Keyed by (kind, field name) so a synth-row cell viewed under a
    /// `kind_override` can't collide with another view of the same cell.
    fn typed_proj_memo_get(&self, name: &str) -> Option<Value> {
        let ItemCellKind::Block {
            typed_proj_memo, ..
        } = &self.cells.kind
        else {
            return None;
        };
        let memo = typed_proj_memo.read().ok()?;
        memo.get(&(self.kind().to_string(), name.to_string()))
            .cloned()
    }

    /// Store a `typed_field` projection in this block's cells.
    fn typed_proj_memo_insert(&self, name: &str, value: Value) {
        let ItemCellKind::Block {
            typed_proj_memo, ..
        } = &self.cells.kind
        else {
            return;
        };
        if let Ok(mut memo) = typed_proj_memo.write() {
            memo.insert((self.kind().to_string(), name.to_string()), value);
        }
    }

    /// `true` when this block's scope frame could bind `name`: a let /
    /// field / nested block kind / table header in its realized sources
    /// (own items + in-block imports), or a field on its schema
    /// (including extends-inherited ones, which `typed_field` projects).
    /// Over-approximate; built once per cell per viewed kind so
    /// `scope_lookup` can skip a frame's per-item scans on a miss.
    pub(crate) fn can_bind_name(&self, name: &str) -> bool {
        let ItemCellKind::Block { bindable_names, .. } = &self.cells.kind else {
            return true; // not a block cell — stay conservative
        };
        if let Ok(memo) = bindable_names.read()
            && let Some(set) = memo.get(self.kind())
        {
            return set.contains(name);
        }
        let mut set = std::collections::HashSet::new();
        for src in self.realize_and_sources() {
            for item in src.items {
                match item {
                    ast::Item::Let(l) => {
                        set.insert(l.name.clone());
                    }
                    ast::Item::Field(f) => {
                        set.insert(f.name.clone());
                    }
                    ast::Item::Block(b) => {
                        set.insert(b.kind.clone());
                    }
                    ast::Item::Table(t) => {
                        set.insert(t.field_name.clone());
                    }
                    _ => {}
                }
            }
        }
        if let Some(schema) = self.schema() {
            for f in schema.effective_fields() {
                set.insert(f.name().to_string());
            }
        }
        let contains = set.contains(name);
        if let Ok(mut memo) = bindable_names.write() {
            memo.insert(self.kind().to_string(), std::sync::Arc::new(set));
        }
        contains
    }

    /// The scope this block's own items evaluate in: the surrounding
    /// scope with a frame for this block pushed on, so `self` and the
    /// block's `let` bindings resolve.
    pub(crate) fn child_scope(&self) -> Scope<'a> {
        self.scope.push(ScopeFrame {
            ast: self.ast,
            cells: self.cells,
            file_ns: self.file_ns,
            kind_override: self.kind_override,
            bindings: None,
            content: None,
            expansion_depth: 0,
        })
    }

    /// How deep `@contextual` expansion currently is at this block.
    /// [`Block::expand_children`] caps on this to stop a self-referential
    /// expansion from growing forever (iteration count doesn't inflate
    /// it: all elements of one repetition share a depth).
    ///
    /// This is the max of the frames' **dynamic** `expansion_depth`, not a
    /// count of binding frames: an instantiated body's expansion scope is
    /// rebuilt from the *declaration's* (shallow) lexical scope, so a
    /// frame count would stay constant across nested instantiations and
    /// the guard would never fire.
    pub fn binding_scope_depth(&self) -> usize {
        self.scope
            .frames()
            .iter()
            .map(|f| f.expansion_depth)
            .max()
            .unwrap_or(0)
    }

    /// Structural content supplied by the nearest renderer-driven component
    /// expansion, if any. Returned as block nodes so a placement marker can
    /// feed them back through the caller's normal recursion without lowering
    /// them to a value or rendered string first.
    pub fn structural_content(&self) -> Option<Vec<Block<'a>>> {
        self.structural_slot_content("content")
    }

    /// Structural block nodes supplied for the named slot by the nearest
    /// renderer-driven expansion. Unlike a global block-kind lookup, this is
    /// scoped to one instantiated holder, so two components may declare the
    /// same slot name with unrelated contracts.
    pub fn structural_slot_content(&self, name: &str) -> Option<Vec<Block<'a>>> {
        self.scope
            .frames()
            .iter()
            .rev()
            .find_map(|frame| frame.content.as_deref()?.get(name).cloned())
    }

    /// Expand `body`'s child blocks once per binding set, each under a
    /// scope carrying that set's `name → value` bindings **and a fresh
    /// copy of the body's evaluation cells**. The fresh cells are the
    /// crux: `Field::value` memoises in a per-cell `OnceLock`, so the same
    /// body AST evaluated under different bindings (repeated instances,
    /// repetition iterations) would otherwise collide on the first-seen
    /// value. Caching the fresh cells on `self`'s cell (the per-expansion
    /// owner — the `@contextual` block being expanded) gives each
    /// expansion an independent cache.
    ///
    /// Returns one `Vec<Block>` of child views per binding set, in order.
    /// Child expressions (and `${…}` interpolation) resolve the bindings,
    /// shadowing like an inner `let`; nested components / repeaters stack
    /// their own frames and compose. This is the component/repeater
    /// analogue of the `@children` splice's `computed_children`.
    pub fn expand_bodies(
        &self,
        body: &Block<'a>,
        binding_sets: Vec<std::sync::Arc<Vec<(String, Value)>>>,
    ) -> Vec<Vec<Block<'a>>> {
        self.expand_bodies_inner(body, binding_sets, None)
    }

    /// [`expand_bodies`](Self::expand_bodies) with structural content attached
    /// to every expanded descendant's scope. Used by component renderers so a
    /// placement marker can recover the instance's child block nodes even
    /// after recursion crosses a native wrapper or container.
    pub fn expand_bodies_with_content(
        &self,
        body: &Block<'a>,
        binding_sets: Vec<std::sync::Arc<Vec<(String, Value)>>>,
        content: std::rc::Rc<Vec<Block<'a>>>,
    ) -> Vec<Vec<Block<'a>>> {
        let slots = std::rc::Rc::new(std::collections::BTreeMap::from([(
            "content".to_string(),
            content.as_ref().clone(),
        )]));
        self.expand_bodies_inner(body, binding_sets, Some(slots))
    }

    /// [`expand_bodies`](Self::expand_bodies) with several independently
    /// named structural content slots attached to the expansion scope.
    pub fn expand_bodies_with_slots(
        &self,
        body: &Block<'a>,
        binding_sets: Vec<std::sync::Arc<Vec<(String, Value)>>>,
        slots: std::rc::Rc<std::collections::BTreeMap<String, Vec<Block<'a>>>>,
    ) -> Vec<Vec<Block<'a>>> {
        self.expand_bodies_inner(body, binding_sets, Some(slots))
    }

    /// Shared implementation behind the `expand_bodies` entry points.
    /// `content` carries the caller's slot fills when expanding a block
    /// that declares slots, and is `None` otherwise.
    fn expand_bodies_inner(
        &self,
        body: &Block<'a>,
        binding_sets: Vec<std::sync::Arc<Vec<(String, Value)>>>,
        content: Option<std::rc::Rc<std::collections::BTreeMap<String, Vec<Block<'a>>>>>,
    ) -> Vec<Vec<Block<'a>>> {
        let ItemCellKind::Block { expansions, .. } = &self.cells.kind else {
            return Vec::new();
        };
        let groups = expansions.get_or_init(|| {
            binding_sets
                .into_iter()
                .map(|set| crate::doc::cells::Expansion {
                    bindings: set,
                    // Fresh cells matching `body.ast`'s structure; only
                    // the cells are kept (the clone feeds the builder).
                    cells: ItemCells::build(&ast::Item::Block(body.ast.clone()), None),
                })
                .collect()
        });
        let doc = self.doc;
        groups
            .iter()
            .map(|g| {
                let ItemCellKind::Block {
                    items: fresh_items, ..
                } = &g.cells.kind
                else {
                    return Vec::new();
                };
                let scope = body.scope.push(ScopeFrame {
                    ast: body.ast,
                    cells: &g.cells,
                    // The body's children live in the component
                    // *definition's* file: a library component using its
                    // own namespaced kinds bare must resolve them there,
                    // not at the instantiation site.
                    file_ns: body.file_ns,
                    kind_override: body.kind_override,
                    bindings: Some(g.bindings.clone()),
                    content: content.clone(),
                    // One deeper than the *instance* (`self`), whose own
                    // scope carries the dynamic depth at the
                    // instantiation site — `body.scope` is the
                    // definition's lexical chain and carries none.
                    expansion_depth: self.binding_scope_depth() + 1,
                });
                iter_blocks(&body.ast.items, fresh_items, doc, body.file_ns, scope).collect()
            })
            .collect()
    }

    /// Evaluated values for each label slot. Cached on first call; later
    /// calls return a clone of the cached `Vec`.
    pub fn labels(&self) -> Result<Vec<Value>, EvalError> {
        let (cell, _) = self.block_inner();
        // Evaluate labels in the block's own scope (not root) so an
        // interpolated `$"…${slot}…"` label resolves component/repeater
        // bindings. Bare identifiers still stay opaque literal names, and
        // plain literal labels are scope-independent, so this is
        // behaviour-preserving for every existing label form.
        let scope = self.scope.clone();
        let result = cell.get_or_init(|| {
            self.ast
                .labels
                .iter()
                .map(|e| self.doc.eval_literal_in_scope(e, &scope))
                .collect()
        });
        match result {
            Ok(v) => Ok(v.clone()),
            Err(e) => Err(e.clone()),
        }
    }

    /// Source span of this node in the file that declares it.
    pub fn span(&self) -> Span {
        self.ast.span
    }

    /// Pretty-printed source for this block.
    pub fn to_source(&self) -> String {
        crate::format::to_source_item(&ast::Item::Block(self.ast.clone()))
    }

    /// Realise any pending block-level imports, then return one
    /// `BlockSlice` for the block's own items plus one for each
    /// successfully-loaded import (transitively). The block's own
    /// slice is always element 0.
    pub(in crate::doc) fn realize_and_sources(&self) -> Vec<BlockSlice<'a>> {
        let (_, items_cells) = self.block_inner();
        // Force any unloaded Import cells.
        for cell in items_cells {
            if let ItemCellKind::Import {
                path,
                system,
                base_dir,
                path_span,
                loaded,
            } = &cell.kind
            {
                let _ = loaded.get_or_init(|| {
                    load_import_lazily(
                        path,
                        base_dir.as_deref(),
                        *system,
                        *path_span,
                        self.doc.loader(),
                    )
                });
            }
        }
        let mut out = vec![BlockSlice {
            items: &self.ast.items,
            cells: items_cells,
            file_ns: self.file_ns,
        }];
        push_loaded_imports(items_cells, &mut out);
        out
    }

    /// Slices spliced in by this block's in-block `import`s, excluding the
    /// block's own items (which `realize_and_sources` returns as element
    /// 0). Used by the `@child(ren)` projections to nest imported block
    /// instances after the block's own children — the own slice is handled
    /// separately because table-row interleaving and computed children are
    /// keyed to the block's own cell.
    fn imported_slices(&self) -> Vec<BlockSlice<'a>> {
        let mut all = self.realize_and_sources();
        if !all.is_empty() {
            all.remove(0);
        }
        all
    }

    /// The field with this name, if this body declares one.
    pub fn field(&self, name: &str) -> Option<Field<'a>> {
        let child_scope = self.child_scope();
        for src in self.realize_and_sources() {
            if let Some(f) = find_field(
                src.items,
                src.cells,
                name,
                self.doc,
                src.file_ns,
                &child_scope,
            ) {
                return Some(f);
            }
        }
        None
    }

    /// The first block of this kind in the body, if any.
    pub fn block(&self, kind: &str) -> Option<Block<'a>> {
        let child_scope = self.child_scope();
        for src in self.realize_and_sources() {
            if let Some(b) = find_block(
                src.items,
                src.cells,
                kind,
                self.doc,
                src.file_ns,
                &child_scope,
            ) {
                return Some(b);
            }
        }
        None
    }

    /// Find a `let` binding named `name` declared directly in this
    /// block (or a block-level import). The let's value evaluates in
    /// this block's child scope, so it can reference sibling lets /
    /// fields and ancestors.
    pub(crate) fn find_let(&self, name: &str) -> Option<LetView<'a>> {
        let child_scope = self.child_scope();
        for src in self.realize_and_sources() {
            if let Some(l) = find_let(src.items, src.cells, name, self.doc, &child_scope) {
                return Some(l);
            }
        }
        None
    }

    /// The bare fields declared in this body, in source order.
    pub fn fields(&self) -> impl Iterator<Item = Field<'a>> + 'a {
        let doc = self.doc;
        let scope = self.child_scope();
        self.realize_and_sources()
            .into_iter()
            .flat_map(move |src| iter_fields(src.items, src.cells, doc, src.file_ns, scope.clone()))
    }

    /// The blocks declared in this body, in source order.
    pub fn blocks(&self) -> impl Iterator<Item = Block<'a>> + 'a {
        let doc = self.doc;
        let scope = self.child_scope();
        let synth_scope = scope.clone();
        let synth_ns = self.file_ns;
        // Computed-children splices (`field = <list expr>` for a
        // `@children`/`@child` slot) appear here too, after the literal
        // nested blocks, so renderers that walk `blocks()` (e.g.
        // `render_list`, `render_column`) see generated children.
        let synth = self.computed_children();
        self.realize_and_sources()
            .into_iter()
            .flat_map(move |src| iter_blocks(src.items, src.cells, doc, src.file_ns, scope.clone()))
            .chain(synth.iter().map(move |sc| Block {
                ast: &sc.block,
                cells: &sc.cells,
                doc,
                file_ns: synth_ns,
                kind_override: Some(sc.kind.as_str()),
                scope: synth_scope.clone(),
            }))
    }

    /// Source-order iterator over `Item::Table` entries in this block.
    /// Each [`TableView`] carries the parent field name and the row
    /// values as written. Hosts that want the schema-projected view
    /// should use `typed_field`/`doc.get` instead.
    pub fn tables(&self) -> impl Iterator<Item = TableView<'a>> + 'a {
        let doc = self.doc;
        self.realize_and_sources()
            .into_iter()
            .flat_map(move |src| iter_tables(src.items, doc))
    }

    /// Return the most recently surfaced lazy-import error for this
    /// block, if any. Useful for callers that want to surface load
    /// failures explicitly rather than only seeing `None` from
    /// `field`/`block`.
    pub fn import_errors(&self) -> Vec<EvalError> {
        let (_, items_cells) = self.block_inner();
        let mut out = Vec::new();
        for cell in items_cells {
            if let ItemCellKind::Import { loaded, .. } = &cell.kind
                && let Some(Err(e)) = loaded.get()
            {
                out.push(e.clone());
            }
        }
        out
    }

    /// The namespace qualifier written before this block's kind with
    /// `::` (`wdoc::process` → `["wdoc"]`). Empty for a bare kind and for
    /// synthesised blocks.
    pub fn kind_ns(&self) -> &'a [String] {
        &self.ast.kind_ns
    }

    /// Namespace declared by the file this block instance lives in.
    pub(crate) fn file_ns(&self) -> &'a [String] {
        self.file_ns
    }

    /// The schema (`TypeDecl`) for this block's `kind`, if any. Resolved
    /// namespace-aware: a `::` qualifier selects an explicit namespace,
    /// and a bare kind prefers a declaration in the namespace of the
    /// file this instance lives in (so a `namespace lib` data file's
    /// blocks resolve to `lib`'s schemas regardless of import order).
    pub fn schema(&self) -> Option<TypeDecl<'a>> {
        let k = self.kind();
        let q = self.kind_ns();
        let ctx = self.file_ns;
        self.doc
            .block_schema_in(q, k, ctx)
            .or_else(|| self.doc.table_schema_in(q, k, ctx))
    }

    /// Schema-aware field lookup. Projects the block through its
    /// declared type:
    ///
    /// - `@inline(N)` → returns a synthetic `Field` over the label slot
    /// - `@child(K)`  → returns a `DataRef::Block` of the matching
    ///   nested block (or `None` if absent)
    /// - `@children(K)` → returns a `DataRef::BlockList` of all matching
    ///   nested blocks
    /// - any other named field on the schema → tries a literal child
    ///   field by name
    ///
    /// Returns `None` if the block has no schema, or if the name
    /// doesn't match any schema field or literal item.
    pub fn typed_field(&self, name: &str) -> Option<DataRef<'a>> {
        let schema = self.schema()?;
        let f = schema.field(name)?;

        // `@connections(SchemaName)`: project sibling Item::Connection
        // statements through the named connection schema. Memoised per
        // (kind, name) — see `typed_proj_memo`.
        if let Some(conn_schema) = f.connection_schema() {
            if let Some(hit) = self.typed_proj_memo_get(name) {
                return Some(DataRef::from_variant_value(hit));
            }
            let scope = self.child_scope();
            // Connection statements live in the block's own items and in
            // any in-block import it splices in; project across both.
            let mut values = Vec::new();
            for src in self.realize_and_sources() {
                values.extend(self.doc.project_connections(src.items, conn_schema, &scope));
            }
            let projected = Value::List(std::sync::Arc::new(values));
            self.typed_proj_memo_insert(name, projected.clone());
            return Some(DataRef::from_variant_value(projected));
        }

        // Union-typed @children: dispatch every nested block / table
        // row to a Value::Variant via structural-shape matching.
        if let Some(crate::doc::ChildKind::Union(union)) = f.children_kind_or_union() {
            if let Some(Value::List(items)) = self.typed_proj_memo_get(name) {
                return Some(DataRef::from_variant_value_list(items.to_vec()));
            }
            let dr = self.dispatch_union_children(name, union);
            if let DataKind::VariantValueList(items) = dr.inner() {
                self.typed_proj_memo_insert(name, Value::List(std::sync::Arc::new(items.clone())));
            }
            return Some(dr);
        }
        // Union-typed @child: dispatch the single matching nested block.
        if let Some(crate::doc::ChildKind::Union(union)) = f.child_kind_or_union() {
            if let Some(hit) = self.typed_proj_memo_get(name) {
                return Some(DataRef::from_variant_value(hit));
            }
            let dr = self.dispatch_union_child(union);
            if let DataKind::VariantValue(v) = dr.inner() {
                self.typed_proj_memo_insert(name, v.clone());
            }
            return Some(dr);
        }

        if let Some(kind) = f.children_block_kind_str() {
            // Use the projection: combines literal nested blocks of
            // this kind with synthesised blocks from `Item::Table`
            // rows under the matching field name.
            let blocks = match self.children_projection(name, kind) {
                Ok(blocks) => blocks,
                Err(e) => return Some(DataRef::from_error(e)),
            };
            let is_table = self.doc.table_schema_in(&[], kind, self.file_ns).is_some();
            return Some(if is_table {
                DataRef::from_table(blocks)
            } else {
                DataRef::from_block_list(blocks)
            });
        }
        if let Some(kind) = f.child_block_kind() {
            let block = self.blocks().find(|b| b.kind() == kind)?;
            return Some(DataRef::from_block(block));
        }
        if let Some(slot) = f.inline_slot() {
            // Inline labels become a synthetic field — we don't have a `Field`
            // view for a label, so return the typed-field view. But when the
            // inline label was omitted and an explicit `name = value` field of
            // the same name is present, read that instead, so `block { text =
            // "x" }` works like the inline `block "x"`. Hosts wanting the label
            // value should access `block.labels()` directly.
            let has_label = self
                .labels()
                .map(|ls| (slot as usize) < ls.len())
                .unwrap_or(false);
            if !has_label && let Some(field) = self.field(name) {
                return Some(DataRef::from_field(field));
            }
            return Some(DataRef::new(DataKind::TypeField(f)));
        }
        // Plain schema field → look it up in literal block items. An
        // unset optional (or `@default`-carrying) field projects its
        // default / `none` so member access reads it as `none` rather
        // than failing with an unresolved-reference error — `??` and
        // `match` over `block.optional_field` then behave as authored.
        if let Some(field) = self.field(name) {
            return Some(DataRef::from_field(field));
        }
        if f.optional() || f.default_value().is_some() {
            return Some(DataRef::from_variant_value(
                f.default_value().unwrap_or(Value::None),
            ));
        }
        None
    }

    /// Reify this block as a `Value::Record`, so a `@children`/`@child`
    /// reference can be consumed as ordinary list/record data by builtins
    /// (`len`, `map`, …), arithmetic, and a repetition block's `each`.
    ///
    /// Schema-aware, mirroring the wdoc lowering reifier: each declared
    /// field is populated from its `@inline(N)` label slot, its
    /// `@children`/`@child` projection (recursively reified), a literal
    /// block field, a leaf typed projection, or the schema default — with
    /// missing optionals becoming `Value::None`. An un-schema'd block
    /// reifies its literal fields verbatim, keyed by its block kind.
    pub fn to_record_value(&self) -> Result<Value, EvalError> {
        self.to_record_value_at(&[])
    }

    /// [`to_record_value`](Self::to_record_value) with the document path
    /// that re-resolves this block from root (`base`). The path is only
    /// consulted to address `@by_ref` child slots: a `@child`/`@children`
    /// slot whose kind is marked `@by_ref` reifies to a
    /// `Value::DataPath { segments: base + [field, …] }` reference instead
    /// of inlining its content (so the referenced content — e.g. a wdoc
    /// `body` — can be projected elsewhere). Every other field reifies
    /// identically regardless of `base`, so a document with no `@by_ref`
    /// kinds produces byte-identical records. An empty `base` (the entry
    /// reifier couldn't address this block) still reifies normally; the
    /// reference it emits simply won't resolve, which the consumer handles.
    pub(crate) fn to_record_value_at(&self, base: &[String]) -> Result<Value, EvalError> {
        use std::collections::BTreeMap;
        let Some(schema) = self.schema() else {
            // Un-schema'd block: literal fields only.
            let mut map = BTreeMap::new();
            for field in self.fields() {
                map.insert(
                    field.name().to_string(),
                    field.value().cloned().map_err(|e| e.clone())?,
                );
            }
            return Ok(Value::Record {
                ty: vec![self.kind().to_string()],
                fields: std::sync::Arc::new(map),
            });
        };
        let labels = self.labels().unwrap_or_default();
        let mut map = BTreeMap::new();
        for f in schema.fields() {
            let name = f.name();
            let is_child_slot =
                f.children_kind_or_union().is_some() || f.child_kind_or_union().is_some();
            let val = if let Some(slot) = f.inline_slot() {
                // An `@inline(N)` field is normally the positional label slot,
                // but also honour an explicit `name = value` field of the same
                // name, so `node { text = "x" }` reads the same as `node "x"`.
                // Prefer the label when it was actually given.
                if let Some(v) = labels.get(slot as usize).cloned() {
                    v
                } else if let Some(field) = self.field(name) {
                    field.value().cloned().map_err(|e| e.clone())?
                } else {
                    f.default_value().unwrap_or(Value::None)
                }
            } else if is_child_slot {
                // Project the slot and recursively reify; a computed splice
                // is schema-completed exactly like statically-nested blocks.
                // Extend the address by this field name so a nested `@by_ref`
                // slot emits a root-resolvable reference.
                let mut child_base = base.to_vec();
                child_base.push(name.to_string());
                match self
                    .typed_field(name)
                    .and_then(|dr| dataref_to_value_at(&dr, &child_base))
                {
                    Some(v) => v?,
                    None => f.default_value().unwrap_or(Value::None),
                }
            } else if let Some(field) = self.field(name) {
                field.value().cloned().map_err(|e| e.clone())?
            } else if let Some(dr) = self.typed_field(name) {
                dr.value()
                    .unwrap_or_else(|_| f.default_value().unwrap_or(Value::None))
            } else {
                f.default_value().unwrap_or(Value::None)
            };
            map.insert(name.to_string(), val);
        }
        Ok(Value::Record {
            ty: vec![schema.name().to_string()],
            fields: std::sync::Arc::new(map),
        })
    }

    /// Dispatch all of a `@children(SomeUnion)` field's nested blocks
    /// and table rows through structural-shape matching to produce a
    /// list of `Value::Variant`. Failures from individual blocks or
    /// rows are silently skipped here; the schema check pipeline
    /// emits them via `Document::schema_errors()`.
    fn dispatch_union_children(&self, field_name: &str, union: UnionDecl<'a>) -> DataRef<'a> {
        let mut out: Vec<Value> = Vec::new();
        for (kind, blk) in self.union_children_blocks(field_name) {
            let v = match kind {
                UnionChildKind::Nested => variant_dispatch::block_to_variant(self.doc, &blk, union),
                UnionChildKind::TableRow => {
                    variant_dispatch::table_row_to_variant(self.doc, &blk, union)
                }
            };
            if let Ok(v) = v {
                out.push(v);
            }
        }
        // Computed-children splice (`field = <list expr>`): the field's
        // declared `list<Union>` type already coerced each bare record to
        // a variant by shape (`Field::value`), so just splice them in.
        if let Some(field) = self.field(field_name)
            && let Ok(Value::List(items)) = field.value()
        {
            for it in items.iter() {
                if matches!(it, Value::Variant { .. }) {
                    out.push(it.clone());
                }
            }
        }
        DataRef::from_variant_value_list(out)
    }

    /// Iterate the nested-block + synth-row sources for a union-typed
    /// `@children(SomeUnion)` field. Each entry comes back with a
    /// tag identifying which dispatcher should consume it.
    pub(crate) fn union_children_blocks(
        &self,
        field_name: &str,
    ) -> Vec<(UnionChildKind, Block<'a>)> {
        let (items_cells, synth_rows) = match &self.cells.kind {
            ItemCellKind::Block {
                items, synth_rows, ..
            } => (items, synth_rows),
            _ => unreachable!("Block view wraps a Block cell"),
        };
        let mut out: Vec<(UnionChildKind, Block<'a>)> = Vec::new();
        let child_scope = self.child_scope();
        for (item, cells) in self.ast.items.iter().zip(items_cells.iter()) {
            match item {
                ast::Item::Block(b) => {
                    out.push((
                        UnionChildKind::Nested,
                        Block {
                            ast: b,
                            cells,
                            doc: self.doc,
                            file_ns: self.file_ns,
                            kind_override: None,
                            scope: child_scope.clone(),
                        },
                    ));
                }
                ast::Item::Table(t) if t.field_name == field_name => {
                    let mut synth_iter = synth_rows.iter().filter(|r| r.field_name == field_name);
                    for _ in &t.rows {
                        if let Some(sr) = synth_iter.next() {
                            out.push((
                                UnionChildKind::TableRow,
                                Block {
                                    ast: &sr.block,
                                    cells: &sr.cells,
                                    doc: self.doc,
                                    file_ns: self.file_ns,
                                    kind_override: None,
                                    scope: child_scope.clone(),
                                },
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        // In-block imports splice their top-level block instances in as
        // nested children too; dispatch decides which (if any) variant
        // each matches by shape.
        for src in self.imported_slices() {
            for (item, cells) in src.items.iter().zip(src.cells.iter()) {
                if let ast::Item::Block(b) = item {
                    out.push((
                        UnionChildKind::Nested,
                        Block {
                            ast: b,
                            cells,
                            doc: self.doc,
                            file_ns: src.file_ns,
                            kind_override: None,
                            scope: child_scope.clone(),
                        },
                    ));
                }
            }
        }
        out
    }

    /// Dispatch a single nested block to a variant for a
    /// `@child(SomeUnion)` field.
    fn dispatch_union_child(&self, union: UnionDecl<'a>) -> DataRef<'a> {
        let (items_cells, _) = match &self.cells.kind {
            ItemCellKind::Block {
                items, synth_rows, ..
            } => (items, synth_rows),
            _ => unreachable!("Block view wraps a Block cell"),
        };
        let child_scope = self.child_scope();
        for (item, cells) in self.ast.items.iter().zip(items_cells.iter()) {
            if let ast::Item::Block(b) = item {
                let blk = Block {
                    ast: b,
                    cells,
                    doc: self.doc,
                    file_ns: self.file_ns,
                    kind_override: None,
                    scope: child_scope.clone(),
                };
                if let Ok(v) = variant_dispatch::block_to_variant(self.doc, &blk, union) {
                    return DataRef::from_variant_value(v);
                }
            }
        }
        // Fall back to in-block imports: the first imported block instance
        // that matches a variant wins.
        for src in self.imported_slices() {
            for (item, cells) in src.items.iter().zip(src.cells.iter()) {
                if let ast::Item::Block(b) = item {
                    let blk = Block {
                        ast: b,
                        cells,
                        doc: self.doc,
                        file_ns: src.file_ns,
                        kind_override: None,
                        scope: child_scope.clone(),
                    };
                    if let Ok(v) = variant_dispatch::block_to_variant(self.doc, &blk, union) {
                        return DataRef::from_variant_value(v);
                    }
                }
            }
        }
        DataRef::from_variant_value(Value::None)
    }

    /// Build the list of `Block`s for one `@children(kind)` field —
    /// combining literal nested `Block`s of the matching kind with
    /// the parent's pre-built synthesised row-Blocks whose
    /// `field_name` matches. The synthesised blocks store an empty
    /// kind in the AST; we set `kind_override` here so views see the
    /// correct kind.
    ///
    /// Fails only when a `@contextual` child's generated blocks are
    /// demanded from a document opened without an expander — see
    /// [`Block::expand_children`].
    fn children_projection(
        &self,
        field_name: &str,
        kind: &'a str,
    ) -> Result<Vec<Block<'a>>, EvalError> {
        let (items_cells, synth_rows) = match &self.cells.kind {
            ItemCellKind::Block {
                items, synth_rows, ..
            } => (items, synth_rows),
            _ => unreachable!("Block view wraps a Block cell"),
        };
        let mut out: Vec<Block<'a>> = Vec::new();
        // Walk items + cells in source order. Real Item::Block entries
        // contribute their own Block view; Item::Table entries are
        // replaced (in-order) by their corresponding synthesised rows
        // from `synth_rows`.
        let child_scope = self.child_scope();
        let mut synth_iter = synth_rows.iter().filter(|r| r.field_name == field_name);
        for (item, cells) in self.ast.items.iter().zip(items_cells.iter()) {
            match item {
                ast::Item::Block(b) if b.kind == kind => {
                    out.push(Block {
                        ast: b,
                        cells,
                        doc: self.doc,
                        file_ns: self.file_ns,
                        kind_override: None,
                        scope: child_scope.clone(),
                    });
                }
                ast::Item::Table(t) if t.field_name == field_name => {
                    // Pull one synthesised row per source row.
                    for _ in &t.rows {
                        if let Some(sr) = synth_iter.next() {
                            out.push(Block {
                                ast: &sr.block,
                                cells: &sr.cells,
                                doc: self.doc,
                                file_ns: self.file_ns,
                                kind_override: Some(kind),
                                scope: child_scope.clone(),
                            });
                        }
                    }
                }
                // A `@contextual` child expands here so its generated
                // blocks of the matching kind participate in the slot
                // exactly like literal ones — without this, data-driven
                // children inside a custom shape silently vanished from
                // `@children` projections (the lowering record).
                ast::Item::Block(b) => {
                    let view = Block {
                        ast: b,
                        cells,
                        doc: self.doc,
                        file_ns: self.file_ns,
                        kind_override: None,
                        scope: child_scope.clone(),
                    };
                    if view.is_contextual() {
                        push_generated_matching(&view, kind, &mut out)?;
                    }
                }
                _ => {}
            }
        }
        // In-block imports splice their top-level block instances of the
        // matching kind in as children, after the block's own nested
        // blocks / table rows (mirroring `blocks()` ordering).
        for src in self.imported_slices() {
            for (item, cells) in src.items.iter().zip(src.cells.iter()) {
                if let ast::Item::Block(b) = item
                    && b.kind == kind
                {
                    out.push(Block {
                        ast: b,
                        cells,
                        doc: self.doc,
                        file_ns: src.file_ns,
                        kind_override: None,
                        scope: child_scope.clone(),
                    });
                }
            }
        }
        // Computed children: a `field = <list expr>` splice for this
        // slot (see `computed_children`). Appended after the literal
        // nested blocks / table rows, preserving the "everything mixes,
        // in source order" rule (the splice runs last).
        for sc in self.computed_children() {
            if sc.field_name == field_name {
                out.push(Block {
                    ast: &sc.block,
                    cells: &sc.cells,
                    doc: self.doc,
                    file_ns: self.file_ns,
                    kind_override: Some(sc.kind.as_str()),
                    scope: child_scope.clone(),
                });
            }
        }
        Ok(out)
    }

    /// `true` when this block is context-polymorphic — its `@block` type
    /// carries `@contextual`. Such a block is legal wherever children
    /// are allowed at all, its body is not recursed into by the child
    /// walk, and its children come from the host's expander.
    ///
    /// An instance of a `@declares_kind`-declared kind answers `true`
    /// through the same route as any other block: the schema derived
    /// from its declarer carries `@contextual`, because what such an
    /// instance emits is whatever its declarer's body expands to.
    pub(crate) fn is_contextual(&self) -> bool {
        self.schema().is_some_and(|t| t.is_contextual())
    }

    /// The blocks this `@contextual` block generates, flattened across
    /// every expansion (a repetition's iterations, an instantiated
    /// body). Produced by the [`Expander`](crate::Expander) the host
    /// registered on the [`Environment`](crate::Environment), so the
    /// blocks a projection sees are exactly the ones the host's own
    /// renderer sees — including the diagnostics it records on the way.
    ///
    /// Empty for a block that is not `@contextual`, and for one past
    /// the expansion-depth cap. Demanding the children of a `@contextual`
    /// block with no registered expander is a hard error: the language
    /// declines to guess at expansion semantics it does not own.
    pub fn expand_children(&self) -> Result<Vec<Block<'a>>, EvalError> {
        if !self.is_contextual() {
            return Ok(Vec::new());
        }
        // Mirrors the renderer's own recursion guard: a self-referential
        // expansion stops here rather than growing without bound.
        if self.binding_scope_depth() > MAX_EXPANSION_DEPTH {
            return Ok(Vec::new());
        }
        match self.doc.expander() {
            Some(e) => Ok(e.expand(self)),
            None => Err(EvalError::missing_expander(self.kind(), self.span())),
        }
    }

    /// Lazily materialise the *computed children* of this block — the
    /// `@children(kind)` / `@child(kind)` slots authored as a value
    /// expression (`field = map(data, …)`) instead of nested block
    /// literals (a "splice"). Each list element becomes one
    /// value-backed synthetic `Block` of the slot's concrete kind,
    /// cached in the block cell so repeated projection / `blocks()` /
    /// validation passes reuse the same owned storage.
    ///
    /// Union-typed `@children(SomeUnion)` slots are **not** synthesised
    /// here — they're consumed as a coerced `Value::List` of variants by
    /// the value path (`dispatch_union_children` / `block_to_record`).
    /// Interface slots are skipped too: a bare record carries no kind
    /// tag, so a concrete child kind can't be inferred for them.
    pub(crate) fn computed_children(&self) -> &'a [crate::doc::cells::SynthChild] {
        let ItemCellKind::Block {
            computed_children, ..
        } = &self.cells.kind
        else {
            return &[];
        };
        computed_children
            .get_or_init(|| self.build_computed_children())
            .as_slice()
    }

    /// Build the synthetic child blocks this block's schema implies —
    /// the uncached half of `computed_children`.
    fn build_computed_children(&self) -> Vec<crate::doc::cells::SynthChild> {
        let mut out = Vec::new();
        let Some(schema) = self.schema() else {
            return out;
        };
        for f in schema.fields() {
            // Only concrete-kind @children / @child slots.
            let (kind, is_list) = if let Some(k) = f.children_block_kind() {
                (k, true)
            } else if let Some(k) = f.child_block_kind() {
                (k, false)
            } else {
                continue;
            };
            // A literal `field = expr` present? (Nested-block / table-row
            // authoring leaves no `Item::Field` of this name, so a hit
            // here means the splice form.)
            let Some(field) = self.field(f.name()) else {
                continue;
            };
            let Ok(value) = field.value() else {
                continue;
            };
            match value {
                Value::List(items) if is_list => {
                    for el in items.iter() {
                        if let Some(sc) = synth_child_from_value(self.doc, f.name(), &kind, el) {
                            out.push(sc);
                        }
                    }
                }
                // A single-block `@child` slot: the value is one element.
                single if !is_list && !matches!(single, Value::None) => {
                    if let Some(sc) = synth_child_from_value(self.doc, f.name(), &kind, single) {
                        out.push(sc);
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// Iterate schema-projected fields in declared order. Empty for
    /// un-schema'd blocks.
    pub fn typed_fields(&self) -> Box<dyn Iterator<Item = (&'a str, DataRef<'a>)> + 'a> {
        let Some(schema) = self.schema() else {
            return Box::new(std::iter::empty());
        };
        let this = self.clone();
        Box::new(schema.fields().filter_map(move |f| {
            let name = f.name();
            this.typed_field(name).map(|dr| (name, dr))
        }))
    }

    /// Schema-content validation errors for this block. Computed and
    /// cached on first access; subsequent calls return the cached slice.
    pub fn schema_errors(&self) -> &'a [EvalError] {
        let ItemCellKind::Block {
            schema_validation, ..
        } = &self.cells.kind
        else {
            unreachable!("Block view wraps a Block cell")
        };
        let result = schema_validation.get_or_init(|| compute_schema_errors(self));
        result.as_slice()
    }
}

/// Source-level view of an `Item::Table` (a `FIELD:` header followed
/// by one or more `| ... |` rows) within a parent block.
#[derive(Clone, Copy)]
pub struct TableView<'a> {
    /// The AST node this view borrows.
    pub(in crate::doc) ast: &'a ast::TableItem,
    /// The document these views read through.
    pub(in crate::doc) doc: &'a Document,
}

impl<'a> TableView<'a> {
    /// Name of the parent-block field that this table binds to.
    pub fn field_name(&self) -> &'a str {
        &self.ast.field_name
    }

    /// Iterator over the rows in source order.
    pub fn rows(&self) -> impl Iterator<Item = RowView<'a>> + 'a {
        let doc = self.doc;
        self.ast.rows.iter().map(move |r| RowView { ast: r, doc })
    }

    /// Source span of this node in the file that declares it.
    pub fn span(&self) -> Span {
        self.ast.span
    }
}

/// Source-level view of a single `| ... |` row inside a [`TableView`].
#[derive(Clone, Copy)]
pub struct RowView<'a> {
    /// The AST node this view borrows.
    ast: &'a ast::Row,
    /// The document these views read through.
    doc: &'a Document,
}

impl<'a> RowView<'a> {
    /// Evaluate each cell expression and return the row as values.
    /// Cells are treated as literals: bare identifiers materialise as
    /// `Value::Identifier`, not resolved through the enclosing scope.
    pub fn values(&self) -> Result<Vec<Value>, EvalError> {
        self.ast
            .values
            .iter()
            .map(|e| self.doc.eval_literal(e))
            .collect()
    }

    /// Source span of this node in the file that declares it.
    pub fn span(&self) -> Span {
        self.ast.span
    }
}
