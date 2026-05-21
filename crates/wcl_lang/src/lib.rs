//! WCL language library.
//!
//! Parse a WCL source string into a [`Document`]. Fields are evaluated lazily
//! on first access and cached; the document is otherwise immutable.

pub(crate) mod ast;
mod doc;
mod error;
mod lexer;
mod numeric;
mod parser;
mod schema;
mod value;

pub use ast::Span;
pub use doc::{
    Block, Decorator, Document, Field, NamedArg, ResolvedType, SymbolEntry, SymbolSetDecl,
    TypeDecl, TypeField, UnionDecl, UnionVariant, UseDeclView, UseFormView, UseItem,
    VariantBodyView,
};
pub use error::{EvalError, ParseError, SyntaxError};
pub use schema::{BuiltType, DecoratorBuilder, SchemaRegistry, TypeBuilder, TypeFieldBuilder};
pub use value::{BuiltinType, TensorDim, TypeRef, Value};
