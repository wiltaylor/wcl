//! Finding the schema that governs a thing.
//!
//! Answers "which type declaration validates this block / decorator /
//! table / document?", plus the derived schemas the language fabricates
//! for kinds no type declares directly. This is lookup only — the
//! checking itself lives in [`schema_check`](super::schema_check) and
//! [`decorators`](super::decorators).

use std::collections::HashMap;

use crate::ast::{self, synthetic_decorator, synthetic_field, synthetic_span};
use crate::value::TypeRef;

use super::cells::ItemCells;
use super::views::{Block, BuiltinDecorator, DeclName, DeclaresKind, TypeDecl};
use super::{DeclLoc, DeclaredKind, DocSchemas, Document, block_first_label, block_label_at};

impl Document {
    /// Look up the type that schemas a block of the given kind. Bare
    /// kinds resolve from the document's own namespace (local
    /// declarations preferred), so a user `@block("process")` shadows an
    /// imported one of the same kind.
    ///
    /// A kind no `@block`/`@table` type declares may still be declared
    /// by an *instance* — see
    /// [`derived_block_schema`](Self::derived_block_schema). Declared
    /// types win: the collision is reported at the declarer
    /// (`DeclaredKindCollision`) rather than silently resolved here.
    pub fn block_schema(&self, kind: &str) -> Option<TypeDecl<'_>> {
        self.find_schema(BuiltinDecorator::Block, kind)
            .or_else(|| self.derived_block_schema(kind))
    }

    /// Namespace-aware block lookup: `qualifier` is the `::` namespace
    /// (empty for bare), `context_ns` the referencing site's namespace.
    /// An instance-declared kind is unqualified — it is declared by a
    /// block, not by a namespaced type — so only a bare lookup falls
    /// back to the derivation.
    pub(crate) fn block_schema_in(
        &self,
        qualifier: &[String],
        kind: &str,
        context_ns: &[String],
    ) -> Option<TypeDecl<'_>> {
        self.find_schema_ns(BuiltinDecorator::Block, qualifier, kind, context_ns)
            .or_else(|| {
                qualifier
                    .is_empty()
                    .then(|| self.derived_block_schema(kind))
                    .flatten()
            })
    }

    /// Look up the type that schemas a decorator of the given name.
    pub fn decorator_schema(&self, name: &str) -> Option<TypeDecl<'_>> {
        self.find_schema(BuiltinDecorator::Decorator, name)
    }

    /// Decorator schemas applicable to a block of `kind`. Both ordinary
    /// `@block` kinds and instance-derived kinds resolve through
    /// [`block_schema`](Self::block_schema). An unknown kind has no
    /// applicable schemas.
    pub fn decorator_schemas_for_block_kind(&self, kind: &str) -> Vec<TypeDecl<'_>> {
        if self.block_schema(kind).is_none() {
            return Vec::new();
        }
        self.declared_decorators()
            .map(|(_, schema)| schema)
            .filter(|schema| schema.decorator_applies_to("block", Some(kind)))
            .collect()
    }

    /// Namespace-aware decorator lookup. `qualifier` is the dotted
    /// namespace written before the decorator name (empty for bare), and
    /// `context_ns` is the namespace of the site carrying the decorator.
    pub fn decorator_schema_in(
        &self,
        qualifier: &[String],
        name: &str,
        context_ns: &[String],
    ) -> Option<TypeDecl<'_>> {
        self.find_schema_ns(BuiltinDecorator::Decorator, qualifier, name, context_ns)
    }

    /// Look up the type that schemas a table of the given name, i.e.
    /// the first type carrying an `@table("name")` decorator.
    pub fn table_schema(&self, name: &str) -> Option<TypeDecl<'_>> {
        self.find_schema(BuiltinDecorator::Table, name)
    }

    /// Namespace-aware table lookup (see [`Document::block_schema_in`]).
    pub(crate) fn table_schema_in(
        &self,
        qualifier: &[String],
        name: &str,
        context_ns: &[String],
    ) -> Option<TypeDecl<'_>> {
        self.find_schema_ns(BuiltinDecorator::Table, qualifier, name, context_ns)
    }

    /// Look up the type carrying the `@document` decorator, if any.
    /// At most one is expected; if multiple are declared, this
    /// returns the first and `Document::schema_errors` surfaces a
    /// `MultipleDocumentSchemas` violation.
    pub fn doc_schema(&self) -> Option<TypeDecl<'_>> {
        self.document_schema_decls().into_iter().next()
    }

    /// Every `@document`-decorated type declaration, in `type_decls()`
    /// order. The decorator scan runs once (locations cached in
    /// `document_schema_locs`); each call rebuilds only the few
    /// matching views by direct indexing.
    pub(super) fn document_schema_decls(&self) -> Vec<TypeDecl<'_>> {
        let sources = self.all_sources();
        let locs = self.document_schema_locs.get_or_init(|| {
            let dec_name = BuiltinDecorator::Document.as_str();
            let mut out = Vec::new();
            for (si, src) in sources.iter().enumerate() {
                for (ii, (item, cells)) in src.items.iter().zip(src.cells.iter()).enumerate() {
                    let ast::Item::TypeDecl(t) = item else {
                        continue;
                    };
                    let td = TypeDecl {
                        ast: t,
                        file_ns: src.file_ns,
                        cells,
                        doc: self,
                        is_imported: src.path.is_some(),
                        is_derived: false,
                    };
                    if td.decorators().any(|d| d.full_name() == dec_name) {
                        out.push(DeclLoc::Source {
                            source: si,
                            item: ii,
                        });
                    }
                }
            }
            for (i, (t, cells)) in self
                .synthetic_types
                .iter()
                .zip(self.synthetic_type_cells.iter())
                .enumerate()
            {
                let td = TypeDecl {
                    ast: t,
                    file_ns: &[],
                    cells,
                    doc: self,
                    is_imported: false,
                    is_derived: false,
                };
                if td.decorators().any(|d| d.full_name() == dec_name) {
                    out.push(DeclLoc::Synthetic(i));
                }
            }
            out
        });
        locs.iter()
            .map(|loc| match *loc {
                DeclLoc::Source { source, item } => {
                    let src = &sources[source];
                    let ast::Item::TypeDecl(t) = &src.items[item] else {
                        unreachable!("document_schema_locs points at a TypeDecl")
                    };
                    TypeDecl {
                        ast: t,
                        file_ns: src.file_ns,
                        cells: &src.cells[item],
                        doc: self,
                        is_imported: src.path.is_some(),
                        is_derived: false,
                    }
                }
                DeclLoc::Synthetic(i) => TypeDecl {
                    ast: &self.synthetic_types[i],
                    file_ns: &[],
                    cells: &self.synthetic_type_cells[i],
                    doc: self,
                    is_imported: false,
                    is_derived: false,
                },
            })
            .collect()
    }

    /// Every `@document` schema that governs `file_ns`, root-authored
    /// first. The effective document schema for a namespace is the
    /// *merge* of these: a top-level field/block is legal if any of
    /// them declares it. This lets a user declare their own
    /// `@document` at the root that composes with library `@document`
    /// schemas pulled in by imports (from any number of modules),
    /// instead of an imported schema "taking over" the root.
    pub(crate) fn doc_schemas_for_ns(&self, file_ns: &[String]) -> DocSchemas<'_> {
        // `type_decls()` walks the root source before imports, so
        // root-authored declarations already sort ahead of imported
        // ones — preserve that order so `field()`/`primary()` prefer
        // the root-authored schema.
        // A source is governed by the `@document` schemas declared in
        // its own namespace, plus any pulled in from an imported library
        // (`is_imported`) — importing a schema library is opting its
        // `@document` into your document. This is what lets an
        // un-namespaced (or differently-namespaced) user document compose
        // against the stdlib `Site` schema, which lives in `namespace
        // wdoc`. Root-authored declarations stay namespace-scoped.
        let schemas = self
            .document_schema_decls()
            .into_iter()
            .filter(|t| t.file_ns() == file_ns || t.is_imported())
            .collect();
        DocSchemas { schemas }
    }

    /// Every type declaration carrying the named decorator. Used by
    /// the document-level validator to detect duplicate `@document`
    /// declarations.
    pub(crate) fn find_all_decorated(&self, dec: BuiltinDecorator) -> Vec<TypeDecl<'_>> {
        let dec_name = dec.as_str();
        self.type_decls()
            .filter(|t| t.decorators().any(|d| d.full_name() == dec_name))
            .collect()
    }

    /// `true` if any declared type carries `@block(kind)` or
    /// `@table(kind)`. Used to spot un-registered block kinds in the
    /// strict validator.
    pub(crate) fn is_registered_kind(&self, kind: &str) -> bool {
        self.block_schema(kind).is_some() || self.table_schema(kind).is_some()
    }

    /// The block that *declares* the kind `name`, if any: an instance of
    /// a type carrying `@declares_kind` (wdoc's `wdoc_component`, and
    /// whatever a host calls its own). A declared kind is instantiated by
    /// its own name as a bare block; this resolves that instance kind
    /// back to the declaration it came from — the params it takes and the
    /// body it expands to.
    ///
    /// Served from the once-built [`declared_kinds`](Self::declared_kinds)
    /// index — a host's expander consults this for every nested block, so
    /// a per-call label-evaluating scan over all top-level blocks would be
    /// O(blocks²) across a build.
    pub fn kind_declarer(&self, name: &str) -> Option<Block<'_>> {
        if name.is_empty() {
            return None;
        }
        let pos = self.declared_kinds()?.get(name)?.declarer?;
        self.blocks().nth(pos)
    }

    /// The `@block` schema derived from the declarer of `name`, if that
    /// kind is instance-declared. One field per declared param —
    /// optional when the param carries a `default` — so the generic
    /// `UnknownField` / `required_fields` checks apply to instances of
    /// it, and `@contextual` because what an instance emits is whatever
    /// its declarer's body expands to.
    ///
    /// Deliberately absent from [`type_decls`](Self::type_decls), which
    /// walks what the document *declares*; reach it through kind lookup
    /// ([`block_schema`](Self::block_schema)) or here.
    pub fn derived_block_schema(&self, name: &str) -> Option<TypeDecl<'_>> {
        let declared = self.declared_kinds()?.get(name)?;
        Some(TypeDecl {
            ast: &declared.ast,
            file_ns: &[],
            cells: &declared.cells,
            doc: self,
            is_imported: false,
            is_derived: true,
        })
    }

    /// Every instance-declared kind, keyed by name. `None` only for a
    /// re-entrant call from inside the derivation itself (see the
    /// `deriving` field): the caller sees a document with no declared
    /// kinds.
    ///
    /// That stand-down is invisible because of *what* runs during the
    /// build — a schema lookup per top-level block and the evaluation of
    /// a declarer's labels. Neither reports diagnostics: validation is
    /// demanded by `schema_errors` / `Block::schema_errors`, which the
    /// build never calls. A re-entrant lookup that missed *and* was
    /// being validated would report `UnregisteredKind`, so keep it that
    /// way — no diagnostic path may run inside `build_declared_kinds`.
    pub(super) fn declared_kinds(&self) -> Option<&HashMap<String, DeclaredKind>> {
        if let Some(built) = self.declared_kinds.get() {
            return Some(built);
        }
        let me = std::thread::current().id();
        {
            // A poisoned guard means a panic unwound through the few
            // lines below, not that the set is unusable: recover it
            // rather than letting an unrelated panic turn every later
            // lookup into a silent miss.
            let mut in_flight = self
                .deriving
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !in_flight.insert(me) {
                return None;
            }
        }
        let built = self.build_declared_kinds();
        self.deriving
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&me);
        // Another thread may have won the race; either result is the
        // same map, built from the same fixed inputs.
        let _ = self.declared_kinds.set(built);
        self.declared_kinds.get()
    }

    /// Scan the top-level blocks for instances of a `@declares_kind`
    /// type and derive one schema per declared kind. First declaration
    /// wins, matching every other kind lookup.
    fn build_declared_kinds(&self) -> HashMap<String, DeclaredKind> {
        let mut out: HashMap<String, DeclaredKind> = HashMap::new();
        for (i, b) in self.blocks().enumerate() {
            let Some(schema) = b.schema() else { continue };
            let Some(contract) = schema.declares_kind() else {
                continue;
            };
            let Some(name) = block_label_at(&b, contract.name_slot) else {
                continue;
            };
            if out.contains_key(&name) {
                continue;
            }
            let derived = derive_kind_schema(&b, &schema, &contract, &name);
            let cells = ItemCells::build(&ast::Item::TypeDecl(derived.clone()), None);
            out.insert(
                name,
                DeclaredKind {
                    declarer: Some(i),
                    ast: derived,
                    cells,
                },
            );
        }
        out
    }

    /// Whether `ty`, as written in `file_ns`, names a type marked
    /// `@block_slot`. The marker keeps `slot` derivation independent of any
    /// host's chosen type name (wdoc uses `content`).
    pub(super) fn type_ref_is_block_slot(&self, ty: &TypeRef, file_ns: &[String]) -> bool {
        let Some(fqn) = self.resolve_type_fqn_in(ty, file_ns) else {
            return false;
        };
        self.type_decl(&fqn).is_some_and(|decl| {
            decl.decorators()
                .any(|dec| dec.is(BuiltinDecorator::BlockSlot))
        })
    }

    /// Whether some top-level holder declares `kind` as a bare nested-block
    /// slot. This is only a generic-validation escape for otherwise
    /// *unregistered* wrapper names; it does not install a schema or decide
    /// which holder is active. The host still owns scoped contract checks.
    pub(crate) fn is_possible_block_slot_fill(&self, kind: &str) -> bool {
        self.blocks().any(|holder| {
            holder.ast.items.iter().any(|item| {
                let ast::Item::Block(slot) = item else {
                    return false;
                };
                if slot.kind != "slot" {
                    return false;
                }
                let Some(decl) = &slot.slot_decl else {
                    return false;
                };
                let names_kind = matches!(
                    slot.labels.first(),
                    Some(ast::Expr::Identifier(name, _)) if name == kind
                );
                names_kind && self.type_ref_is_block_slot(&decl.ty, holder.file_ns)
            })
        })
    }
}

/// A param block carrying a field of this name declares a fallback, so
/// the derived field is optional. The language's own word for a
/// fallback value (`@default`), asked of the declarer's own vocabulary.
const PARAM_DEFAULT_FIELD: &str = "default";

/// Derive the `@block` schema for the kind `kind`, declared by the
/// instance `declarer` (whose own type is `schema`, carrying `contract`).
///
/// One scalar field per param block, in declaration order: optional when the
/// param carries a `default`/`?`, listed in `required_fields` when it does
/// not. Typed slots preserve their `TypeRef`; legacy untyped params remain
/// permissive `@schemaless utf8`. A slot whose type carries `@block_slot` is
/// a host-checked nested-block hole and therefore emits no scalar field.
/// The type carries `@contextual`: an instance emits whatever the
/// declarer's body expands to, which is the host's business too.
fn derive_kind_schema(
    declarer: &Block<'_>,
    schema: &TypeDecl<'_>,
    contract: &DeclaresKind,
    kind: &str,
) -> ast::TypeDecl {
    let param_kind = schema
        .effective_field(&contract.params_field)
        .and_then(|f| f.children_block_kind());
    let mut fields: Vec<ast::TypeField> = Vec::new();
    let mut required: Vec<String> = Vec::new();
    if let Some(param_kind) = param_kind {
        for p in declarer
            .blocks()
            .filter(|b| b.kind() == param_kind || (param_kind == "slot" && b.kind() == "wdoc_slot"))
        {
            let Some(name) = block_first_label(&p) else {
                continue;
            };
            // Content slots describe nested block holes, not instance
            // parameter fields. Their host validates/fills them against the
            // resolved container contract at build/render time.
            if p.slot_type_ref()
                .is_some_and(|ty| p.doc.type_ref_is_block_slot(ty, p.file_ns))
            {
                continue;
            }
            if fields.iter().any(|f| f.name == name) {
                continue;
            }
            let optional = p.field(PARAM_DEFAULT_FIELD).is_some() || p.slot_optional();
            if !optional {
                required.push(name.clone());
            }
            let typed = p.slot_type_ref().cloned();
            let mut field = synthetic_field(
                &name,
                typed
                    .clone()
                    .unwrap_or(TypeRef::Builtin(crate::value::BuiltinType::Utf8)),
                optional,
            );
            // Legacy `wdoc_slot` declarations were deliberately untyped.
            // Preserve that contract while typed `slot` declarations use
            // ordinary schema checking.
            if typed.is_none() {
                field.decorators.push(synthetic_decorator(
                    BuiltinDecorator::Schemaless.as_str(),
                    Vec::new(),
                ));
            }
            fields.push(field);
        }
    }

    let mut block_dec = synthetic_decorator(
        BuiltinDecorator::Block.as_str(),
        vec![ast::Expr::Utf8(kind.to_string())],
    );
    if !required.is_empty() {
        block_dec.named.push(ast::NamedArg {
            name: "required_fields".to_string(),
            value: ast::Expr::ListLit {
                elements: required.into_iter().map(ast::Expr::Utf8).collect(),
                elem_trivia: Vec::new(),
                trailing_trivia: Vec::new(),
                span: synthetic_span(),
            },
            span: synthetic_span(),
            leading_trivia: Vec::new(),
            trailing_comment: None,
        });
    }

    ast::TypeDecl {
        name: vec![kind.to_string()],
        extends: Vec::new(),
        alias: None,
        fields,
        decorators: vec![
            block_dec,
            synthetic_decorator(BuiltinDecorator::Contextual.as_str(), Vec::new()),
        ],
        span: declarer.span(),
        leading_trivia: Vec::new(),
        trailing_comment: None,
        trailing_trivia: Vec::new(),
    }
}
