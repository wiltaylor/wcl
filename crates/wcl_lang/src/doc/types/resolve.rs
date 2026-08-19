//! Name resolution: which declaration does this type name point at?
//!
//! Three layers, innermost first. [`Document::resolve_path_in`] turns a
//! written path into a fully-qualified one, following the `use` aliases
//! visible in the namespace the path was written in. The `*_decl`
//! lookups turn an FQN into the declaration view that owns it. And
//! [`Document::resolve`] puts the two together, mapping a whole
//! [`TypeRef`] onto a [`ResolvedType`].
//!
//! Aliases are *not* peeled here: a name that points at `type Port = u16`
//! resolves to the `Port` declaration, not to `u16`. That is
//! [`alias`](super::alias)'s job.

use std::collections::HashSet;

use crate::ast::{self, TypeRef};
use crate::symbols::SymbolKind;

use crate::doc::validate::{decl_fqn_matches, resolve_path};
use crate::doc::views::{
    BuiltinDecorator, ConnectionDecl, InterfaceDecl, SymbolSetDecl, TypeDecl, UnionDecl,
};
use crate::doc::{Document, first_positional_utf8};

use super::ResolvedType;

/// Scan every source for the first declaration matching `$fqn` whose symbol
/// kind, AST item, and view struct all share `$variant`'s name, returning the
/// constructed view from the enclosing fn on a hit. The macro emits only the
/// search loop — the caller supplies the miss tail (`None`, or a synthetic
/// fallback). `cells`/`nocells` selects whether the view carries a `cells`
/// borrow; the optional trailing field names are `TypeDecl`'s two origin
/// flags — `is_imported`, set from the source's import origin, and
/// `is_derived`, false for every declaration a source *writes*. Collapses
/// the five `type_decl`/`interface`/`union_decl`/`symbol_set`/
/// `connection_decl` lookups (M4).
macro_rules! find_decl {
    ($self:ident, $fqn:ident, $variant:ident, cells $(, $imp:ident, $der:ident)?) => {
        for src in $self.all_sources() {
            if let Some(rec) = src.symbols.lookup($fqn)
                && matches!(rec.kind, SymbolKind::$variant)
                && let ast::Item::$variant(node) = &src.items[rec.path.item_index]
            {
                return Some($variant {
                    ast: node,
                    file_ns: src.file_ns,
                    cells: &src.cells[rec.path.item_index],
                    doc: $self,
                    $( $imp: src.path.is_some(), $der: false, )?
                });
            }
        }
    };
    ($self:ident, $fqn:ident, $variant:ident, nocells) => {
        for src in $self.all_sources() {
            if let Some(rec) = src.symbols.lookup($fqn)
                && matches!(rec.kind, SymbolKind::$variant)
                && let ast::Item::$variant(node) = &src.items[rec.path.item_index]
            {
                return Some($variant {
                    ast: node,
                    file_ns: src.file_ns,
                    doc: $self,
                });
            }
        }
    };
}

/// Iterate every `$variant` declaration across the document and its eager
/// imports, in source order, yielding the matching `cells`-carrying view.
/// Collapses the `interfaces`/`union_decls`/`symbol_sets` iterators (M4).
macro_rules! decl_iter_cells {
    ($self:ident, $variant:ident) => {{
        let doc = $self;
        doc.all_sources().into_iter().flat_map(move |src| {
            src.items
                .iter()
                .zip(src.cells.iter())
                .filter_map(move |(item, cells)| match item {
                    ast::Item::$variant(node) => Some($variant {
                        ast: node,
                        file_ns: src.file_ns,
                        cells,
                        doc,
                    }),
                    _ => None,
                })
        })
    }};
}

impl Document {
    /// Run the name-resolution algorithm on `path` against this document's
    /// root file ns / aliases / wildcards / registry.
    pub(in crate::doc) fn resolve_path(&self, path: &[String]) -> Option<Vec<String>> {
        self.resolve_path_in(path, &self.file_ns)
    }

    /// Resolve `path` as if it were written in a source whose namespace
    /// is `file_ns`. This makes a bare reference resolve **within its
    /// declaring file's namespace first** — e.g. a stdlib type's
    /// `extends ContentBlock` (written in `namespace wdoc`) resolves to
    /// `wdoc.ContentBlock`, not the root namespace. The document's
    /// `use`-aliases/wildcards still apply (they only come from the root
    /// today).
    pub(crate) fn resolve_path_in(
        &self,
        path: &[String],
        file_ns: &[String],
    ) -> Option<Vec<String>> {
        let registry = self.ref_registry();
        // `self.wildcards` already includes every imported library's
        // namespace (added in `validate_document`), so a bare reference
        // to a stdlib type resolves through it.
        resolve_path(
            path,
            file_ns,
            &self.item_aliases,
            &self.ns_aliases,
            &self.wildcards,
            registry,
        )
    }

    /// Resolve a `TypeRef::Named` (or `TypeRef::Reference(Named ...)`,
    /// which is how interface-typed connection endpoints must be
    /// written) to its dotted FQN. Returns `None` for builtins,
    /// lists, tensors, etc.
    ///
    /// The reference resolves as if written in a source whose namespace
    /// is `file_ns` — e.g. a `connection X : Adr -> Adr` declared under
    /// `namespace lib` resolves its endpoints to `lib.Adr`.
    pub(crate) fn resolve_type_fqn_in(&self, t: &TypeRef, file_ns: &[String]) -> Option<String> {
        match t {
            TypeRef::Named { path, .. } => self.resolve_path_in(path, file_ns).map(|p| p.join(".")),
            TypeRef::Reference(inner) => self.resolve_type_fqn_in(inner, file_ns),
            _ => None,
        }
    }

    /// The `type` declaration with this fully-qualified name.
    pub fn type_decl(&self, fqn: &str) -> Option<TypeDecl<'_>> {
        find_decl!(self, fqn, TypeDecl, cells, is_imported, is_derived);
        // Synthetic types live in the root namespace (no file ns prefix)
        // and are not registered in the parser-built index.
        let target: Vec<&str> = fqn.split('.').collect();
        self.synthetic_types
            .iter()
            .enumerate()
            .find(|(_, t)| decl_fqn_matches(&t.name, &target))
            .map(|(i, t)| TypeDecl {
                ast: t,
                file_ns: &[],
                cells: &self.synthetic_type_cells[i],
                doc: self,
                is_imported: false,
                is_derived: false,
            })
    }

    /// Every `type` declaration in scope, synthetic ones included.
    pub fn type_decls(&self) -> impl Iterator<Item = TypeDecl<'_>> + '_ {
        let doc = self;
        let mine_and_imports = self.all_sources().into_iter().flat_map(move |src| {
            src.items
                .iter()
                .zip(src.cells.iter())
                .filter_map(move |(item, cells)| match item {
                    ast::Item::TypeDecl(t) => Some(TypeDecl {
                        ast: t,
                        file_ns: src.file_ns,
                        cells,
                        doc,
                        is_imported: src.path.is_some(),
                        is_derived: false,
                    }),
                    _ => None,
                })
        });
        let syn = self
            .synthetic_types
            .iter()
            .zip(self.synthetic_type_cells.iter())
            .map(move |(t, cells)| TypeDecl {
                ast: t,
                file_ns: &[],
                cells,
                doc,
                is_imported: false,
                is_derived: false,
            });
        mine_and_imports.chain(syn)
    }

    /// Every declared decorator and the type that schemas it, in
    /// [`type_decls`](Self::type_decls) order. This includes declarations
    /// from eager imports and synthetic types supplied by the environment.
    pub fn declared_decorators(&self) -> impl Iterator<Item = (String, TypeDecl<'_>)> + '_ {
        self.type_decls().flat_map(|schema| {
            let names: Vec<_> = schema
                .decorators()
                .filter_map(|decorator| {
                    if decorator.is(BuiltinDecorator::Decorator) {
                        first_positional_utf8(&decorator)
                    } else {
                        None
                    }
                })
                .collect();
            names.into_iter().map(move |name| (name, schema))
        })
    }

    /// Look up an interface declaration by fully-qualified name.
    /// Mirrors `type_decl` / `union_decl`.
    pub fn interface(&self, fqn: &str) -> Option<InterfaceDecl<'_>> {
        find_decl!(self, fqn, InterfaceDecl, cells);
        None
    }

    /// Every `interface` declaration in scope.
    pub fn interfaces(&self) -> impl Iterator<Item = InterfaceDecl<'_>> + '_ {
        decl_iter_cells!(self, InterfaceDecl)
    }

    /// Look up a union by fully-qualified name (dotted).
    /// The union a source-written type path resolves to (root-namespace
    /// context), memoised per path — see the `union_path_memo` field.
    /// Returns the resolved FQN; look the decl up with [`union_decl`].
    pub(crate) fn union_fqn_for_path(&self, path: &[String]) -> Option<String> {
        if let Some(hit) = self.union_path_memo.read().ok()?.get(path) {
            return hit.clone();
        }
        let resolved = self
            .resolve_path_in(path, &self.file_ns)
            .map(|p| p.join("."))
            .unwrap_or_else(|| path.join("."));
        let fqn = if self.union_decl(&resolved).is_some() {
            Some(resolved)
        } else {
            let raw = path.join(".");
            if raw != resolved && self.union_decl(&raw).is_some() {
                Some(raw)
            } else {
                None
            }
        };
        if let Ok(mut memo) = self.union_path_memo.write() {
            memo.insert(path.to_vec(), fqn.clone());
        }
        fqn
    }

    /// The `union` declaration with this fully-qualified name.
    pub fn union_decl(&self, fqn: &str) -> Option<UnionDecl<'_>> {
        find_decl!(self, fqn, UnionDecl, cells);
        None
    }

    /// Every `union` declaration in scope.
    pub fn union_decls(&self) -> impl Iterator<Item = UnionDecl<'_>> + '_ {
        decl_iter_cells!(self, UnionDecl)
    }

    /// Look up a connection schema by fully-qualified name (dotted).
    pub fn connection_decl(&self, fqn: &str) -> Option<ConnectionDecl<'_>> {
        find_decl!(self, fqn, ConnectionDecl, nocells);
        None
    }

    /// Every `connection` declaration in scope.
    pub fn connection_decls(&self) -> impl Iterator<Item = ConnectionDecl<'_>> + '_ {
        let doc = self;
        self.all_sources().into_iter().flat_map(move |src| {
            src.items.iter().filter_map(move |item| match item {
                ast::Item::ConnectionDecl(c) => Some(ConnectionDecl {
                    ast: c,
                    file_ns: src.file_ns,
                    doc,
                }),
                _ => None,
            })
        })
    }

    /// The `symbol_set` with this fully-qualified name.
    pub fn symbol_set(&self, fqn: &str) -> Option<SymbolSetDecl<'_>> {
        find_decl!(self, fqn, SymbolSetDecl, cells);
        let target: Vec<&str> = fqn.split('.').collect();
        self.synthetic_symbol_sets
            .iter()
            .enumerate()
            .find(|(_, set)| decl_fqn_matches(&set.name, &target))
            .map(|(index, set)| SymbolSetDecl {
                ast: set,
                file_ns: &[],
                cells: &self.synthetic_symbol_set_cells[index],
                doc: self,
            })
    }

    /// Every `symbol_set` in scope, synthetic ones included.
    pub fn symbol_sets(&self) -> impl Iterator<Item = SymbolSetDecl<'_>> + '_ {
        let doc = self;
        let authored = decl_iter_cells!(self, SymbolSetDecl);
        let synthetic = self
            .synthetic_symbol_sets
            .iter()
            .zip(self.synthetic_symbol_set_cells.iter())
            .map(move |(set, cells)| SymbolSetDecl {
                ast: set,
                file_ns: &[],
                cells,
                doc,
            });
        authored.chain(synthetic)
    }

    /// Resolve a [`TypeRef`] to either its built-in tag or the user-declared
    /// [`TypeDecl`] / [`UnionDecl`] it points to. `Named` refs are validated
    /// at [`Document::open`], so the lookup never fails here.
    ///
    /// Names resolve from the document's ROOT namespace. A reference written
    /// inside a namespaced file must resolve from *that* namespace instead —
    /// see [`Document::resolve_in`] and [`TypeField::resolved_type`](crate::TypeField::resolved_type).
    pub fn resolve<'a>(&'a self, t: &'a TypeRef) -> ResolvedType<'a> {
        self.resolve_in(t, &self.file_ns)
    }

    /// [`Document::resolve`] for a reference written in a source whose
    /// namespace is `file_ns`: the name resolves *within its declaring
    /// namespace first*. This is what keeps two same-named types in
    /// different namespaces apart — a user schema's `acme.Container` and
    /// wdoc's diagram `wdoc.Container` are both named `Container`, and
    /// resolving an `acme` field's type from the root namespace can answer
    /// the wrong one.
    pub fn resolve_in<'a>(&'a self, t: &'a TypeRef, file_ns: &[String]) -> ResolvedType<'a> {
        match t {
            TypeRef::Builtin(b) => ResolvedType::Builtin(*b),
            TypeRef::Named { path, .. } => {
                let fqn = self
                    .resolve_path_in(path, file_ns)
                    .expect("named ref validated at Document::open");
                let fqn_dotted = fqn.join(".");
                if let Some(decl) = self.type_decl(&fqn_dotted) {
                    ResolvedType::Named(decl)
                } else if let Some(iface) = self.interface(&fqn_dotted) {
                    ResolvedType::Interface(iface)
                } else if let Some(union) = self.union_decl(&fqn_dotted) {
                    ResolvedType::Union(union)
                } else if let Some(ss) = self.symbol_set(&fqn_dotted) {
                    ResolvedType::SymbolSet(ss)
                } else {
                    ResolvedType::Connection(
                        self.connection_decl(&fqn_dotted)
                            .expect("named ref validated at Document::open"),
                    )
                }
            }
            TypeRef::Reference(inner) => {
                ResolvedType::Reference(Box::new(self.resolve_in(inner, file_ns)))
            }
            TypeRef::List(inner) => ResolvedType::List(Box::new(self.resolve_in(inner, file_ns))),
            TypeRef::Tensor { element, dims } => ResolvedType::Tensor {
                element: Box::new(self.resolve_in(element, file_ns)),
                dims,
            },
            TypeRef::Function { params, return_ty } => ResolvedType::Function {
                params: params.iter().map(|p| self.resolve_in(p, file_ns)).collect(),
                return_ty: Box::new(self.resolve_in(return_ty, file_ns)),
            },
        }
    }

    /// Prefix a declared name with the root source's namespace.
    pub(in crate::doc) fn compose_fqn(&self, name: &[String]) -> Vec<String> {
        let mut v = self.file_ns.clone();
        v.extend(name.iter().cloned());
        v
    }

    /// The set of fully-qualified names declared anywhere in the
    /// document (root + every eagerly-imported file), used as the
    /// resolution registry for type references. Built once per document
    /// (see the `ref_registry` field for why that's sound).
    pub(in crate::doc) fn ref_registry(&self) -> &HashSet<Vec<String>> {
        self.ref_registry.get_or_init(|| self.build_ref_registry())
    }

    /// Collect every type FQN that some `&T` field references, so the
    /// reference-acceptance check can be answered by lookup.
    fn build_ref_registry(&self) -> HashSet<Vec<String>> {
        let mut registry: HashSet<Vec<String>> = self
            .ast
            .items
            .iter()
            .filter_map(|item| match item {
                ast::Item::TypeDecl(t) => Some(self.compose_fqn(&t.name)),
                ast::Item::InterfaceDecl(i) => Some(self.compose_fqn(&i.name)),
                ast::Item::UnionDecl(u) => Some(self.compose_fqn(&u.name)),
                ast::Item::SymbolSetDecl(s) => Some(self.compose_fqn(&s.name)),
                ast::Item::ConnectionDecl(c) => Some(self.compose_fqn(&c.name)),
                _ => None,
            })
            .collect();
        // Synthetic types live at the root namespace.
        for t in &self.synthetic_types {
            registry.insert(t.name.clone());
        }
        for set in &self.synthetic_symbol_sets {
            registry.insert(set.name.clone());
        }
        // Declarations from eagerly-imported files resolve too — e.g. a
        // connection whose endpoint type `&SvgBlock` is defined in an
        // imported schema file. Each source's symbol index already holds
        // FQNs with that file's namespace composed in.
        for src in self.all_sources() {
            for rec in src.symbols.iter() {
                if matches!(
                    rec.kind,
                    SymbolKind::TypeDecl
                        | SymbolKind::InterfaceDecl
                        | SymbolKind::UnionDecl
                        | SymbolKind::SymbolSetDecl
                        | SymbolKind::ConnectionDecl
                ) {
                    registry.insert(rec.fqn.split('.').map(str::to_string).collect());
                }
            }
        }
        registry
    }
}
