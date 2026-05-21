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
mod value;

pub use ast::Span;
pub use doc::{Block, Document, Field, TypeDecl, TypeField};
pub use error::{EvalError, ParseError, SyntaxError};
pub use value::{BuiltinType, TypeRef, Value};
