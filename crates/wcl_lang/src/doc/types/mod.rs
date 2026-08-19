//! The type system: what a type name points at, and what inhabits it.
//!
//! [`crate::ast`] holds types as *written* — `TypeRef` is syntax, and it
//! knows nothing about the document it was written in. This module holds
//! types as *resolved*: it takes a `TypeRef` plus the namespace it was
//! written in and answers the questions the rest of the document layer
//! asks about it.
//!
//! - [`resolve`] — which declaration does this name point at? Alias-free
//!   name resolution, the declaration lookups (`type_decl`, `union_decl`,
//!   …) and the FQN registry they consult.
//! - [`alias`] — `type Port = u16`. Peeling a transparent alias, and the
//!   `@unit` factors that ride on the chain.
//! - [`inherit`] — `extends`. The effective field set of a type or
//!   interface, walked through its ancestors.
//! - [`interfaces`] — structural conformance: does this concrete type
//!   provide what the interface requires?
//! - [`inhabit`] — does this *value* belong to this type, and can it be
//!   coerced into it?
//! - [`variant_dispatch`] / [`unions`] — projecting a value onto a union
//!   variant by shape, and the declaration-time checks that keep those
//!   shapes unambiguous.
//!
//! What is deliberately *not* here: whether a **block** satisfies its
//! schema. Unknown fields, child quotas, table-row arity and decorator
//! cardinality are structure rather than type identity, and stay in
//! [`schema_check`](super::schema_check), [`schema_lookup`](super::schema_lookup)
//! and [`decorators`](super::decorators) — all of which are consumers of
//! this module.

mod alias;
mod inhabit;
mod inherit;
mod interfaces;
mod resolve;
mod unions;
pub(super) mod variant_dispatch;

use crate::ast::{BuiltinType, TensorDim};

use super::Document;
use super::views::{ConnectionDecl, InterfaceDecl, SymbolSetDecl, TypeDecl, UnionDecl};

pub(super) use inhabit::{
    coerce_value_to_type, symbol_set_membership_error_in, value_matches_declared,
    value_matches_type_ref,
};
pub(super) use inherit::{
    build_effective_fields, build_merged_decorators, is_descendant_of_walk, lookup_effective_field,
};
pub(super) use interfaces::{check_interface_conformance, dataref_concrete_type, same_type_decl};
pub(super) use unions::{format_union_variants_hint, validate_union};

/// What one type name points at, resolved a single link at a time.
///
/// This answers "what is this name?", not "what shape does this field
/// have" — a transparent alias resolves to [`ResolvedType::Named`]
/// carrying the alias declaration, not to its target. Use
/// [`FieldShape`] when you want the alias peeled.
#[derive(Debug)]
pub enum ResolvedType<'a> {
    /// A builtin scalar (`utf8`, `u32`, `bool`, …).
    Builtin(BuiltinType),
    /// A declared `type` — possibly an alias; see the note above.
    Named(TypeDecl<'a>),
    /// A declared `interface`.
    Interface(InterfaceDecl<'a>),
    /// A declared `union`.
    Union(UnionDecl<'a>),
    /// A declared `symbol_set`.
    SymbolSet(SymbolSetDecl<'a>),
    /// A declared `connection`.
    Connection(ConnectionDecl<'a>),
    /// `&T` — a reference to the wrapped type.
    Reference(Box<ResolvedType<'a>>),
    /// `list<T>` — the wrapped type is the element.
    List(Box<ResolvedType<'a>>),
    /// `tensor<T, [dims]>`.
    Tensor {
        /// The element type.
        element: Box<ResolvedType<'a>>,
        /// The declared dimensions.
        dims: &'a [TensorDim],
    },
    /// A function type.
    Function {
        /// Parameter types, in order.
        params: Vec<ResolvedType<'a>>,
        /// The return type.
        return_ty: Box<ResolvedType<'a>>,
    },
}

/// How many alias links a walk down an alias chain will peel before
/// giving up — the cap that stops a cycle (`type A = B  type B = A`,
/// which parses) from hanging the walk. Shared by every walker, so the
/// document can't disagree with itself about how deep an alias goes.
pub(crate) const ALIAS_DEPTH: u8 = 8;

/// The **shape** of a schema field's declared type: what a consumer needs
/// in order to decide how to treat the field, without printing the type
/// and matching on the text.
///
/// [`ResolvedType`] answers "what does this name point at", one link at a
/// time. A shape answers the question a form, a graph or a validator
/// actually asks — is this field one scalar, a list of them, a closed
/// vocabulary, a nested block, a function? Two differences from rendering
/// [`TypeField::type_ref`](super::TypeField::type_ref) and comparing the text matter, because both of
/// them fail *silently* — the field simply stops being whatever the
/// consumer was classifying it as:
///
/// - **Aliases are transparent.** A field declared `id: NodeId`, where
///   `type NodeId = identifier`, has the shape `Scalar(Identifier)` —
///   what the field *is*. Its rendering is `NodeId`, which matches no
///   string a consumer would have thought to write.
/// - **Names can't collide with syntax.** A field typed by a declaration
///   named `fnord` is a `Block`; `starts_with("fn")` on the rendering
///   says it is a function.
#[derive(Debug, Clone)]
pub enum FieldShape<'a> {
    /// One builtin scalar — `utf8`, `identifier`, `bool`, `u32`, …
    Scalar(BuiltinType),
    /// A closed vocabulary (`symbol_set`).
    Symbols(SymbolSetDecl<'a>),
    /// A declared record / block type.
    Block(TypeDecl<'a>),
    /// A declared `interface`.
    Interface(InterfaceDecl<'a>),
    /// A declared `union`.
    Union(UnionDecl<'a>),
    /// A declared `connection`.
    Connection(ConnectionDecl<'a>),
    /// `list<T>`, carrying the element's shape.
    List(Box<FieldShape<'a>>),
    /// `&T`, carrying the referent's shape.
    Reference(Box<FieldShape<'a>>),
    /// `tensor<T, [...]>`, carrying the element's shape.
    Tensor(Box<FieldShape<'a>>),
    /// A function-valued field. The signature is not part of the shape —
    /// ask [`TypeField::resolved_type`](super::TypeField::resolved_type) for it.
    Function,
    /// An alias chain too long or too tangled to peel ([`ALIAS_DEPTH`]),
    /// carrying the link the walk stopped on. The declaration is an
    /// alias, so it is not a [`Block`](FieldShape::Block); what it stands
    /// for is unknown, so it is nothing else either. Saying so is the
    /// point — a shape that guessed here would misclassify the field in
    /// exactly the silence this type exists to end.
    Unresolved(TypeDecl<'a>),
}

impl<'a> FieldShape<'a> {
    /// The builtin behind a scalar field; `None` for every other shape,
    /// containers included. "This field holds one builtin scalar" is the
    /// question — a `list<utf8>` is not a `utf8`.
    pub fn builtin(&self) -> Option<BuiltinType> {
        match self {
            FieldShape::Scalar(b) => Some(*b),
            _ => None,
        }
    }

    /// The element shape of a `list<T>`; `None` for every other shape.
    pub fn list_element(&self) -> Option<&FieldShape<'a>> {
        match self {
            FieldShape::List(inner) => Some(inner),
            _ => None,
        }
    }

    /// Whether the field holds a function.
    pub fn is_function(&self) -> bool {
        matches!(self, FieldShape::Function)
    }

    /// Read a resolved type as a shape, peeling transparent type aliases
    /// (`type Port = u16`) as it goes. Only alias links spend `depth`:
    /// `list<list<Port>>` is as peelable as `Port`.
    pub(in crate::doc) fn from_resolved(
        doc: &'a Document,
        ty: ResolvedType<'a>,
        depth: u8,
    ) -> Self {
        let nest = |inner: Box<ResolvedType<'a>>| Box::new(Self::from_resolved(doc, *inner, depth));
        match ty {
            ResolvedType::Builtin(b) => FieldShape::Scalar(b),
            ResolvedType::SymbolSet(ss) => FieldShape::Symbols(ss),
            ResolvedType::Interface(i) => FieldShape::Interface(i),
            ResolvedType::Union(u) => FieldShape::Union(u),
            ResolvedType::Connection(c) => FieldShape::Connection(c),
            ResolvedType::Named(d) => match d.ast.alias.as_ref() {
                // The alias target resolves in the ALIAS's namespace, not
                // the field's — `type Port = u16` means what it meant
                // where it was written.
                Some(_) if depth == 0 => FieldShape::Unresolved(d),
                Some(target) => {
                    Self::from_resolved(doc, doc.resolve_in(target, d.file_ns), depth - 1)
                }
                None => FieldShape::Block(d),
            },
            ResolvedType::List(inner) => FieldShape::List(nest(inner)),
            ResolvedType::Reference(inner) => FieldShape::Reference(nest(inner)),
            ResolvedType::Tensor { element, .. } => FieldShape::Tensor(nest(element)),
            ResolvedType::Function { .. } => FieldShape::Function,
        }
    }
}
