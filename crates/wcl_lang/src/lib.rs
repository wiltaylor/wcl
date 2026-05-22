//! WCL language library.
//!
//! Parse a WCL source string into a [`Document`]. Fields are evaluated lazily
//! on first access and cached; the document is otherwise immutable.

pub(crate) mod ast;
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
mod symbols;
mod value;

pub use ast::Span;
pub use builtins::{
    BuiltinFn, Caller, FromValue, IntoBuiltin, IntoValue, IntoValueResult, from_fn,
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
