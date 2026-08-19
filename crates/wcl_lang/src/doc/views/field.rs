//! Views over fields: the declared kind ([`TypeField`]), the document
//! data kind ([`Field`]), and the composition helper ([`LetView`]).
//!
//! Only [`Field`] is document data. A [`TypeField`] describes what a
//! field may hold, and a [`LetView`] is resolvable by name but never
//! appears in the document model.

use super::decl::resolve_child_kind_arg;
use super::decorator::iter_decorators;
use super::*;

#[derive(Clone, Copy)]
/// One field of a [`TypeDecl`], [`InterfaceDecl`] or record variant,
/// with its type and decorators resolvable against the document.
pub struct TypeField<'a> {
    /// The AST node this view borrows.
    pub(in crate::doc) ast: &'a ast::TypeField,
    /// Lazily-evaluated caches for this item's decorators.
    pub(in crate::doc) decorator_cells: &'a [DecoratorCell],
    /// The document these views read through.
    pub(in crate::doc) doc: &'a Document,
    /// Namespace of the declaration (type / interface / union variant)
    /// that owns this field. Type references in the field's decorators
    /// (`@child`/`@children`/`@connections`) and its declared type
    /// resolve relative to this namespace first.
    pub(in crate::doc) file_ns: &'a [String],
}

impl<'a> TypeField<'a> {
    /// Decorators attached to this item, in source order.
    pub fn decorators(&self) -> impl Iterator<Item = Decorator<'a>> + 'a {
        iter_decorators(
            &self.ast.decorators,
            self.decorator_cells,
            self.doc,
            self.file_ns,
        )
    }

    /// The doc comment (contiguous `#` / `//` lines) directly above this
    /// field, or `None`.
    pub fn doc_comment(&self) -> Option<String> {
        doc_comment_from_trivia(&self.ast.leading_trivia)
    }

    /// The field's declared type, resolved relative to the namespace of the
    /// declaration that owns it. Prefer this over
    /// `doc.resolve(field.type_ref())`, which resolves from the document's
    /// root namespace and so can answer a same-named type from another
    /// namespace (`wdoc.Container` for an `acme` field typed `Container`).
    pub fn resolved_type(&self) -> ResolvedType<'a> {
        self.doc.resolve_in(&self.ast.ty, self.file_ns)
    }

    /// The field's [`FieldShape`] — the typed answer to "what kind of
    /// thing does this field hold", resolved in the declaring namespace
    /// with type aliases peeled. Prefer this over matching on
    /// `type_ref().to_string()`, which an alias or a type whose name
    /// starts with `fn` defeats without saying so.
    pub fn shape(&self) -> FieldShape<'a> {
        FieldShape::from_resolved(self.doc, self.resolved_type(), ALIAS_DEPTH)
    }

    /// If this field carries an `@inline(N)` decorator, returns N.
    /// Used by schemas to map block label slots to typed fields.
    pub fn inline_slot(&self) -> Option<u64> {
        let dec = self.decorators().find(|d| d.is(BuiltinDecorator::Inline))?;
        dec.positional().ok()?.first()?.as_u64()
    }

    /// Default value for this field, if any. Priority:
    /// 1. The inline `name = expr` form (stored as `default_expr`).
    /// 2. The `@default(v)` decorator (classic form).
    ///
    /// Both forms produce the same `Value`; the inline form just
    /// avoids spelling the type a second time.
    pub fn default_value(&self) -> Option<Value> {
        if let Some(expr) = &self.ast.default_expr {
            return self.doc.eval_literal(expr).ok();
        }
        let dec = self
            .decorators()
            .find(|d| d.is(BuiltinDecorator::Default))?;
        dec.positional().ok()?.into_iter().next()
    }

    /// If this field carries an `@child("kind")` decorator, returns the
    /// nested block kind it binds. Returns `None` when the decorator
    /// is absent OR when its positional arg names a union type rather
    /// than a string kind (use [`child_kind_or_union`] for the union
    /// case).
    pub fn child_block_kind(&self) -> Option<String> {
        match self.child_kind_or_union()? {
            ChildKind::Kind(s) => Some(s),
            ChildKind::Union(_) | ChildKind::Interface(_) => None,
        }
    }

    /// If this field carries an `@children("kind", min?, max?)`
    /// decorator, returns the nested block kind it binds. Returns
    /// `None` for the union form — use [`children_kind_or_union`].
    pub fn children_block_kind(&self) -> Option<String> {
        match self.children_kind_or_union()? {
            ChildKind::Kind(s) => Some(s),
            ChildKind::Union(_) | ChildKind::Interface(_) => None,
        }
    }

    /// Resolves the positional arg of `@child(...)` into either a
    /// string kind or a union declaration. `None` when the decorator
    /// is absent or the arg is neither.
    pub fn child_kind_or_union(&self) -> Option<ChildKind<'a>> {
        let dec = self.decorators().find(|d| d.is(BuiltinDecorator::Child))?;
        resolve_child_kind_arg(self.doc, self.file_ns, &dec.positional().ok()?)
    }

    /// Resolves the positional arg of `@children(...)` into either a
    /// string kind or a union declaration. `None` when the decorator
    /// is absent or the arg is neither.
    pub fn children_kind_or_union(&self) -> Option<ChildKind<'a>> {
        let dec = self
            .decorators()
            .find(|d| d.is(BuiltinDecorator::Children))?;
        resolve_child_kind_arg(self.doc, self.file_ns, &dec.positional().ok()?)
    }

    /// Resolves the positional arg of `@connections(...)` into a
    /// connection schema. `None` if the decorator is absent or the
    /// positional arg doesn't name a declared connection.
    pub fn connection_schema(&self) -> Option<ConnectionDecl<'a>> {
        let dec = self
            .decorators()
            .find(|d| d.is(BuiltinDecorator::Connections))?;
        let positional = dec.positional().ok()?;
        let first = positional.first()?;
        let name = match first {
            Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) => s,
            _ => return None,
        };
        // Resolve relative to the declaring field's namespace first.
        let resolved = self
            .doc
            .resolve_path_in(std::slice::from_ref(name), self.file_ns);
        let key = resolved
            .map(|p| p.join("."))
            .unwrap_or_else(|| name.clone());
        self.doc.connection_decl(&key)
    }

    /// If this field carries a `@ref("kind")` decorator, returns the
    /// block kind its value must reference (the id / first label of an
    /// existing block of that kind). `None` when the decorator is absent
    /// or its positional arg isn't a string kind. Drives the
    /// dangling-reference check in `wcl check`.
    pub fn ref_block_kind(&self) -> Option<String> {
        let dec = self.decorators().find(|d| d.is(BuiltinDecorator::Ref))?;
        match dec.positional().ok()?.first()? {
            Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) => Some(s.clone()),
            _ => None,
        }
    }

    /// Like [`children_block_kind`] but borrows directly from the AST
    /// — useful when callers need a `&'a str` (e.g. to plug into a
    /// `Block::kind_override`). `None` if the decorator isn't present
    /// or the positional arg isn't a string literal.
    pub fn children_block_kind_str(&self) -> Option<&'a str> {
        let dec = self
            .ast
            .decorators
            .iter()
            .find(|d| d.name.last().map(|s| s == "children").unwrap_or(false))?;
        let first = dec.positional.first()?;
        match first {
            crate::ast::Expr::Utf8(s) | crate::ast::Expr::Ascii(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Optional `min` cardinality on `@children(...)`.
    pub fn children_min(&self) -> Option<u64> {
        decorator_u64_named(
            &self.decorators().collect::<Vec<_>>(),
            BuiltinDecorator::Children,
            "min",
        )
    }

    /// Optional `max` cardinality on `@children(...)`.
    pub fn children_max(&self) -> Option<u64> {
        decorator_u64_named(
            &self.decorators().collect::<Vec<_>>(),
            BuiltinDecorator::Children,
            "max",
        )
    }

    /// The declared name.
    pub fn name(&self) -> &'a str {
        &self.ast.name
    }

    /// Source span of this node in the file that declares it.
    pub fn span(&self) -> Span {
        self.ast.span
    }

    /// Pretty-printed source for this type/interface field declaration.
    pub fn to_source(&self) -> String {
        crate::format::to_source_type_field(self.ast)
    }

    /// Whether the field was declared with a trailing `?`.
    pub fn optional(&self) -> bool {
        self.ast.optional
    }

    /// The field's declared type, unresolved. Use `resolved_type` to
    /// follow the name, or `shape` to also peel aliases.
    pub fn type_ref(&self) -> &'a TypeRef {
        &self.ast.ty
    }

    /// FQN segments of this field's element type, peeling `list<…>` and
    /// `&…` wrappers and resolving the name in the declaring file's
    /// namespace (so a `@children("gizmo") gizmos: list<Gizmo>` under
    /// `namespace lib` yields `["lib", "Gizmo"]`). Falls back to the
    /// source-written segments when the name doesn't resolve. `None`
    /// for builtin / function / tensor element types.
    pub(crate) fn element_type_fqn_segments(&self) -> Option<Vec<String>> {
        fn peel(ty: &TypeRef) -> Option<&[String]> {
            match ty {
                TypeRef::Named { path: segs, .. } => Some(segs),
                TypeRef::List(inner) | TypeRef::Reference(inner) => peel(inner),
                _ => None,
            }
        }
        let segs = peel(self.type_ref())?;
        Some(
            self.doc
                .resolve_path_in(segs, self.file_ns)
                .unwrap_or_else(|| segs.to_vec()),
        )
    }
}

#[derive(Clone)]
/// A `name = expr` field of the document, whose value is evaluated on
/// first read and cached thereafter.
pub struct Field<'a> {
    /// The AST node this view borrows.
    pub(in crate::doc) ast: &'a ast::Field,
    /// Lazily-evaluated caches for this item's decorators and fields.
    pub(in crate::doc) cells: &'a ItemCells,
    /// The document these views read through.
    pub(in crate::doc) doc: &'a Document,
    /// Namespace of the file that declared this item, prefixed to
    /// its own name to form the fully-qualified name.
    pub(in crate::doc) file_ns: &'a [String],
    /// Scope the field's expression is evaluated in — the declaring
    /// block's child scope, or the root scope for a top-level field.
    pub(in crate::doc) scope: Scope<'a>,
}

impl<'a> Field<'a> {
    /// The evaluation cache backing this field. Panics if the view was
    /// built over a non-field cell, which the constructors make
    /// unreachable.
    pub(in crate::doc) fn field_cell(&self) -> &'a FieldCell {
        let ItemCellKind::Field(c) = &self.cells.kind else {
            unreachable!("Field view wraps a Field cell")
        };
        c
    }

    /// `true` while this field's RHS is being evaluated higher up the
    /// (single-threaded) evaluation stack — see
    /// [`LetView::mid_evaluation`].
    pub(in crate::doc) fn mid_evaluation(&self) -> bool {
        let cell = self.field_cell();
        cell.value.get().is_none() && cell.evaluating.load(Ordering::Acquire)
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

    /// The declared name.
    pub fn name(&self) -> &'a str {
        &self.ast.name
    }

    /// Source span of this node in the file that declares it.
    pub fn span(&self) -> Span {
        self.ast.span
    }

    /// Return the authored symbol when this field is exactly a symbol
    /// literal (`template = :book`). Computed expressions deliberately
    /// return `None`; hosts use this to avoid false-positive static checks.
    pub fn literal_symbol(&self) -> Option<&'a str> {
        match &self.ast.expr {
            ast::Expr::Symbol(name) => Some(name.as_str()),
            _ => None,
        }
    }

    /// Pretty-printed source for this field (`name = expr`).
    pub fn to_source(&self) -> String {
        crate::format::to_source_item(&ast::Item::Field(self.ast.clone()))
    }

    /// File that declares this field. `None` means "the document's
    /// main source" — the file the host passed to
    /// [`Document::from_file`] (or the in-memory source if the
    /// document was opened via [`Document::open`] from a string).
    ///
    /// Walks both eager imports and any lazy in-block imports that
    /// have been forced (a call to [`Document::get`] forces every
    /// lazy import on the path it walks, which is the regime
    /// CLI-style consumers operate in).
    pub fn source_path(&self) -> Option<&'a Path> {
        let target: *const ast::Field = self.ast;
        self.doc.find_field_source_path(target)
    }

    /// The field's evaluated value.
    ///
    /// Forces the expression on first call and caches the outcome, so a
    /// field that fails to evaluate reports the same error on every
    /// later read rather than being retried.
    pub fn value(&self) -> Result<&'a Value, &'a EvalError> {
        let cell = self.field_cell();
        if let Some(cached) = cell.value.get() {
            return cached.as_ref();
        }
        let _profile_guard = self
            .doc
            .profile_enter(crate::diagnostics::ProfileKey::Field {
                path: self.ast.name.clone(),
            });
        // Strict membership check (skipped when the field is
        // `@schemaless`). The field must be named by either the
        // enclosing block's schema or, for top-level fields, the
        // document's `@document` schema.
        if !has_schemaless(&self.ast.decorators)
            && let Some(err) = self.schema_membership_error()
        {
            let _ = cell.value.set(Err(err));
            return cell
                .value
                .get()
                .expect("just-set membership error")
                .as_ref();
        }
        if cell.evaluating.swap(true, Ordering::Acquire) {
            let _ = cell.value.set(Err(EvalError::Cycle {
                field: self.ast.name.clone(),
                span: span_to_miette(self.ast.span),
            }));
            return cell
                .value
                .get()
                .expect("cycle cell was just initialised")
                .as_ref();
        }
        // For `&T`-typed fields, evaluate the RHS as a path producing
        // a `DataRef`. If the target is a leaf `Field`, auto-deref to
        // its value. Otherwise (type / union / variant / block / …),
        // produce a `Value::DataPath` so reflective builtins can keep
        // walking. Non-reference fields evaluate normally through
        // `eval_in_scope`.
        let result = if matches!(self.declared_type_ref(), Some(TypeRef::Reference(_))) {
            let ctx = EvalCtx::new(self.scope.clone());
            self.doc
                .eval_to_dataref(&self.ast.expr, &ctx)
                .and_then(|dr| {
                    let segments = expr_to_path_segments(&self.ast.expr).unwrap_or_default();
                    materialise_dataref_or_path(dr, segments, self.ast.span)
                })
        } else if matches!(
            self.declared_type_ref(),
            Some(TypeRef::Builtin(BuiltinType::Identifier))
        ) {
            // Identifier-typed field: a *bare* identifier stays an opaque
            // name (`id = web` → `"web"`, not a variable lookup), but any
            // other expression evaluates in the field's scope — so a
            // data-derived `id = s.key` (a repeater/component binding)
            // resolves instead of being looked up at root. A string result
            // (quoted ref, interpolation) coerces to `Value::Identifier` so
            // `ref == id` joins hold regardless of authoring style.
            self.doc
                .eval_literal_in_scope(&self.ast.expr, &self.scope)
                .map(str_to_identifier)
        } else if matches!(
            self.declared_type_ref(),
            Some(TypeRef::List(inner)) if matches!(inner.as_ref(), TypeRef::Builtin(BuiltinType::Identifier))
        ) {
            // `list<identifier>` field: the element-wise lift of the rule
            // above. A bare-id list (`members = [shop, stripe]`) keeps each
            // name opaque so it can reference shapes by id, while a
            // data-derived expression still evaluates normally. String
            // elements coerce to identifiers like the scalar rule above.
            self.doc
                .eval_identifier_list_in_scope(&self.ast.expr, &self.scope)
                .map(|v| match v {
                    Value::List(items) => Value::List(std::sync::Arc::new(
                        std::sync::Arc::unwrap_or_clone(items)
                            .into_iter()
                            .map(str_to_identifier)
                            .collect(),
                    )),
                    other => other,
                })
        } else {
            self.doc
                .eval_in_scope(&self.ast.expr, &self.scope)
                .and_then(|v| match self.declared_type_ref() {
                    // Coerce a bare-record value to the field's declared
                    // union variant by shape (recursing through lists).
                    Some(ty) => coerce_value_to_type(self.doc, v, ty, self.ast.span),
                    None => Ok(v),
                })
        };
        // A literal unit that reached here unresolved (e.g. an untyped
        // field, or a `&T`/identifier-typed field that doesn't coerce) has
        // no declared type to resolve against — that is an error, not a
        // silent `PendingUnit`. (A typed field already resolved or errored
        // in `coerce_value_to_type` above.)
        let result = result.and_then(|v| match v {
            Value::PendingUnit { unit, .. } => {
                Err(EvalError::unit_without_type(unit, self.ast.span))
            }
            other => Ok(other),
        });
        cell.evaluating.store(false, Ordering::Release);
        cell.value.get_or_init(|| result).as_ref()
    }

    /// Evaluate this field against a caller-supplied type instead of the
    /// schema-declared one, named by its fully-qualified name
    /// (`"std.Duration"`).
    ///
    /// The escape hatch for hosts whose blocks are `@schemaless`: a unit
    /// literal resolves against a *declared* type, so in a block that
    /// declares no fields `value()` can only report
    /// [`EvalError::UnitWithoutType`]. A host that knows out-of-band what
    /// a field means (say, from its own parameter schema) resolves it
    /// here.
    ///
    /// A value that isn't a unit literal passes through whatever coercion
    /// the named type implies, exactly as a declared field would. The
    /// result is *not* cached — `value()`'s cache keeps meaning "the
    /// schema-typed value" — so call this once per field and keep the
    /// result.
    pub fn value_typed(&self, type_fqn: &str) -> Result<Value, EvalError> {
        let ty = TypeRef::named(type_fqn.split('.').map(str::to_string).collect());
        let value = self.doc.eval_in_scope(&self.ast.expr, &self.scope)?;
        coerce_value_to_type(self.doc, value, &ty, self.ast.span)
    }

    /// `Some(err)` if this field's name isn't accepted by the
    /// applicable schema (parent block, or the document if top-level).
    /// `None` means the membership check passes.
    fn schema_membership_error(&self) -> Option<EvalError> {
        use crate::error::SchemaViolationKind as Kind;
        match self.scope.frames().last().cloned() {
            Some(frame) => {
                // Whole-block opt-out shadows individual fields too.
                if has_schemaless(&frame.ast.decorators) {
                    return None;
                }
                let block = Block {
                    ast: frame.ast,
                    cells: frame.cells,
                    doc: self.doc,
                    file_ns: frame.file_ns,
                    kind_override: frame.kind_override,
                    scope: Scope::root(),
                };
                match block.schema() {
                    // A `@schemaless` type declaration opens all its
                    // instances — every undeclared field passes, with no
                    // per-instance annotation.
                    Some(schema) if schema.is_schemaless() => None,
                    Some(schema) if schema.field(self.name()).is_some() => None,
                    Some(schema) => Some(EvalError::schema_violation(
                        Kind::UnknownField,
                        format!(
                            "field '{}' is not declared by schema '{}'",
                            self.name(),
                            schema.name()
                        ),
                        self.ast.span,
                    )),
                    // Inside an un-schema'd block — the enclosing
                    // block's UnregisteredKind covers it.
                    None => None,
                }
            }
            None => {
                // Top-level field: validate against the *merged*
                // `@document` schema(s) governing this field's source
                // namespace. The field is fine if any of them declares
                // it (so a user's own root `@document` composes with
                // library schemas pulled in by imports).
                let field_ns = self.doc.find_field_source_ns(self.ast);
                let schemas = self.doc.doc_schemas_for_ns(field_ns);
                if schemas.is_empty() {
                    Some(EvalError::schema_violation(
                        Kind::NoDocumentSchema,
                        format!("top-level field '{}' has no @document schema", self.name()),
                        self.ast.span,
                    ))
                } else if schemas.declares_field(self.name()) {
                    None
                } else {
                    Some(EvalError::schema_violation(
                        Kind::UnknownField,
                        format!(
                            "field '{}' is not declared by @document schema '{}'",
                            self.name(),
                            schemas.names()
                        ),
                        self.ast.span,
                    ))
                }
            }
        }
    }

    /// Returns the schema-declared `TypeRef` for this field, if the
    /// field lives inside a schema'd block and that schema declares
    /// it. Top-level fields and fields inside un-schema'd blocks
    /// return `None`.
    pub(in crate::doc) fn declared_type_ref(&self) -> Option<&'a TypeRef> {
        if let Some(frame) = self.scope.frames().last().cloned() {
            let block = Block {
                ast: frame.ast,
                cells: frame.cells,
                doc: self.doc,
                file_ns: frame.file_ns,
                kind_override: frame.kind_override,
                scope: Scope::root(),
            };
            let schema = block.schema()?;
            let schema_field = schema.field(self.name())?;
            return Some(schema_field.type_ref());
        }
        // Top-level field: consult the merged @document schema(s) in
        // this field's source namespace, preferring a root-authored
        // declaration over an imported one.
        let field_ns = self.doc.find_field_source_ns(self.ast);
        let schema_field = self.doc.doc_schemas_for_ns(field_ns).field(self.name())?;
        Some(schema_field.type_ref())
    }

    /// For a `&T`-typed field, return the lazy navigator pointing at
    /// the referenced target.
    ///
    /// - `None` — the field is not declared as a reference.
    /// - `Some(Ok(dr))` — the reference resolves; `dr` walks the
    ///   target the same way `Document::get` would.
    /// - `Some(Err(e))` — the field is `&T` but the target can't be
    ///   resolved through the field's scope chain.
    pub fn reference(&self) -> Option<Result<DataRef<'a>, EvalError>> {
        let declared = self.declared_type_ref()?;
        let TypeRef::Reference(inner) = declared else {
            return None;
        };
        let ctx = EvalCtx::new(self.scope.clone());
        let target_dr = match self.doc.eval_to_dataref(&self.ast.expr, &ctx) {
            Ok(d) => d,
            Err(e) => return Some(Err(e)),
        };

        // Apply interface conformance / ancestor-acceptance checks
        // only when the target has a statically-known concrete type
        // and the declared inner is a named path. For anything
        // unresolvable (raw blocks, lists, etc.) we trust the
        // navigator and skip both checks.
        if let TypeRef::Named { path, .. } = inner.as_ref()
            && let Some(target_decl) = dataref_concrete_type(&target_dr, self.doc)
        {
            let key = path.join(".");
            // Case A: interface conformance.
            if let Some(iface) = self.doc.interface(&key) {
                if let Err(e) =
                    check_interface_conformance(self.doc, &iface, &target_decl, self.ast.span)
                {
                    return Some(Err(e));
                }
            } else if let Some(expected) = self.doc.type_decl(&key)
                && !same_type_decl(&expected, &target_decl)
                && !target_decl.is_descendant_of(&expected.full_name())
            {
                // Case B: ancestor acceptance for regular types.
                return Some(Err(EvalError::schema_violation(
                    crate::error::SchemaViolationKind::InterfaceNotImplemented,
                    format!(
                        "target type '{}' is not '{}' and does not extend it",
                        target_decl.full_name(),
                        expected.full_name(),
                    ),
                    self.ast.span,
                )));
            }
        }
        Some(Ok(target_dr))
    }
}

/// View over a `let name = expr` item (top-level or block-level). A
/// composition helper resolved by name during evaluation but never
/// surfaced as document data. Its value is memoised in a [`FieldCell`]
/// with the same cycle-detection the field evaluator uses; evaluation
/// happens lazily on first name resolution.
#[derive(Clone)]
pub(crate) struct LetView<'a> {
    /// The AST node this view borrows.
    pub(in crate::doc) ast: &'a ast::LetItem,
    /// Evaluation cache for the bound expression.
    pub(in crate::doc) cell: &'a FieldCell,
    /// The document these views read through.
    pub(in crate::doc) doc: &'a Document,
    /// Scope the let's value expression is evaluated in — the
    /// declaring block's child scope (or `Scope::root()` for a
    /// top-level let), so it sees siblings and ancestors.
    pub(in crate::doc) scope: Scope<'a>,
}

impl<'a> LetView<'a> {
    /// `true` while this let's RHS is being evaluated higher up the
    /// (single-threaded) evaluation stack. Used by `scope_lookup` to
    /// give `a = a` outward-shadowing semantics: a mid-evaluation match
    /// is skipped so the name resolves to an outer binding instead of
    /// the binding being defined.
    pub(crate) fn mid_evaluation(&self) -> bool {
        self.cell.value.get().is_none() && self.cell.evaluating.load(Ordering::Acquire)
    }

    /// Evaluate (once) and return the bound value. Mirrors
    /// [`Field::value`]'s cycle-detection: a re-entrant evaluation
    /// caches and returns an `EvalError::Cycle`.
    pub(crate) fn value(&self) -> Result<Value, EvalError> {
        let cell = self.cell;
        if let Some(cached) = cell.value.get() {
            return cached.clone();
        }
        if cell.evaluating.swap(true, Ordering::Acquire) {
            let _ = cell.value.set(Err(EvalError::Cycle {
                field: self.ast.name.clone(),
                span: span_to_miette(self.ast.span),
            }));
            return cell
                .value
                .get()
                .expect("cycle cell was just initialised")
                .clone();
        }
        let result = self.doc.eval_in_scope(&self.ast.value, &self.scope);
        cell.evaluating.store(false, Ordering::Release);
        cell.value.get_or_init(|| result).clone()
    }
}
