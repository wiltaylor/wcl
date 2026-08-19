//! WCL language library.
//!
//! Two entry points, mutually exclusive by design:
//!
//! - **Evaluating path** — [`Document::open`] / [`Document::open_with`] parse a
//!   source string and return a lazy, evaluation-only view. Fields evaluate
//!   on first access and cache; the document is otherwise immutable.
//! - **Editing path** — [`parse_for_edit`] returns an owned [`ast::Source`]
//!   with public fields. Hosts inspect or mutate the AST. They then print it
//!   back to a `.wcl` file with [`format::to_source`]. To evaluate after an
//!   edit, reopen the file as a `Document`.
//!
//! There is no AST escape hatch on `Document`; mixing edit + evaluate inside
//! one process state would silently invalidate the document's cell caches,
//! so the API forces the host to pick one mode per parse.

pub mod ast;
/// What the language reports about a run rather than computes from one:
/// the error types and the opt-in evaluation profiler.
mod diagnostics;
/// The document model: opening, evaluating and querying a file.
mod doc;
pub mod edit;
mod environment;
pub mod format;
mod functions;
mod lexer;
mod numeric;
/// The recursive-descent parser: tokens in, syntax tree out.
mod parser;
/// The name index built during parsing.
mod symbols;
/// Runtime values produced by evaluation. The types that *describe*
/// them are syntax, and live in [`ast::types`](ast).
mod value;

pub use ast::{BuiltinType, Span, TensorDim, TypeRef};
pub use diagnostics::{ArithmeticFault, EvalError, ParseError, SchemaViolationKind, SyntaxError};
pub use diagnostics::{Profile, ProfileKey, ProfileNode};
pub use doc::{
    Block, ChildKind, Connection, ConnectionDecl, DataKind, DataRef, DeclName, DeclaresKind,
    Decorator, Document, Field, FieldShape, FileLoader, InterfaceDecl, NamedArg, Registry,
    ResolvedType, RowView, SYSTEM_IMPORT_ROOT, SymbolEntry, SymbolHit, SymbolSetDecl, TableView,
    TypeDecl, TypeField, UnionDecl, UnionVariant, UseDeclView, UseFormView, UseItem,
    VariantBodyView, disk_loader, overlay_loader, system_import_key,
};
pub use edit::{parse_expr, parse_for_edit};
pub use environment::{
    BuiltType, DecoratorBuilder, Environment, Expander, TypeBuilder, TypeFieldBuilder,
};
pub use functions::{
    BuiltinFn, Caller, DataPath, FromValue, IntoBuiltin, IntoValue, IntoValueResult, from_fn,
};
pub use lexer::{
    LexError, Lexer, NumberLit, StringEncoding, StringLit, StringPart, Token, TokenKind,
    is_identifier,
};
pub use symbols::{SymbolIndex, SymbolKind, SymbolPath, SymbolRecord};
pub use value::{FnParam, FnValue, Value, VariantPayload};
