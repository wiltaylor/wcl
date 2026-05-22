//! WCL language library.
//!
//! Two entry points, mutually exclusive by design:
//!
//! - **Evaluating path** — [`Document::open`] / [`Document::open_with`] parse a
//!   source string and return a lazy, evaluation-only view. Fields evaluate
//!   on first access and cache; the document is otherwise immutable.
//! - **Editing path** — [`parse_for_edit`] returns an owned [`ast::Source`]
//!   with public fields. Hosts inspect or mutate the AST, then write it back
//!   to a `.wcl` file (a source printer lands in a later phase). To
//!   evaluate after editing, reopen the file as a `Document`.
//!
//! There is no AST escape hatch on `Document`; mixing edit + evaluate inside
//! one process state would silently invalidate the document's cell caches,
//! so the API forces the host to pick one mode per parse.

pub mod ast;
mod builtins;
mod collections;
mod data;
mod doc;
mod environment;
mod error;
mod lexer;
mod numeric;
mod parser;
mod profile;
mod reflect;
mod symbols;
mod value;

pub use ast::Span;
pub use builtins::{
    BuiltinFn, Caller, DataPath, FromValue, IntoBuiltin, IntoValue, IntoValueResult, from_fn,
};
pub use data::{DataKind, DataRef};
pub use doc::{
    Block, ChildKind, Connection, ConnectionDecl, DeclName, Decorator, Document, Field,
    InterfaceDecl, NamedArg, ResolvedType, RowView, SymbolEntry, SymbolSetDecl, TableView,
    TypeDecl, TypeField, UnionDecl, UnionVariant, UseDeclView, UseFormView, UseItem,
    VariantBodyView,
};
pub use environment::{BuiltType, DecoratorBuilder, Environment, TypeBuilder, TypeFieldBuilder};
pub use error::{EvalError, ParseError, SchemaViolationKind, SyntaxError};
pub use profile::{Profile, ProfileKey, ProfileNode};
pub use symbols::{SymbolIndex, SymbolKind, SymbolPath, SymbolRecord};
pub use value::{BuiltinType, FnParam, FnValue, TensorDim, TypeRef, Value, VariantPayload};

/// Parse a WCL source string into an owned [`ast::Source`] for inspection
/// or mutation. The returned AST has fully `pub` fields; hosts may walk
/// it, edit it, and (once the source printer ships) write it back to a
/// `.wcl` file.
///
/// This is the **edit-path** entry point. It performs *no* evaluation,
/// schema checks, or import resolution — those happen only when a
/// [`Document`] is opened from the (post-edit) file. The two paths are
/// deliberately disjoint so AST mutations can't invalidate a
/// Document's cached fields silently.
///
/// `name` is used for diagnostics only (it becomes the
/// `NamedSource` label on any [`ParseError`]).
pub fn parse_for_edit(source: &str, name: impl Into<String>) -> Result<ast::Source, ParseError> {
    parser::Parser::new(source, name)
        .parse_source()
        .map(|(src, _idx)| src)
}
