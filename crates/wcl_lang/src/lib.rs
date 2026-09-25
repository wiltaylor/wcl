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
//!   edit, reopen the file as a `Document`. [`parse_for_edit_recovering`]
//!   is its editor-facing twin: it keeps going past syntax errors and
//!   returns the items that parsed alongside every error.
//!
//! There is no AST escape hatch on `Document`; mixing edit + evaluate inside
//! one process state would silently invalidate the document's cell caches,
//! so the API forces the host to pick one mode per parse.
//!
//! Two tools are built over those paths. [`edit::set_field`] finds a field
//! through a `Document` and rewrites it through the edit path, and
//! [`diff::diff_documents`] compares two documents by what they evaluate to.
//!
//! # API stability
//!
//! The language is pre-1.0 and still gaining forms, errors and kinds, so the
//! enums that grow with it are `#[non_exhaustive]`. A `match` on one outside
//! this crate needs a wildcard arm, and a new variant is then a minor
//! release rather than a breaking one:
//!
//! - errors: [`EvalError`], [`ParseError`], [`SchemaViolationKind`],
//!   [`ArithmeticFault`], [`edit::EditError`];
//! - values and the syntax tree: [`Value`], [`ast::Item`], [`ast::Expr`],
//!   [`ast::Pattern`], [`TypeRef`], [`BuiltinType`], [`ast::BinOp`],
//!   [`ast::UnaryOp`], [`ast::UseForm`], [`ast::VariantBody`],
//!   [`ast::Trivia`], [`TokenKind`], [`NumberLit`];
//! - document views and kinds: [`ResolvedType`], [`FieldShape`],
//!   [`DataKind`], [`ChildKind`], [`VariantBodyView`], [`UseFormView`],
//!   [`SymbolKind`], [`ProfileKey`];
//! - diffs: [`diff::ChangeOp`], [`diff::FieldKind`], [`diff::Skipped`].
//!
//! Report structs this crate builds and hosts only read —
//! [`SyntaxError`], [`Profile`], [`ProfileNode`], [`SymbolHit`],
//! [`PartialParse`], and [`diff::Diff`] with its [`diff::Change`],
//! [`diff::FieldChange`] and [`diff::DiffWarning`] — are `#[non_exhaustive]`
//! too, so they can gain fields; read their fields, but do not build or
//! destructure them without `..`. Closed sets stay exhaustive
//! ([`diff::Side`], the unit/positional/record argument shapes, the string
//! encodings), and so do the AST structs and [`format::FormatConfig`], which
//! hosts build with literals.

pub mod ast;
/// What the language reports about a run rather than computes from one:
/// the error types and the opt-in evaluation profiler.
mod diagnostics;
pub mod diff;
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
/// The remaining-stack guard under the depth caps.
mod stack;
/// The name index built during parsing.
mod symbols;
/// Runtime values produced by evaluation. The types that *describe*
/// them are syntax, and live in [`ast::types`](ast).
mod value;

pub use ast::{BuiltinType, Span, TensorDim, TypeRef};
pub use diagnostics::{
    ArithmeticFault, EvalError, MAX_SYNTAX_ERRORS, ParseError, SchemaViolationKind, SyntaxError,
};
pub use diagnostics::{Profile, ProfileKey, ProfileNode};
pub use doc::{
    Block, ChildKind, Connection, ConnectionDecl, DataKind, DataRef, DeclName, DeclaresKind,
    Decorator, Document, Field, FieldShape, FileLoader, InterfaceDecl, MAX_EXPANDED_BLOCKS,
    NamedArg, Registry, ResolvedType, RowView, SYSTEM_IMPORT_ROOT, SymbolEntry, SymbolHit,
    SymbolSetDecl, TableView, TypeDecl, TypeField, UnionDecl, UnionVariant, UseDeclView,
    UseFormView, UseItem, VariantBodyView, disk_loader, overlay_loader, system_import_key,
};
pub use edit::{PartialParse, parse_expr, parse_for_edit, parse_for_edit_recovering};
pub use environment::{
    BuiltType, DecoratorBuilder, Environment, Expander, TypeBuilder, TypeFieldBuilder,
};
pub use functions::{
    BuiltinFn, Caller, DataPath, FromValue, IntoBuiltin, IntoValue, IntoValueResult, from_fn,
};
pub use lexer::{
    LexError, Lexer, NumberLit, StringEncoding, StringLit, StringPart, Token, TokenKind,
    is_identifier, is_keyword,
};
pub use symbols::{SymbolIndex, SymbolKind, SymbolPath, SymbolRecord};
pub use value::{FnParam, FnValue, Value, VariantPayload};
