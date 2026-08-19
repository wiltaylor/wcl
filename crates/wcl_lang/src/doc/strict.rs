//! Strict-mode validation: every schema violation in the document, in
//! one pass.
//!
//! The lazy path checks a field when someone reads it. This walks the
//! whole document up front and reports everything —
//! [`Document::schema_errors`] for violations, and
//! [`Document::schema_warnings`] for the advisory siblings a host
//! surfaces without failing. `wcl check` is the caller that wants both.
//!
//! The two paths must agree: anything reported here as an *error* must
//! also be raised by the lazy check when that field is read. Warnings
//! are exempt from that contract by construction — nothing about
//! reading one field can detect that two `@document` schemas shadow
//! each other.
//!
//! Checking one block is [`schema_check`](super::schema_check)'s job;
//! this module decides what to check, and pairs each violation with the
//! source it should be rendered against.

use std::collections::HashSet;
use std::path::Path;

use miette::NamedSource;

use crate::ast::TypeRef;
use crate::diagnostics::EvalError;
use crate::value::Value;

use super::cells::ItemCellKind;
use super::decorators::decorator_name_span;
use super::lookup::{iter_blocks, iter_fields};
use super::schema_check;
use super::schema_check::{has_annotation_exemption, has_schemaless};
use super::schema_lookup::{DeclLoc, DocSchemas};
use super::scope::Scope;
use super::types::variant_dispatch;
use super::types::{
    format_union_variants_hint, symbol_set_membership_error_in, validate_union,
    value_matches_type_ref,
};
use super::views::{Block, BuiltinDecorator, DeclName, Field, TypeDecl, TypeField, UnionDecl};
use super::{CollectedSchemaErrors, Document};

impl Document {
    /// Non-fatal schema diagnostics — advisory siblings of
    /// [`schema_errors`](Self::schema_errors) that hosts surface without
    /// failing (the CLI prints them, builds ignore them, the LSP maps
    /// them to `Warning` severity).
    ///
    /// Currently detects **gather-field shadowing** between `@document`
    /// schemas ([`SchemaViolationKind::DocumentFieldShadow`]): two
    /// schemas that co-govern a namespace declaring the same field name
    /// where at least one side is a `@child`/`@children` gather slot.
    /// The merge (`doc_schemas_for_ns`) resolves such a name first-wins,
    /// so the shadowed schema's gathered blocks silently vanish from any
    /// template iterating the field — the failure mode that once forced a
    /// real schema to rename its `component` gather to `sw_components`.
    /// Pairs are checked across *all* co-governing declarations (not
    /// just root-vs-imported): `is_imported` means "from an imported
    /// file", so two library schemas (e.g. wdoc's `Site` and a base
    /// schema imported from disk) can shadow each other too. Scalar
    /// fields colliding with scalar fields are deliberately not
    /// reported — only gather slots break silently.
    ///
    /// The lazy validation path (`Field::schema_membership_error`) is
    /// intentionally untouched: the strict/lazy agreement contract
    /// covers membership *errors* only.
    pub fn schema_warnings(&self) -> Vec<EvalError> {
        use crate::diagnostics::SchemaViolationKind as Kind;
        let mut out = Vec::new();

        let decls = self.document_schema_decls();
        // `document_schema_decls` just built the location cache; zip
        // each decl with its source path so messages can name files
        // (the `TypeDecl` view itself doesn't carry one).
        let sources = self.all_sources();
        let locs = self
            .document_schema_locs
            .get()
            .expect("document_schema_decls built the cache");
        let paths: Vec<Option<&Path>> = locs
            .iter()
            .map(|loc| match *loc {
                DeclLoc::Source { source, .. } => sources[source].path,
                DeclLoc::Synthetic(_) => None,
            })
            .collect();

        let is_gather = |f: &TypeField<'_>| {
            f.child_kind_or_union().is_some() || f.children_kind_or_union().is_some()
        };
        let file_of = |p: Option<&Path>| match p {
            Some(p) => format!(" ({})", p.display()),
            None => " (this document)".to_string(),
        };

        let mut seen: HashSet<(usize, String)> = HashSet::new();
        for (bi, b) in decls.iter().enumerate() {
            for (ai, a) in decls.iter().enumerate().take(bi) {
                // Co-governance mirrors `doc_schemas_for_ns`: an
                // imported `@document` governs every namespace, a
                // root-authored one only its own.
                let co_govern = a.is_imported() || b.is_imported() || a.file_ns() == b.file_ns();
                if !co_govern {
                    continue;
                }
                for fb in b.fields() {
                    let Some(fa) = a.field(fb.name()) else {
                        continue;
                    };
                    if !is_gather(&fa) && !is_gather(&fb) {
                        continue;
                    }
                    // Anchor at the root-authored side when exactly one
                    // side is root-authored (that span is valid against
                    // the root source, which is what the CLI snippet
                    // renderer and the LSP have open); otherwise at the
                    // later declaration (in practice the schema the
                    // user imported last).
                    let (anchor_idx, anchor_field, other, other_idx) =
                        if !a.is_imported() && b.is_imported() {
                            (ai, fa, b, bi)
                        } else {
                            (bi, fb, a, ai)
                        };
                    if !seen.insert((anchor_idx, anchor_field.name().to_string())) {
                        continue;
                    }
                    let anchored = decls[anchor_idx];
                    out.push(EvalError::schema_violation_named(
                        Kind::DocumentFieldShadow,
                        format!(
                            "gather field '{name}' of @document '{anchored_fqn}'{anchored_file} \
                             collides with '{name}' declared by @document '{other_fqn}'{other_file} \
                             — the merged document schema resolves '{name}' to only one \
                             declaration, so the other schema's gathered blocks silently \
                             vanish; rename one field",
                            name = anchor_field.name(),
                            anchored_fqn = anchored.full_name(),
                            anchored_file = file_of(paths[anchor_idx]),
                            other_fqn = other.full_name(),
                            other_file = file_of(paths[other_idx]),
                        ),
                        anchor_field.name(),
                        anchor_field.span(),
                    ));
                }
            }
        }
        out
    }
    /// Strict-mode validation: returns every schema violation across
    /// the document.
    ///
    /// At the top level: detects multiple `@document` declarations,
    /// top-level fields/blocks without a matching schema, and
    /// top-level block kinds that aren't registered via
    /// `@block`/`@table`. Each top-level block then has its
    /// `Block::schema_errors()` collected recursively.
    pub fn schema_errors(&self) -> Vec<EvalError> {
        self.collect_schema_errors()
            .into_iter()
            .map(|(error, _)| error)
            .collect()
    }

    /// Strict-mode validation paired with a source when its provenance is
    /// known. Hosts should omit a snippet for `None` rather than attach the
    /// root document to a recursively-produced cross-file error.
    pub fn schema_diagnostics(&self) -> Vec<(EvalError, Option<NamedSource<String>>)> {
        self.collect_schema_errors()
    }

    /// Run the full schema validation pass, pairing each violation
    /// with the source it should be rendered against.
    fn collect_schema_errors(&self) -> CollectedSchemaErrors {
        use crate::diagnostics::SchemaViolationKind as Kind;
        use std::collections::BTreeMap;
        let mut out = Vec::new();

        // Group every `@document` type by its file_ns. Each namespace
        // is independent. Several `@document` schemas may govern one
        // namespace and they *compose* (a root-authored one plus any
        // library-provided ones from imports) — see
        // `doc_schemas_for_ns`. Only a second *root-authored*
        // `@document` in a namespace is an error: imported library
        // schemas merge silently, so importing several modules that
        // each ship a base document is fine.
        let doc_schemas = self.document_schema_decls();
        let mut by_ns: BTreeMap<Vec<String>, Vec<TypeDecl<'_>>> = BTreeMap::new();
        for d in &doc_schemas {
            by_ns.entry(d.file_ns().to_vec()).or_default().push(*d);
        }
        for decls in by_ns.values() {
            for extra in decls.iter().filter(|d| !d.is_imported()).skip(1) {
                out.push((
                    EvalError::schema_violation(
                        Kind::MultipleDocumentSchemas,
                        format!(
                            "type '{}' declares a second root @document schema \
                         (only one root-authored @document is allowed per namespace; \
                         imported library schemas merge automatically)",
                            extra.name()
                        ),
                        extra.span(),
                    ),
                    Some(self.root_named_source()),
                ));
            }
        }

        // Duplicate kind detection for `@block`/`@table`/`@decorator`.
        // Two declarations sharing a kind string *within one namespace*
        // are ambiguous; across namespaces a `::` qualifier disambiguates,
        // so those are fine. Mirroring the `@document` rule, only a second
        // *root-authored* declaration in a (namespace, kind) group is an
        // error — imported library duplicates resolve first-wins silently.
        for dec in [
            BuiltinDecorator::Block,
            BuiltinDecorator::Table,
            BuiltinDecorator::Decorator,
        ] {
            let dec_name = dec.as_str();
            let mut by_kind: BTreeMap<(Vec<String>, String), Vec<TypeDecl<'_>>> = BTreeMap::new();
            for t in self.find_all_decorated(dec) {
                let Some(kind) = t.decorators().find_map(|d| {
                    if d.full_name() != dec_name {
                        return None;
                    }
                    match d.positional().ok()?.into_iter().next() {
                        Some(Value::Utf8(s)) => Some(s),
                        _ => None,
                    }
                }) else {
                    continue;
                };
                by_kind
                    .entry((t.file_ns().to_vec(), kind))
                    .or_default()
                    .push(t);
            }
            for ((_, kind), decls) in &by_kind {
                for extra in decls.iter().filter(|d| !d.is_imported()).skip(1) {
                    out.push((
                        EvalError::schema_violation(
                            Kind::DuplicateBlockKind,
                            format!(
                                "type '{}' redeclares @{dec_name}(\"{kind}\") already declared \
                             in this namespace (kinds must be unique per namespace; \
                             qualify across namespaces with `::`)",
                                extra.name()
                            ),
                            extra.span(),
                        ),
                        Some(self.root_named_source()),
                    ));
                }
            }
        }

        // An instance-declared kind (`@declares_kind`) whose name matches
        // a declared `@block`/`@table` kind is incoherent: expansion
        // dispatches the kind to the declarer while schema lookup
        // validates instances against the declared type (and a host may
        // special-case the kind in Rust, ignoring the declarer
        // entirely). The collision itself is the error, reported at the
        // declaration. Both sides are looked up *without* the
        // derivation, so a declared kind never collides with itself.
        let declared_kinds: Vec<String> = self
            .declared_kinds()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        for name in declared_kinds {
            let colliding = self
                .find_schema(BuiltinDecorator::Block, &name)
                .or_else(|| self.find_schema(BuiltinDecorator::Table, &name));
            let Some(t) = colliding else { continue };
            let Some(declarer) = self.kind_declarer(&name) else {
                continue;
            };
            out.push((
                EvalError::schema_violation(
                    Kind::DeclaredKindCollision,
                    format!(
                        "block '{}' declares the kind '{name}', which collides with the \
                     @block/@table kind \"{name}\" declared by type '{}' — rename it",
                        declarer.kind(),
                        t.full_name()
                    ),
                    declarer.span(),
                ),
                Some(declarer.named_source()),
            ));
        }

        // Walk every source (the main file + every eagerly-imported
        // file). Each source's items are validated against the merged
        // `@document` schema for that source's namespace, if any.
        for src in self.all_sources() {
            let mut errors = Vec::new();
            let schemas = self.doc_schemas_for_ns(src.file_ns);

            // Pre-compute the merged schema's union-typed children
            // slots so structurally-matched blocks bypass the kind
            // check.
            let root_union_slots = schemas.union_slots();

            // Walk the top-level fields in this source.
            for f in iter_fields(src.items, src.cells, self, src.file_ns, Scope::root()) {
                if has_schemaless(&f.ast.decorators) {
                    continue;
                }
                self.validate_root_field(&f, &schemas, &mut errors);
            }

            // Walk the top-level blocks in this source.
            for b in iter_blocks(src.items, src.cells, self, src.file_ns, Scope::root()) {
                if has_schemaless(&b.ast.decorators) {
                    continue;
                }
                self.validate_root_block(&b, &schemas, &root_union_slots, &mut errors);
            }
            let source = self.named_source_for_view(src);
            out.extend(errors.into_iter().map(|error| {
                let is_undeclared_decorator = matches!(
                    &error,
                    EvalError::SchemaViolation {
                        kind: Kind::UndeclaredDecorator,
                        ..
                    }
                );
                let error_source = is_undeclared_decorator.then(|| source.clone());
                (error, error_source)
            }));
        }

        // Duplicate identity labels among top-level blocks, grouped per
        // namespace — every source of one namespace feeds the same
        // gather lists, so two `component engine { }` blocks in
        // different data files silently coexist without this. Only
        // kinds whose schema declares an `@inline(0) identifier` label
        // participate (see `schema_check::identity_label`); nested
        // duplicates are caught per parent in `compute_schema_errors`.
        {
            let mut by_ns: BTreeMap<Vec<String>, Vec<Block<'_>>> = BTreeMap::new();
            for src in self.all_sources() {
                for b in iter_blocks(src.items, src.cells, self, src.file_ns, Scope::root()) {
                    if has_schemaless(&b.ast.decorators) {
                        continue;
                    }
                    by_ns.entry(src.file_ns.to_vec()).or_default().push(b);
                }
            }
            for blocks in by_ns.into_values() {
                out.extend(
                    crate::doc::schema_check::duplicate_id_errors(blocks.into_iter())
                        .into_iter()
                        .map(|(error, block)| (error, Some(block.named_source()))),
                );
            }
        }

        // Validate every union declaration: cycles in the extends
        // chain, duplicate variants across that chain, and structural
        // collisions between variant bodies.
        for u in self.union_decls() {
            let source = self.named_source_for_union(u.ast);
            out.extend(
                validate_union(self, u.ast)
                    .into_iter()
                    .map(|error| (error, Some(source.clone()))),
            );
        }

        // Top-level connection statements: dispatch and kind checks.
        for src in self.all_sources() {
            let errors = crate::doc::schema_check::validate_connection_stmts(
                self,
                src.items,
                &Scope::root(),
            );
            let source = self.named_source_for_view(src);
            out.extend(
                errors
                    .into_iter()
                    .map(|error| (error, Some(source.clone()))),
            );
        }

        // Applicability and cardinality share one grammar-shaped walk so
        // every decorator-bearing position is covered by the same rules.
        // Follow each authored import edge from the root exactly once. This
        // reaches eager and lazy block imports without re-walking eager
        // sources already exposed by `all_sources()`.
        let mut decorated_imports = HashSet::new();
        let root_source = self.root_named_source();
        self.validate_decorators_in_items(
            &self.ast.items,
            &self.cells.items,
            &self.file_ns,
            &mut decorated_imports,
            &root_source,
            &mut out,
        );
        for (declaration, cells) in self.synthetic_types.iter().zip(&self.synthetic_type_cells) {
            let mut errors = Vec::new();
            self.validate_decorator_group(
                &declaration.decorators,
                &cells.decorators,
                "type",
                None,
                &[],
                &mut errors,
            );
            let ItemCellKind::TypeDecl { field_decorators } = &cells.kind else {
                unreachable!("synthetic type cells mirror the declaration")
            };
            for (field, decorator_cells) in declaration.fields.iter().zip(field_decorators) {
                self.validate_decorator_group(
                    &field.decorators,
                    decorator_cells,
                    "type_field",
                    None,
                    &[],
                    &mut errors,
                );
            }
            out.extend(errors.into_iter().map(|error| (error, None)));
        }

        // Validate the applicability declarations themselves before using
        // them to check decorator occurrences.
        for declaration in self.type_decls() {
            let mut errors = Vec::new();
            let declares_decorator = declaration
                .decorators()
                .any(|decorator| decorator.full_name() == "decorator");
            for applies_to in declaration
                .decorators()
                .filter(|decorator| decorator.full_name() == "applies_to")
            {
                if !declares_decorator {
                    EvalError::push_schema_violation(
                        &mut errors,
                        Kind::InvalidDecoratorApplicability,
                        "@applies_to is attached to no decorator schema",
                        decorator_name_span(&applies_to),
                    );
                }
                let positions = applies_to
                    .named_arg("on")
                    .and_then(Result::ok)
                    .and_then(|value| match value {
                        Value::List(values) => Some(values),
                        _ => None,
                    });
                if let Some(positions) = &positions
                    && let Some(position_set) = self.symbol_set("DecoratorPosition")
                {
                    let on_span = applies_to
                        .named()
                        .find(|arg| arg.name() == "on")
                        .map(|arg| arg.span())
                        .unwrap_or_else(|| decorator_name_span(&applies_to));
                    for position in positions.iter().filter_map(|value| match value {
                        Value::Symbol(position) => Some(position.as_str()),
                        _ => None,
                    }) {
                        if !position_set.has(position) {
                            EvalError::push_schema_violation(
                                &mut errors,
                                Kind::InvalidDecoratorApplicability,
                                format!("unknown decorator position '{position}'"),
                                on_span,
                            );
                        }
                    }
                }
                let kinds_arg = applies_to.named().find(|arg| arg.name() == "kinds");
                if let (Some(positions), Some(kinds_arg)) = (positions, kinds_arg) {
                    let includes_block = positions.iter().any(
                        |value| matches!(value, Value::Symbol(position) if position == "block"),
                    );
                    if !includes_block {
                        EvalError::push_schema_violation(
                            &mut errors,
                            Kind::InvalidDecoratorApplicability,
                            "@applies_to 'kinds' requires the 'block' position in 'on'",
                            kinds_arg.span(),
                        );
                    }
                    if let Ok(Value::List(kinds)) = kinds_arg.value() {
                        for kind in kinds.iter().filter_map(|value| match value {
                            Value::Utf8(kind) => Some(kind.as_str()),
                            _ => None,
                        }) {
                            if self
                                .block_schema_in(&[], kind, declaration.file_ns())
                                .is_none()
                            {
                                EvalError::push_schema_violation(
                                    &mut errors,
                                    Kind::InvalidDecoratorApplicability,
                                    format!("@applies_to names unknown block kind '{kind}'"),
                                    kinds_arg.span(),
                                );
                            }
                        }
                    }
                }
            }
            let source = self.named_source_for_type(declaration.ast);
            out.extend(
                errors
                    .into_iter()
                    .map(|error| (error, Some(source.clone()))),
            );
        }

        out
    }

    /// Validate one top-level field against the merged `@document`
    /// schema(s) for its namespace, pushing any violation into `out`.
    /// The field is legal if any member schema declares it; the
    /// declaring schema (root-authored preferred) drives the type
    /// check. Split out of [`schema_errors`] for readability.
    fn validate_root_field(
        &self,
        f: &Field<'_>,
        schemas: &DocSchemas<'_>,
        out: &mut Vec<EvalError>,
    ) {
        use crate::diagnostics::SchemaViolationKind as Kind;
        if !has_annotation_exemption(&f.ast.decorators) {
            for decorator in f.decorators() {
                out.extend(schema_check::decorator_argument_errors(self, &decorator));
            }
        }
        if schemas.is_empty() {
            EvalError::push_schema_violation(
                out,
                Kind::NoDocumentSchema,
                format!("top-level field '{}' has no @document schema", f.name()),
                f.span(),
            );
            return;
        }
        let Some(declared) = schemas.field(f.name()) else {
            EvalError::push_schema_violation(
                out,
                Kind::UnknownField,
                format!(
                    "top-level field '{}' is not declared by @document schema '{}'",
                    f.name(),
                    schemas.names()
                ),
                f.span(),
            );
            return;
        };
        let Ok(v) = f.value() else {
            return;
        };
        // An optional field written out as `none` is absent, not
        // ill-typed: there is no value left to check against the
        // declared type, a symbol set, or a `@min`/`@non_empty` bound.
        if declared.optional() && matches!(v, Value::None) {
            return;
        }
        if let TypeRef::Named { path, .. } = declared.type_ref()
            && let Some(union_decl) = self.union_decl(&path.join("."))
        {
            if let Value::Variant { union, variant, .. } = v
                && union != &union_decl.ast.name
            {
                EvalError::push_schema_violation(
                    out,
                    Kind::VariantUnionMismatch,
                    format!(
                        "field '{}' declared as union '{}' but value is {}::{}",
                        f.name(),
                        union_decl.ast.name.join("."),
                        union.join("."),
                        variant,
                    ),
                    f.span(),
                );
            } else if let Value::Record { .. } = v {
                EvalError::push_schema_violation(
                    out,
                    Kind::VariantNoMatch,
                    format!(
                        "field '{}' declared as union '{}' but value is an \
                         un-inferred record (no variant matches its shape)",
                        f.name(),
                        union_decl.ast.name.join("."),
                    ),
                    f.span(),
                );
            }
        } else if !value_matches_type_ref(
            v,
            &self.resolve_alias_in(declared.type_ref(), declared.file_ns),
        ) {
            EvalError::push_schema_violation(
                out,
                Kind::FieldTypeMismatch,
                format!(
                    "field '{}' declared as {} but value is {}",
                    f.name(),
                    declared.type_ref(),
                    v.type_name(),
                ),
                f.span(),
            );
        } else if let Some(err) = symbol_set_membership_error_in(
            self,
            &self.resolve_alias_in(declared.type_ref(), declared.file_ns),
            v,
            f.name(),
            f.span(),
            declared.file_ns,
        ) {
            out.push(err);
        } else if let Some(msg) = schema_check::constraint_violation(
            self,
            &declared.ast.decorators,
            declared.type_ref(),
            declared.file_ns,
            v,
        ) {
            EvalError::push_schema_violation(
                out,
                Kind::ConstraintViolation,
                format!("field '{}': {msg}", f.name()),
                f.span(),
            );
        } else if let Some(msg) = schema_check::ref_violation(self, &declared, v) {
            EvalError::push_schema_violation(
                out,
                Kind::DanglingReference,
                format!("field '{}': {msg}", f.name()),
                f.span(),
            );
        }
    }

    /// Validate one top-level block: union dispatch, kind registration, and
    /// allowed-child placement under the merged `@document` schema(s) for the
    /// namespace. `root_union_slots` are those schemas' union-typed
    /// `@children` slots (a structurally-matched block bypasses the kind
    /// check). The block is legal if any member schema allows its kind.
    /// Split out of [`schema_errors`].
    fn validate_root_block(
        &self,
        b: &Block<'_>,
        schemas: &DocSchemas<'_>,
        root_union_slots: &[UnionDecl<'_>],
        out: &mut Vec<EvalError>,
    ) {
        use crate::diagnostics::SchemaViolationKind as Kind;
        let dispatched_through_union = root_union_slots
            .iter()
            .any(|u| variant_dispatch::block_to_variant(self, b, *u).is_ok());
        if dispatched_through_union {
            return;
        }
        if !self.is_registered_kind(b.kind()) {
            if self.is_possible_block_slot_fill(b.kind()) {
                return;
            }
            let mut msg = format!(
                "block kind '{}' has no @block or @table declaration",
                b.kind()
            );
            if !root_union_slots.is_empty() {
                let variants = format_union_variants_hint(self, root_union_slots);
                if !variants.is_empty() {
                    msg.push_str(&format!(" (nearby @children union accepts: {variants})"));
                }
            }
            EvalError::push_schema_violation(out, Kind::UnregisteredKind, msg, b.span());
            return;
        }
        if !schemas.is_empty() {
            let allowed = schemas.allowed_child_kinds();
            if !allowed.iter().any(|k| k == b.kind()) {
                EvalError::push_schema_violation(
                    out,
                    Kind::DisallowedChild,
                    format!(
                        "block kind '{}' is not allowed at the document root by @document schema '{}'",
                        b.kind(),
                        schemas.names()
                    ),
                    b.span(),
                );
            }
        } else {
            EvalError::push_schema_violation(
                out,
                Kind::NoDocumentSchema,
                format!("top-level block '{}' has no @document schema", b.kind()),
                b.span(),
            );
        }
        for e in b.schema_errors() {
            out.push(e.clone());
        }
    }
}
